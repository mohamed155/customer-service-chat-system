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
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    content: serde_json::Value,
}

#[derive(serde::Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    #[serde(rename = "input_schema")]
    input_schema: serde_json::Value,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(serde::Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
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
            .map(|m| {
                let role = match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                    Role::Tool => "user".to_string(),
                };
                let content = if m.role == Role::Assistant && !m.tool_calls.is_empty() {
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": m.content
                        }));
                    }
                    for tc in &m.tool_calls {
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments
                        }));
                    }
                    serde_json::Value::Array(blocks)
                } else if m.role == Role::Tool {
                    serde_json::Value::Array(vec![serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": m.tool_call_id,
                        "content": m.content
                    })])
                } else {
                    serde_json::Value::String(m.content.clone())
                };
                AnthropicMessage { role, content }
            })
            .collect();

        let tools: Vec<AnthropicTool> = req
            .tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        let body = AnthropicRequest {
            model: req.model.clone(),
            system: req.system.clone(),
            messages,
            max_tokens: req.max_output_tokens.unwrap_or(4096),
            temperature: req.temperature,
            stream: false,
            tools,
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
            Some("tool_use") => FinishReason::ToolUse,
            _ => FinishReason::Other,
        };

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        for block in anthropic_resp.content {
            match block {
                AnthropicContentBlock::Text { text } => {
                    content.push_str(&text);
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        tracing::info!(
            provider = "anthropic",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            "anthropic complete succeeded"
        );

        Ok(ChatCompletion {
            content,
            model: anthropic_resp.model,
            usage: TokenUsage {
                input: anthropic_resp.usage.input_tokens,
                output: anthropic_resp.usage.output_tokens,
            },
            finish,
            tool_calls,
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
            .map(|m| {
                let role = match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => unreachable!(),
                    Role::Tool => "user".to_string(),
                };
                let content = if m.role == Role::Assistant && !m.tool_calls.is_empty() {
                    let mut blocks = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": m.content
                        }));
                    }
                    for tc in &m.tool_calls {
                        blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments
                        }));
                    }
                    serde_json::Value::Array(blocks)
                } else if m.role == Role::Tool {
                    serde_json::Value::Array(vec![serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": m.tool_call_id,
                        "content": m.content
                    })])
                } else {
                    serde_json::Value::String(m.content.clone())
                };
                AnthropicMessage { role, content }
            })
            .collect();

        let tools: Vec<AnthropicTool> = req
            .tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        let body = AnthropicRequest {
            model: req.model.clone(),
            system: req.system.clone(),
            messages,
            max_tokens: req.max_output_tokens.unwrap_or(4096),
            temperature: req.temperature,
            stream: true,
            tools,
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
    #[derive(Default)]
    struct ToolCallAccum {
        id: String,
        name: String,
        arguments: String,
    }

    let input_tokens = std::sync::Arc::new(std::sync::Mutex::new(None::<u32>));
    let output_tokens = std::sync::Arc::new(std::sync::Mutex::new(None::<u32>));
    let model = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let finish = std::sync::Arc::new(std::sync::Mutex::new(FinishReason::Other));
    let tc_accs: std::sync::Arc<
        std::sync::Mutex<std::collections::BTreeMap<usize, ToolCallAccum>>,
    > = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new()));

    frame_stream.flat_map(move |frame_result| {
        let input_tokens = std::sync::Arc::clone(&input_tokens);
        let output_tokens = std::sync::Arc::clone(&output_tokens);
        let model = std::sync::Arc::clone(&model);
        let finish = std::sync::Arc::clone(&finish);
        let tc_accs = std::sync::Arc::clone(&tc_accs);

        let events: Vec<Result<StreamEvent, ProviderError>> = match frame_result {
            Ok(frame) => {
                let event_type = frame.event.as_deref().unwrap_or("").to_string();

                match event_type.as_str() {
                    "message_start" => {
                        match serde_json::from_str::<serde_json::Value>(&frame.data) {
                            Ok(json) => {
                                if let Some(msg) = json.get("message") {
                                    *input_tokens.lock().unwrap() =
                                        msg["usage"]["input_tokens"].as_u64().map(|v| v as u32);
                                    *model.lock().unwrap() =
                                        msg["model"].as_str().map(|s| s.to_string());
                                }
                                vec![]
                            }
                            Err(e) => vec![Err(ProviderError {
                                category: ErrorCategory::InvalidRequest,
                                retriable: false,
                                detail: format!("malformed message_start frame: {e}"),
                            })],
                        }
                    }
                    "content_block_start" => {
                        match serde_json::from_str::<serde_json::Value>(&frame.data) {
                            Ok(json) => {
                                if json["content_block"]["type"].as_str() == Some("tool_use") {
                                    let index = json["index"].as_i64().unwrap_or(0) as usize;
                                    let id = json["content_block"]["id"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    let name = json["content_block"]["name"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    tc_accs.lock().unwrap().insert(
                                        index,
                                        ToolCallAccum {
                                            id,
                                            name,
                                            arguments: String::new(),
                                        },
                                    );
                                }
                                vec![]
                            }
                            Err(e) => vec![Err(ProviderError {
                                category: ErrorCategory::InvalidRequest,
                                retriable: false,
                                detail: format!("malformed content_block_start frame: {e}"),
                            })],
                        }
                    }
                    "content_block_delta" => {
                        match serde_json::from_str::<serde_json::Value>(&frame.data) {
                            Ok(json) => {
                                let delta_type = json["delta"]["type"].as_str().unwrap_or("");
                                if delta_type == "input_json_delta" {
                                    let index = json["index"].as_i64().unwrap_or(0) as usize;
                                    if let Some(partial) =
                                        json["delta"]["partial_delta_json"].as_str()
                                    {
                                        let mut accs = tc_accs.lock().unwrap();
                                        if let Some(acc) = accs.get_mut(&index) {
                                            acc.arguments.push_str(partial);
                                        }
                                    }
                                    vec![]
                                } else if delta_type == "text_delta" {
                                    match json["delta"]["text"].as_str() {
                                        Some(text) if !text.is_empty() => {
                                            vec![Ok(StreamEvent::Delta(text.to_string()))]
                                        }
                                        _ => vec![],
                                    }
                                } else {
                                    vec![]
                                }
                            }
                            Err(e) => vec![Err(ProviderError {
                                category: ErrorCategory::InvalidRequest,
                                retriable: false,
                                detail: format!("malformed content_block_delta frame: {e}"),
                            })],
                        }
                    }
                    "content_block_stop" => {
                        match serde_json::from_str::<serde_json::Value>(&frame.data) {
                            Ok(json) => {
                                let index = json["index"].as_i64().unwrap_or(0) as usize;
                                let acc_opt = tc_accs.lock().unwrap().remove(&index);
                                if let Some(acc) = acc_opt {
                                    let arguments = serde_json::from_str(&acc.arguments)
                                        .unwrap_or_else(|_| {
                                            serde_json::Value::Object(serde_json::Map::new())
                                        });
                                    vec![Ok(StreamEvent::ToolCall(ToolCall {
                                        id: acc.id,
                                        name: acc.name,
                                        arguments,
                                    }))]
                                } else {
                                    vec![]
                                }
                            }
                            Err(e) => vec![Err(ProviderError {
                                category: ErrorCategory::InvalidRequest,
                                retriable: false,
                                detail: format!("malformed content_block_stop frame: {e}"),
                            })],
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
                                        "tool_use" => FinishReason::ToolUse,
                                        _ => FinishReason::Other,
                                    };
                                }
                                vec![]
                            }
                            Err(e) => vec![Err(ProviderError {
                                category: ErrorCategory::InvalidRequest,
                                retriable: false,
                                detail: format!("malformed message_delta frame: {e}"),
                            })],
                        }
                    }
                    "message_stop" => {
                        let it = input_tokens.lock().unwrap().take();
                        let ot = output_tokens.lock().unwrap().take();
                        let m = model.lock().unwrap().take().unwrap_or_default();
                        let f = finish.lock().unwrap().clone();
                        vec![Ok(StreamEvent::Done {
                            usage: TokenUsage {
                                input: it,
                                output: ot,
                            },
                            model: m,
                            finish: f,
                        })]
                    }
                    _ => vec![],
                }
            }
            Err(e) => vec![Err(e)],
        };

        futures::stream::iter(events)
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
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                Message {
                    role: Role::User,
                    content: "Hello".into(),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ],
            model: "claude-sonnet-4-20250514".into(),
            max_output_tokens: None,
            temperature: Some(0.7),
            request_id: None,
            tools: vec![],
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

    #[tokio::test]
    async fn test_tool_call_complete() {
        let mock = MockServer::start().await;
        let adapter = test_adapter(&mock);

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [
                    {"type": "text", "text": "Let me check the weather..."},
                    {"type": "tool_use", "id": "toolu_abc123", "name": "get_weather", "input": {"location": "NYC"}}
                ],
                "id": "msg_01",
                "model": "claude-sonnet-4-20250514",
                "role": "assistant",
                "stop_reason": "tool_use",
                "stop_sequence": null,
                "type": "message",
                "usage": {"input_tokens": 15, "output_tokens": 10}
            })))
            .expect(1)
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test-key".into());
        let req = ChatRequest {
            system: Some("You are helpful.".into()),
            messages: vec![Message {
                role: Role::User,
                content: "What's the weather in NYC?".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            model: "claude-sonnet-4-20250514".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![ToolSpec {
                name: "get_weather".into(),
                description: "Get weather for a location".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }],
        };

        let result = adapter.complete(&key, &req).await.unwrap();

        assert_eq!(result.content, "Let me check the weather...");
        assert_eq!(result.finish, FinishReason::ToolUse);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "toolu_abc123");
        assert_eq!(result.tool_calls[0].name, "get_weather");
        assert_eq!(
            result.tool_calls[0].arguments,
            serde_json::json!({"location": "NYC"})
        );

        // Verify tools were sent in the request body
        let requests = mock.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert!(body.get("tools").is_some());
        assert_eq!(body["tools"][0]["name"], "get_weather");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    }

    #[tokio::test]
    async fn test_tool_call_stream() {
        use futures::StreamExt;

        let mock = MockServer::start().await;
        let adapter = test_adapter(&mock);

        let sse_body = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"model\":\"claude-sonnet-4-20250514\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":15,\"output_tokens\":0}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Let me check the weather...\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_abc\",\"name\":\"get_weather\",\"input\":{}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_delta_json\":\"{\\\"location\\\":\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_delta_json\":\" \\\"NYC\\\"}\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":1}

