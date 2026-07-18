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
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "functionCall")]
    function_call: Option<GeminiFunctionCallReq>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "functionResponse")]
    function_response: Option<GeminiFunctionResponseReq>,
}

#[derive(Serialize)]
struct GeminiFunctionCallReq {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionResponseReq {
    name: String,
    response: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool>,
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
    #[serde(rename = "functionCall")]
    function_call: Option<GeminiResponseFunctionCall>,
}

#[derive(serde::Deserialize)]
struct GeminiResponseFunctionCall {
    name: String,
    args: serde_json::Value,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiBatchEmbedRequest {
    requests: Vec<GeminiEmbedRequestItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbedRequestItem {
    model: String,
    content: GeminiEmbedContent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbedContent {
    parts: Vec<GeminiPart>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiBatchEmbedResponse {
    embeddings: Vec<GeminiEmbeddingValue>,
}

#[derive(serde::Deserialize)]
struct GeminiEmbeddingValue {
    values: Vec<f32>,
}

fn role_to_gemini(role: &Role) -> &'static str {
    match role {
        Role::System => "user",
        Role::User => "user",
        Role::Assistant => "model",
        Role::Tool => "user",
    }
}

fn map_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "STOP" => FinishReason::Stop,
        "MAX_TOKENS" => FinishReason::Length,
        "FUNCTION_CALL" => FinishReason::ToolUse,
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
            .map(|m| {
                let role = role_to_gemini(&m.role).to_string();
                let parts = if m.role == Role::Assistant && !m.tool_calls.is_empty() {
                    let mut parts = Vec::new();
                    if !m.content.is_empty() {
                        parts.push(GeminiPart {
                            text: Some(m.content.clone()),
                            function_call: None,
                            function_response: None,
                        });
                    }
                    for tc in &m.tool_calls {
                        parts.push(GeminiPart {
                            text: None,
                            function_call: Some(GeminiFunctionCallReq {
                                name: tc.name.clone(),
                                args: tc.arguments.clone(),
                            }),
                            function_response: None,
                        });
                    }
                    parts
                } else if m.role == Role::Tool {
                    vec![GeminiPart {
                        text: None,
                        function_call: None,
                        function_response: Some(GeminiFunctionResponseReq {
                            name: m.tool_call_id.clone().unwrap_or_default(),
                            response: serde_json::json!({"content": m.content}),
                        }),
                    }]
                } else {
                    vec![GeminiPart {
                        text: Some(m.content.clone()),
                        function_call: None,
                        function_response: None,
                    }]
                };
                GeminiContent { role, parts }
            })
            .collect();

        let system_instruction = req.system.as_ref().map(|s| GeminiSystemInstruction {
            parts: vec![GeminiPart {
                text: Some(s.clone()),
                function_call: None,
                function_response: None,
            }],
        });

        let generation_config = if req.max_output_tokens.is_some() || req.temperature.is_some() {
            Some(GeminiGenerationConfig {
                max_output_tokens: req.max_output_tokens,
                temperature: req.temperature,
            })
        } else {
            None
        };

        let tools: Vec<GeminiTool> = if !req.tools.is_empty() {
            vec![GeminiTool {
                function_declarations: req
                    .tools
                    .iter()
                    .map(|t| GeminiFunctionDeclaration {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    })
                    .collect(),
            }]
        } else {
            vec![]
        };

        let body = GeminiRequest {
            system_instruction,
            contents,
            generation_config,
            tools,
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
        let (content, tool_calls) = candidate
            .as_ref()
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.as_ref())
            .map(|parts| {
                let mut text = String::new();
                let mut calls = Vec::new();
                for (idx, part) in parts.iter().enumerate() {
                    if let Some(t) = &part.text {
                        text.push_str(t);
                    }
                    if let Some(fc) = &part.function_call {
                        calls.push(ToolCall {
                            id: format!("{}#{}", fc.name, idx),
                            name: fc.name.clone(),
                            arguments: fc.args.clone(),
                        });
                    }
                }
                (text, calls)
            })
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
            tool_calls,
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
            .map(|m| {
                let role = role_to_gemini(&m.role).to_string();
                let parts = if m.role == Role::Assistant && !m.tool_calls.is_empty() {
                    let mut parts = Vec::new();
                    if !m.content.is_empty() {
                        parts.push(GeminiPart {
                            text: Some(m.content.clone()),
                            function_call: None,
                            function_response: None,
                        });
                    }
                    for tc in &m.tool_calls {
                        parts.push(GeminiPart {
                            text: None,
                            function_call: Some(GeminiFunctionCallReq {
                                name: tc.name.clone(),
                                args: tc.arguments.clone(),
                            }),
                            function_response: None,
                        });
                    }
                    parts
                } else if m.role == Role::Tool {
                    vec![GeminiPart {
                        text: None,
                        function_call: None,
                        function_response: Some(GeminiFunctionResponseReq {
                            name: m.tool_call_id.clone().unwrap_or_default(),
                            response: serde_json::json!({"content": m.content}),
                        }),
                    }]
                } else {
                    vec![GeminiPart {
                        text: Some(m.content.clone()),
                        function_call: None,
                        function_response: None,
                    }]
                };
                GeminiContent { role, parts }
            })
            .collect();

        let system_instruction = req.system.as_ref().map(|s| GeminiSystemInstruction {
            parts: vec![GeminiPart {
                text: Some(s.clone()),
                function_call: None,
                function_response: None,
            }],
        });

        let generation_config = if req.max_output_tokens.is_some() || req.temperature.is_some() {
            Some(GeminiGenerationConfig {
                max_output_tokens: req.max_output_tokens,
                temperature: req.temperature,
            })
        } else {
            None
        };

        let tools: Vec<GeminiTool> = if !req.tools.is_empty() {
            vec![GeminiTool {
                function_declarations: req
                    .tools
                    .iter()
                    .map(|t| GeminiFunctionDeclaration {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    })
                    .collect(),
            }]
        } else {
            vec![]
        };

        let body = GeminiRequest {
            system_instruction,
            contents,
            generation_config,
            tools,
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

#[async_trait::async_trait]
impl EmbeddingProvider for GeminiAdapter {
    async fn embed(
        &self,
        key: &SecretKey,
        req: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        let model_path = format!("models/{}", req.model);
        let url = format!(
            "{}/v1beta/models/{}:batchEmbedContents",
            self.base_url.trim_end_matches('/'),
            req.model
        );

        let requests: Vec<GeminiEmbedRequestItem> = req
            .inputs
            .iter()
            .map(|text| GeminiEmbedRequestItem {
                model: model_path.clone(),
                content: GeminiEmbedContent {
                    parts: vec![GeminiPart {
                        text: Some(text.clone()),
                        function_call: None,
                        function_response: None,
                    }],
                },
            })
            .collect();

        let body = GeminiBatchEmbedRequest { requests };

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
            "gemini embed response"
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

        let embed_resp: GeminiBatchEmbedResponse =
            serde_json::from_slice(&bytes).map_err(|e| ProviderError {
                category: ErrorCategory::InvalidRequest,
                retriable: false,
                detail: format!("failed to parse response: {}", e),
            })?;

        let embeddings: Vec<Vec<f32>> = embed_resp
            .embeddings
            .into_iter()
            .map(|e| e.values)
            .collect();

        tracing::info!(
            provider = "gemini",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            count = embeddings.len(),
            "gemini embed succeeded"
        );

        Ok(EmbeddingResponse {
            embeddings,
            model: req.model.clone(),
            usage: TokenUsage {
                input: None,
                output: None,
            },
        })
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
                                    for (idx, part) in parts.iter().enumerate() {
                                        if let Some(text) = part["text"].as_str() {
                                            if !text.is_empty() {
                                                result
                                                    .push(Ok(StreamEvent::Delta(text.to_string())));
                                            }
                                        }
                                        if let Some(fc) = part.get("functionCall") {
                                            if let (Some(name), Some(args)) = (
                                                fc.get("name").and_then(|n| n.as_str()),
                                                fc.get("args"),
                                            ) {
                                                result.push(Ok(StreamEvent::ToolCall(ToolCall {
                                                    id: format!("{}#{}", name, idx),
                                                    name: name.to_string(),
                                                    arguments: args.clone(),
                                                })));
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
                                            "FUNCTION_CALL" => FinishReason::ToolUse,
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
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                Message {
                    role: Role::Assistant,
                    content: "Hi there!".into(),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ],
            model: "gemini-2.0-flash".into(),
            max_output_tokens: Some(256),
            temperature: Some(0.7),
            request_id: None,
            tools: vec![],
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

    fn make_embed_request() -> EmbeddingRequest {
        EmbeddingRequest {
            model: "text-embedding-004".into(),
            inputs: vec!["Hello world".into()],
            request_id: None,
        }
    }

    fn make_embed_response_body() -> serde_json::Value {
        serde_json::json!({
            "embeddings": [{
                "values": [0.1, 0.2, 0.3]
            }]
        })
    }

    #[tokio::test]
    async fn embed_happy_path() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let response_body = make_embed_response_body();

        Mock::given(method("POST"))
            .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
            .and(header("x-goog-api-key", "test-gemini-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock)
            .await;

        let result = adapter.embed(&test_key(), &make_embed_request()).await;

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.embeddings.len(), 1);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(resp.model, "text-embedding-004");

        let requests = mock.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["requests"][0]["model"], "models/text-embedding-004");
        assert_eq!(
            body["requests"][0]["content"]["parts"][0]["text"],
            "Hello world"
        );
    }

    #[tokio::test]
    async fn embed_multiple_inputs() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let response_body = serde_json::json!({
            "embeddings": [
                {"values": [0.1, 0.2]},
                {"values": [0.3, 0.4]}
            ]
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock)
            .await;

        let req = EmbeddingRequest {
            model: "text-embedding-004".into(),
            inputs: vec!["first".into(), "second".into()],
            request_id: None,
        };

        let result = adapter.embed(&test_key(), &req).await;

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.embeddings.len(), 2);
        assert_eq!(resp.embeddings[0], vec![0.1, 0.2]);
        assert_eq!(resp.embeddings[1], vec![0.3, 0.4]);
    }

    #[tokio::test]
    async fn embed_unauthorized_returns_authentication() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let error_body = serde_json::json!({
            "error": {
                "message": "API key not valid"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/text-embedding-004:batchEmbedContents"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&error_body))
            .mount(&mock)
            .await;

        let result = adapter.embed(&test_key(), &make_embed_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Authentication);
        assert!(!err.retriable);
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

    #[tokio::test]
    async fn test_tool_call_complete() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .and(header("x-goog-api-key", "test-gemini-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [
                            {"text": "Let me check..."},
                            {"functionCall": {"name": "get_weather", "args": {"location": "NYC"}}}
                        ]
                    },
                    "finishReason": "FUNCTION_CALL"
                }],
                "usageMetadata": {
                    "promptTokenCount": 10,
                    "candidatesTokenCount": 5
                }
            })))
            .expect(1)
            .mount(&mock)
            .await;

        let key = test_key();
        let req = ChatRequest {
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "What's the weather in NYC?".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            model: "gemini-2.0-flash".into(),
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

        assert_eq!(result.content, "Let me check...");
        assert_eq!(result.finish, FinishReason::ToolUse);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].id, "get_weather#1");
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
        assert_eq!(
            body["tools"][0]["functionDeclarations"][0]["name"],
            "get_weather"
        );
    }

    #[tokio::test]
    async fn test_tool_call_stream() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let sse_body = "\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"location\":\"NYC\"}}}],\"role\":\"model\"},\"finishReason\":\"FUNCTION_CALL\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5}}

";

        Mock::given(method("POST"))
            .and(path(
                "/v1beta/models/gemini-2.0-flash:streamGenerateContent",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse_body))
            .mount(&mock)
            .await;

        use futures::StreamExt;
        let mut stream = adapter.stream(&test_key(), &make_request()).await.unwrap();

        let first = stream.next().await;
        match first {
            Some(Ok(StreamEvent::ToolCall(tc))) => {
                assert_eq!(tc.id, "get_weather#0");
                assert_eq!(tc.name, "get_weather");
                assert_eq!(tc.arguments, serde_json::json!({"location": "NYC"}));
            }
            other => panic!("expected ToolCall, got {:?}", other),
        }

        let second = stream.next().await;
        assert!(matches!(
            second,
            Some(Ok(StreamEvent::Done {
                finish: FinishReason::ToolUse,
                ..
            }))
        ));

        let third = stream.next().await;
        assert!(third.is_none());
    }

