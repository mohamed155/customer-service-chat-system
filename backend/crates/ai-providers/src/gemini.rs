use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::Serialize;

use crate::contract::*;

#[derive(Debug)]
pub struct GeminiAdapter {
    client: Client,
    base_url: String,
}

impl GeminiAdapter {
    pub fn new(client: Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiResponseContent>,
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct GeminiResponseContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(serde::Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

#[derive(serde::Deserialize)]
struct GeminiErrorBody {
    error: Option<GeminiErrorDetail>,
}

#[derive(serde::Deserialize)]
struct GeminiErrorDetail {
    message: Option<String>,
}

fn role_to_gemini(role: &Role) -> &'static str {
    match role {
        Role::System => "user",
        Role::User => "user",
        Role::Assistant => "model",
    }
}

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "STOP" => FinishReason::Stop,
        "MAX_TOKENS" => FinishReason::Length,
        _ => FinishReason::Other,
    }
}

fn normalize_error(status: reqwest::StatusCode, detail: String) -> ProviderError {
    let category = match status.as_u16() {
        401 | 403 => ErrorCategory::Authentication,
        429 => ErrorCategory::RateLimited,
        500..=599 => ErrorCategory::Unavailable,
        _ => ErrorCategory::InvalidRequest,
    };
    ProviderError {
        category,
        retriable: category.retriable(),
        detail,
    }
}

fn extract_error_detail(body: &[u8]) -> String {
    serde_json::from_slice::<GeminiErrorBody>(body)
        .ok()
        .and_then(|e| e.error)
        .and_then(|e| e.message)
        .unwrap_or_else(|| "unknown error".into())
}

#[async_trait::async_trait]
impl ChatProvider for GeminiAdapter {
    async fn complete(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url.trim_end_matches('/'),
            req.model
        );

        let contents: Vec<GeminiContent> = req
            .messages
            .iter()
            .map(|m| GeminiContent {
                role: role_to_gemini(&m.role).to_string(),
                parts: vec![GeminiPart {
                    text: m.content.clone(),
                }],
            })
            .collect();

        let system_instruction = req.system.as_ref().map(|s| GeminiSystemInstruction {
            parts: vec![GeminiPart { text: s.clone() }],
        });

        let generation_config = if req.max_output_tokens.is_some() || req.temperature.is_some() {
            Some(GeminiGenerationConfig {
                max_output_tokens: req.max_output_tokens,
                temperature: req.temperature,
            })
        } else {
            None
        };

        let body = GeminiRequest {
            system_instruction,
            contents,
            generation_config,
        };

        let elapsed = std::time::Instant::now();

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-goog-api-key", key.expose())
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let response = req_builder.send().await.map_err(|e| {
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
        })?;

        let status = response.status();
        tracing::info!(
            provider = "gemini",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "gemini complete response"
        );

