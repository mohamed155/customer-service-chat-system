use crate::contract::*;
use futures::StreamExt;
use reqwest::StatusCode;

#[derive(Clone)]
pub struct AnthropicAdapter {
    client: reqwest::Client,
    base_url: String,
}

impl AnthropicAdapter {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

fn category_from_status(status: StatusCode) -> ErrorCategory {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ErrorCategory::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ErrorCategory::RateLimited,
        s if s.is_client_error() => ErrorCategory::InvalidRequest,
        s if s.is_server_error() => ErrorCategory::Unavailable,
        _ => ErrorCategory::Unavailable,
    }
}

fn provider_error(status: StatusCode, detail: String) -> ProviderError {
    let category = category_from_status(status);
    let retriable = category.retriable();
    ProviderError {
        category,
        retriable,
        detail,
    }
}

fn normalize_error(e: reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError {
            category: ErrorCategory::Timeout,
            retriable: true,
            detail: "request timed out".into(),
        }
    } else if e.is_connect() {
        ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: "connection error".into(),
        }
    } else {
        ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: "request failed".into(),
        }
    }
}

fn provider_error_from_body(status: StatusCode, body: &str) -> ProviderError {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            body.chars()
                .filter(|c| {
                    c.is_alphanumeric()
                        || c.is_ascii_whitespace()
                        || *c == '.'
                        || *c == '_'
                        || *c == '-'
                })
                .take(200)
                .collect::<String>()
        });
    provider_error(status, detail)
}

#[derive(serde::Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AnthropicRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "is_false")]
    stream: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(serde::Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(serde::Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    model: String,
    usage: AnthropicUsage,
    stop_reason: Option<String>,
}

#[async_trait::async_trait]
impl ChatProvider for AnthropicAdapter {
    async fn complete(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError> {
        let messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| AnthropicMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect();

        let body = AnthropicRequest {
            model: req.model.clone(),
            system: req.system.clone(),
            messages,
            max_tokens: req.max_output_tokens.unwrap_or(4096),
            temperature: req.temperature,
            stream: false,
        };

        let elapsed = std::time::Instant::now();

        let mut req_builder = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", key.expose())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let resp = req_builder.send().await.map_err(normalize_error)?;

        let status = resp.status();
        tracing::info!(
            provider = "anthropic",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "anthropic complete response"
        );

        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(provider_error_from_body(status, &body_text));
        }

        let anthropic_resp: AnthropicResponse = resp.json().await.map_err(|e| ProviderError {
            category: ErrorCategory::InvalidRequest,
            retriable: false,
            detail: format!("response parse: {e}"),
        })?;

        let finish = match anthropic_resp.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            _ => FinishReason::Other,
        };

        tracing::info!(
            provider = "anthropic",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            "anthropic complete succeeded"
        );

        Ok(ChatCompletion {
            content: anthropic_resp
                .content
                .into_iter()
                .next()
                .map(|c| c.text)
                .unwrap_or_default(),
            model: anthropic_resp.model,
            usage: TokenUsage {
                input: anthropic_resp.usage.input_tokens,
                output: anthropic_resp.usage.output_tokens,
            },
            finish,
        })
    }

    async fn stream(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatStream, ProviderError> {
        let messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| AnthropicMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect();

        let body = AnthropicRequest {
            model: req.model.clone(),
            system: req.system.clone(),
            messages,
            max_tokens: req.max_output_tokens.unwrap_or(4096),
            temperature: req.temperature,
            stream: true,
        };

        let mut req_builder = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", key.expose())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let resp = req_builder.send().await.map_err(normalize_error)?;

        let status = resp.status();
        tracing::info!(
            provider = "anthropic",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "anthropic stream response"
        );

        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(provider_error_from_body(status, &body_text));
        }

        let stream = resp.bytes_stream().boxed();
        let frame_stream = crate::sse::sse_frames(stream);

        Ok(Box::pin(anthropic_sse_to_events(frame_stream)) as ChatStream)
    }
}