event: message_delta
data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":10},\"delta\":{\"stop_reason\":\"tool_use\"}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_body))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test-key".into());
        let req = ChatRequest {
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "What's the weather?".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            model: "claude-sonnet-4-20250514".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![],
        };

        let mut stream = adapter.stream(&key, &req).await.unwrap();
        let mut delta_found = false;
        let mut tool_call_found = false;
        let mut done_found = false;

        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            match event {
                StreamEvent::Delta(text) => {
                    assert_eq!(text, "Let me check the weather...");
                    delta_found = true;
                }
                StreamEvent::ToolCall(tc) => {
                    assert!(!tool_call_found, "only one ToolCall expected");
                    tool_call_found = true;
                    assert_eq!(tc.id, "toolu_abc");
                    assert_eq!(tc.name, "get_weather");
                    assert_eq!(tc.arguments, serde_json::json!({"location": "NYC"}));
                }
                StreamEvent::Done { finish, .. } => {
                    assert_eq!(finish, FinishReason::ToolUse);
                    done_found = true;
                }
            }
        }

        assert!(delta_found, "should have received a Delta event");
        assert!(tool_call_found, "should have received a ToolCall event");
        assert!(done_found, "should have received a Done event");
    }

    #[tokio::test]
    async fn test_empty_tools_omits_tools_key() {
        let mock = MockServer::start().await;
        let adapter = test_adapter(&mock);

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test-key".into());
        let req = ChatRequest {
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "Hi".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            model: "claude-sonnet-4-20250514".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![],
        };

        adapter.complete(&key, &req).await.unwrap();

        let requests = mock.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert!(
            body.get("tools").is_none(),
            "empty tools should not produce tools key"
        );
    }
}