        let bytes = response.bytes().await.map_err(|_| ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: "failed to read response body".into(),
        })?;

        if !status.is_success() {
            let detail = extract_error_detail(&bytes);
            return Err(normalize_error(status, detail));
        }

        let gemini_resp: GeminiResponse =
            serde_json::from_slice(&bytes).map_err(|e| ProviderError {
                category: ErrorCategory::InvalidRequest,
                retriable: false,
                detail: format!("failed to parse response: {}", e),
            })?;

        let candidate = gemini_resp.candidates.and_then(|c| c.into_iter().next());
        let content = candidate
            .as_ref()
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.as_ref())
            .and_then(|p| p.first())
            .and_then(|p| p.text.as_ref())
            .cloned()
            .unwrap_or_default();
        let finish = candidate
            .as_ref()
            .and_then(|c| c.finish_reason.as_deref())
            .map(map_finish_reason)
            .unwrap_or(FinishReason::Other);

        let usage = gemini_resp
            .usage_metadata
            .as_ref()
            .map(|u| TokenUsage {
                input: u.prompt_token_count,
                output: u.candidates_token_count,
            })
            .unwrap_or_else(|| TokenUsage {
                input: None,
                output: None,
            });

        tracing::info!(
            provider = "gemini",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            "gemini complete succeeded"
        );

        Ok(ChatCompletion {
            content,
            model: req.model.clone(),
            usage,
            finish,
        })
    }

    async fn stream(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ProviderError>>, ProviderError> {
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
            self.base_url.trim_end_matches('/'),
            req.model
        );

        let contents: Vec<GeminiContent> = req
            .messages
            .iter()
            .map(|m| GeminiContent {
                role: role_to_gemini(&m.role).to_string(),
                parts: vec![GeminiPart {
                    text: m.content.clone(),
                }],
            })
            .collect();

        let system_instruction = req.system.as_ref().map(|s| GeminiSystemInstruction {
            parts: vec![GeminiPart { text: s.clone() }],
        });

        let generation_config = if req.max_output_tokens.is_some() || req.temperature.is_some() {
            Some(GeminiGenerationConfig {
                max_output_tokens: req.max_output_tokens,
                temperature: req.temperature,
            })
        } else {
            None
        };

        let body = GeminiRequest {
            system_instruction,
            contents,
            generation_config,
        };

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-goog-api-key", key.expose())
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let response = req_builder.send().await.map_err(|e| {
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
        })?;

        let status = response.status();
        tracing::info!(
            provider = "gemini",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "gemini stream response"
        );

        if !status.is_success() {
            let bytes = response.bytes().await.map_err(|_| ProviderError {
                category: ErrorCategory::Unavailable,
                retriable: true,
                detail: "failed to read response body".into(),
            })?;
            let detail = extract_error_detail(&bytes);
            return Err(normalize_error(status, detail));
        }

        let stream = response.bytes_stream().boxed();
        let frame_stream = crate::sse::sse_frames(stream);

        Ok(
            gemini_sse_to_events(frame_stream, req.model.clone()).boxed()
                as BoxStream<'static, Result<StreamEvent, ProviderError>>,
        )
    }
}