fn anthropic_sse_to_events(
    frame_stream: impl futures::Stream<Item = Result<crate::sse::SseFrame, ProviderError>>
        + Send
        + 'static,
) -> impl futures::Stream<Item = Result<StreamEvent, ProviderError>> + Send + 'static {
    let input_tokens = std::sync::Arc::new(std::sync::Mutex::new(None::<u32>));
    let output_tokens = std::sync::Arc::new(std::sync::Mutex::new(None::<u32>));
    let model = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let finish = std::sync::Arc::new(std::sync::Mutex::new(FinishReason::Other));

    frame_stream.filter_map(move |frame_result| {
        let input_tokens = std::sync::Arc::clone(&input_tokens);
        let output_tokens = std::sync::Arc::clone(&output_tokens);
        let model = std::sync::Arc::clone(&model);
        let finish = std::sync::Arc::clone(&finish);
        async move {
            let frame = match frame_result {
                Ok(f) => f,
                Err(e) => return Some(Err(e)),
            };

            let event_type = frame.event.as_deref().unwrap_or("");

            match event_type {
                "message_start" => {
                    match serde_json::from_str::<serde_json::Value>(&frame.data) {
                        Ok(json) => {
                            if let Some(msg) = json.get("message") {
                                *input_tokens.lock().unwrap() =
                                    msg["usage"]["input_tokens"].as_u64().map(|v| v as u32);
                                *model.lock().unwrap() = msg["model"].as_str().map(|s| s.to_string());
                            }
                            None
                        }
                        Err(e) => Some(Err(ProviderError {
                            category: ErrorCategory::InvalidRequest,
                            retriable: false,
                            detail: format!("malformed message_start frame: {e}"),
                        })),
                    }
                }
                "content_block_delta" => {
                    match serde_json::from_str::<serde_json::Value>(&frame.data) {
                        Ok(json) => {
                            match json["delta"]["text"].as_str() {
                                Some(text) if !text.is_empty() => {
                                    Some(Ok(StreamEvent::Delta(text.to_string())))
                                }
                                Some(_) => None,
                                None => Some(Err(ProviderError {
                                    category: ErrorCategory::InvalidRequest,
                                    retriable: false,
                                    detail: "malformed content_block_delta frame: missing or non-string delta.text".into(),
                                })),
                            }
                        }
                        Err(e) => Some(Err(ProviderError {
                            category: ErrorCategory::InvalidRequest,
                            retriable: false,
                            detail: format!("malformed content_block_delta frame: {e}"),
                        })),
                    }
                }
                "message_delta" => {
                    match serde_json::from_str::<serde_json::Value>(&frame.data) {
                        Ok(json) => {
                            *output_tokens.lock().unwrap() =
                                json["usage"]["output_tokens"].as_u64().map(|v| v as u32);
                            if let Some(stop) = json["delta"]["stop_reason"].as_str() {
                                *finish.lock().unwrap() = match stop {
                                    "end_turn" => FinishReason::Stop,
                                    "max_tokens" => FinishReason::Length,
                                    _ => FinishReason::Other,
                                };
                            }
                            None
                        }
                        Err(e) => Some(Err(ProviderError {
                            category: ErrorCategory::InvalidRequest,
                            retriable: false,
                            detail: format!("malformed message_delta frame: {e}"),
                        })),
                    }
                }
                "message_stop" => {
                    let it = input_tokens.lock().unwrap().take();
                    let ot = output_tokens.lock().unwrap().take();
                    let m = model.lock().unwrap().take().unwrap_or_default();
                    let f = finish.lock().unwrap().clone();
                    Some(Ok(StreamEvent::Done {
                        usage: TokenUsage {
                            input: it,
                            output: ot,
                        },
                        model: m,
                        finish: f,
                    }))
                }
                _ => None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn test_request() -> ChatRequest {
        ChatRequest {
            system: Some("You are a helpful assistant.".into()),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "ignore this system message".into(),
                },
                Message {
                    role: Role::User,
                    content: "Hello".into(),
                },
            ],
            model: "claude-sonnet-4-20250514".into(),
            max_output_tokens: None,
            temperature: Some(0.7),
            request_id: None,
        }
    }

    fn test_adapter(mock_server: &MockServer) -> AnthropicAdapter {
        AnthropicAdapter::new(reqwest::Client::new(), mock_server.uri())
    }

    fn success_body() -> serde_json::Value {
        serde_json::json!({
            "content": [{"text": "Hi there!", "type": "text"}],
            "id": "msg_01",
            "model": "claude-sonnet-4-20250514",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "type": "message",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 5
            }
        })
    }

    #[tokio::test]
    async fn test_happy_path() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
            .expect(1)
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let result = adapter.complete(&key, &req).await.unwrap();

        assert_eq!(result.content, "Hi there!");
        assert_eq!(result.model, "claude-sonnet-4-20250514");
        assert_eq!(result.usage.input, Some(12));
        assert_eq!(result.usage.output, Some(5));
        assert_eq!(result.finish, FinishReason::Stop);
    }

    #[tokio::test]
    async fn test_request_body_format() {
        let mock = MockServer::start().await;

        let expected_body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "maxTokens": 4096,
            "temperature": 0.7
        });

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(wiremock::matchers::body_json(expected_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
            .expect(1)
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        adapter.complete(&key, &req).await.unwrap();
    }

    #[tokio::test]
    async fn test_missing_usage_returns_none() {
        let mock = MockServer::start().await;

        let body = serde_json::json!({
            "content": [{"text": "Hi", "type": "text"}],
            "id": "msg_02",
            "model": "claude-sonnet-4-20250514",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "type": "message",
            "usage": {}
        });

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let result = adapter.complete(&key, &req).await.unwrap();

        assert_eq!(result.usage.input, None);
        assert_eq!(result.usage.output, None);
    }

    #[tokio::test]
    async fn test_401_authentication_error() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"message": "Invalid API key", "type": "authentication_error"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-bad-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::Authentication);
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn test_429_rate_limited() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {"message": "Rate limit exceeded"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::RateLimited);
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn test_500_unavailable() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {"message": "Internal server error"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::Unavailable);
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn test_400_invalid_request() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"message": "Invalid request body"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::InvalidRequest);
    }

    #[tokio::test]
    async fn test_stream_sse() {
        use futures::StreamExt;

        let sse_body = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"model\":\"claude-sonnet-4-20250514\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi there!\"}}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":5}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_body))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let mut stream = adapter.stream(&key, &req).await.unwrap();

        let first = stream.next().await.unwrap().unwrap();
        match first {
            StreamEvent::Delta(text) => assert_eq!(text, "Hi there!"),
            _ => panic!("expected Delta"),
        }

        let second = stream.next().await.unwrap().unwrap();
        match second {
            StreamEvent::Done {
                usage,
                model,
                finish,
            } => {
                assert_eq!(usage.input, Some(12));
                assert_eq!(usage.output, Some(5));
                assert_eq!(model, "claude-sonnet-4-20250514");
                assert_eq!(finish, FinishReason::Stop);
            }
            _ => panic!("expected Done"),
        }

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_403_authentication_error() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": {"message": "Forbidden", "type": "authentication_error"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-bad-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::Authentication);
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn test_404_maps_to_invalid_request() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": {"message": "Not found"}
            })))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::InvalidRequest);
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn test_non_json_error_body_has_limited_detail() {
        let mock = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("raw error text"))
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-bad-key".into());
        let req = test_request();
        let err = adapter.complete(&key, &req).await.unwrap_err();

        assert_eq!(err.category, ErrorCategory::Authentication);
        // Should NOT contain the full body or request details
        assert!(err.detail.len() < 100, "detail too long: {}", err.detail);
    }

    #[tokio::test]
    async fn test_max_tokens_defaults_to_4096() {
        let mock = MockServer::start().await;

        let expected_body = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "maxTokens": 4096,
            "temperature": 0.7
        });

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .and(wiremock::matchers::body_json(expected_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
            .expect(1)
            .mount(&mock)
            .await;

        let adapter = test_adapter(&mock);
        let key = SecretKey::new("sk-test-key".into());
        let req = test_request(); // max_output_tokens: None
        adapter.complete(&key, &req).await.unwrap();
    }
}