    #[tokio::test]
    async fn test_empty_tools_omits_tools_key() {
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        let response_body = serde_json::json!({
            "candidates": [{
                "content": {"parts": [{"text": "OK"}], "role": "model"},
                "finishReason": "STOP"
            }]
        });

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock)
            .await;

        let req = ChatRequest {
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "Hi".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            model: "gemini-2.0-flash".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![],
        };

        adapter.complete(&test_key(), &req).await.unwrap();

        let requests = mock.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert!(
            body.get("tools").is_none(),
            "empty tools should not produce tools key"
        );
    }

    #[tokio::test]
    async fn test_tool_call_unique_ids() {
        // Multiple functionCalls in one response; synthesized ids should be
        // stable and unique within one response.
        let mock = MockServer::start().await;
        let adapter = GeminiAdapter::new(Client::new(), mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [
                            {"functionCall": {"name": "get_weather", "args": {"city": "NYC"}}},
                            {"functionCall": {"name": "get_weather", "args": {"city": "LA"}}},
                            {"functionCall": {"name": "lookup_customer", "args": {"id": "123"}}}
                        ]
                    },
                    "finishReason": "FUNCTION_CALL"
                }],
                "usageMetadata": {
                    "promptTokenCount": 10,
                    "candidatesTokenCount": 5
                }
            })))
            .expect(1)
            .mount(&mock)
            .await;

        let key = test_key();
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gemini-2.0-flash".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![],
        };

        let result = adapter.complete(&key, &req).await.unwrap();
        assert_eq!(result.tool_calls.len(), 3);
        // Same name but different index → unique ids
        assert_eq!(result.tool_calls[0].id, "get_weather#0");
        assert_eq!(result.tool_calls[1].id, "get_weather#1");
        // Different name + index → different id
        assert_eq!(result.tool_calls[2].id, "lookup_customer#2");

        // Stable: calling again with same input produces same shape
        let mut ids: Vec<String> = result.tool_calls.iter().map(|tc| tc.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "all ids must be unique");
    }
}