fn gemini_sse_to_events(
    frame_stream: impl futures::Stream<Item = Result<crate::sse::SseFrame, ProviderError>>
        + Send
        + 'static,
    request_model: String,
) -> impl futures::Stream<Item = Result<StreamEvent, ProviderError>> + Send + 'static {
    let total_usage = std::sync::Arc::new(std::sync::Mutex::new(None::<TokenUsage>));

    frame_stream.flat_map(move |frame_result| {
        let model = request_model.clone();
        let total_usage = std::sync::Arc::clone(&total_usage);

        let events: Vec<Result<StreamEvent, ProviderError>> = match frame_result {
            Ok(frame) => match serde_json::from_str::<serde_json::Value>(&frame.data) {
                Ok(json) => {
                    if let Some(usage) = json.get("usageMetadata") {
                        let input = usage["promptTokenCount"].as_u64().map(|v| v as u32);
                        let output = usage["candidatesTokenCount"].as_u64().map(|v| v as u32);
                        *total_usage.lock().unwrap() = Some(TokenUsage { input, output });
                    }

                    let mut result = Vec::new();

                    if let Some(candidates) = json["candidates"].as_array() {
                        if let Some(candidate) = candidates.first() {
                            if let Some(content) = candidate["content"].as_object() {
                                if let Some(parts) = content.get("parts").and_then(|p| p.as_array())
                                {
                                    if let Some(part) = parts.first() {
                                        if let Some(text) = part["text"].as_str() {
                                            if !text.is_empty() {
                                                result
                                                    .push(Ok(StreamEvent::Delta(text.to_string())));
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(fr) = candidate["finishReason"].as_str() {
                                if !fr.is_empty() {
                                    let usage =
                                        total_usage.lock().unwrap().take().unwrap_or_default();
                                    result.push(Ok(StreamEvent::Done {
                                        usage,
                                        model,
                                        finish: match fr {
                                            "STOP" => FinishReason::Stop,
                                            "MAX_TOKENS" => FinishReason::Length,
                                            _ => FinishReason::Other,
                                        },
                                    }));
                                }
                            }
                        }
                    } else {
                        let usage = total_usage.lock().unwrap().take().unwrap_or_default();
                        result.push(Ok(StreamEvent::Done {
                            usage,
                            model,
                            finish: FinishReason::Stop,
                        }));
                    }

                    result
                }
                Err(e) => {
                    vec![Err(ProviderError {
                        category: ErrorCategory::InvalidRequest,
                        retriable: false,
                        detail: format!("malformed gemini stream frame: {e}"),
                    })]
                }
            },
            Err(e) => vec![Err(e)],
        };

        futures::stream::iter(events)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ChatRequest, Message, Role, SecretKey};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_key() -> SecretKey {
        SecretKey::new("test-gemini-key-123".into())
    }

    fn make_request() -> ChatRequest {
        ChatRequest {
            system: Some("You are a helpful assistant.".into()),
            messages: vec![
                Message {
                    role: Role::User,
                    content: "Hello!".into(),
                },
                Message {
                    role: Role::Assistant,
                    content: "Hi there!".into(),
                },
            ],
            model: "gemini-2.0-flash".into(),
            max_output_tokens: Some(256),
            temperature: Some(0.7),
            request_id: None,
        }
    }

    fn make_response_body() -> serde_json::Value {
        serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello! How can I help you today?"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 8
            }
        })
    }

    #[tokio::test]
    async fn happy_path() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let response_body = make_response_body();

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .and(header("x-goog-api-key", "test-gemini-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_ok());
        let completion = result.unwrap();
        assert_eq!(completion.content, "Hello! How can I help you today?");
        assert_eq!(completion.model, "gemini-2.0-flash");
        assert_eq!(completion.usage.input, Some(10));
        assert_eq!(completion.usage.output, Some(8));
        assert_eq!(completion.finish, FinishReason::Stop);

        // Verify the request body was correct by inspecting wiremock's recorded requests
        let requests = mock.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are a helpful assistant."
        );
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "Hello!");
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["contents"][1]["parts"][0]["text"], "Hi there!");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(body["generationConfig"]["temperature"], 0.7);
    }

    #[tokio::test]
    async fn missing_usage_returns_none() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let response_body = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "OK"}]
                },
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_ok());
        let completion = result.unwrap();
        assert_eq!(completion.usage.input, None);
        assert_eq!(completion.usage.output, None);
    }

    #[tokio::test]
    async fn unauthorized_returns_authentication() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let error_body = serde_json::json!({
            "error": {
                "message": "API key not valid"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&error_body))
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Authentication);
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn rate_limited_returns_rate_limited() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let error_body = serde_json::json!({
            "error": {
                "message": "Rate limit exceeded"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(429).set_body_json(&error_body))
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category, ErrorCategory::RateLimited);
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn server_error_returns_unavailable() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let error_body = serde_json::json!({
            "error": {
                "message": "Internal server error"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&error_body))
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Unavailable);
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn bad_request_returns_invalid_request() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let error_body = serde_json::json!({
            "error": {
                "message": "Model not found"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .mount(&mock)
            .await;

        let result = adapter.complete(&test_key(), &make_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category, ErrorCategory::InvalidRequest);
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn test_stream_sse() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let sse_body = "\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello!\"}],\"role\":\"model\"}}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5}}

data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5}}

";

        Mock::given(method("POST"))
            .and(path(
                "/v1beta/models/gemini-2.0-flash:streamGenerateContent",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_body))
            .mount(&mock)
            .await;

        let mut stream = adapter.stream(&test_key(), &make_request()).await.unwrap();

        use futures::StreamExt;
        let first = stream.next().await;
        assert!(matches!(first, Some(Ok(StreamEvent::Delta(ref c))) if c == "Hello!"));

        let second = stream.next().await;
        assert!(matches!(second, Some(Ok(StreamEvent::Done { .. }))));

        let third = stream.next().await;
        assert!(third.is_none());
    }

    #[tokio::test]
    async fn test_stream_single_frame_with_text_and_finish() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let sse_body = "\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello!\"}],\"role\":\"model\"},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5}}

";

        Mock::given(method("POST"))
            .and(path(
                "/v1beta/models/gemini-2.0-flash:streamGenerateContent",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_body))
            .mount(&mock)
            .await;

        let mut stream = adapter.stream(&test_key(), &make_request()).await.unwrap();

        use futures::StreamExt;
        let first = stream.next().await;
        assert!(matches!(first, Some(Ok(StreamEvent::Delta(ref c))) if c == "Hello!"));

        let second = stream.next().await;
        assert!(matches!(second, Some(Ok(StreamEvent::Done { .. }))));

        let third = stream.next().await;
        assert!(third.is_none());
    }
}
