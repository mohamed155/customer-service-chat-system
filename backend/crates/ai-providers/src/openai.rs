use crate::contract::*;
use futures::StreamExt;

pub struct OpenAiAdapter {
    client: reqwest::Client,
    base_url: String,
}

impl OpenAiAdapter {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }
}

#[async_trait::async_trait]
impl ChatProvider for OpenAiAdapter {
    async fn complete(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut messages = Vec::new();
        if let Some(ref system) = req.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system
            }));
        }
        for msg in &req.messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": msg.content
            }));
        }

        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
        });
        if let Some(ref max_tokens) = req.max_output_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(ref temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let elapsed = std::time::Instant::now();

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key.expose()))
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| normalize_error(e, "request failed"))?;

        let status = response.status();
        tracing::info!(
            provider = "openai",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "openai complete response"
        );

        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(normalize_http_error(status, &detail));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: format!("response parse: {e}"),
        })?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let model = json["model"].as_str().unwrap_or(&req.model).to_string();
        let input = json["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
        let output = json["usage"]["completion_tokens"]
            .as_u64()
            .map(|v| v as u32);
        let finish = match json["choices"][0]["finish_reason"].as_str() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            _ => FinishReason::Other,
        };

        tracing::info!(
            provider = "openai",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            "openai complete succeeded"
        );

        Ok(ChatCompletion {
            content,
            model,
            usage: TokenUsage { input, output },
            finish,
        })
    }

    async fn stream(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut messages = Vec::new();
        if let Some(ref system) = req.system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system
            }));
        }
        for msg in &req.messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": msg.content
            }));
        }

        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(ref max_tokens) = req.max_output_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(ref temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key.expose()))
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| normalize_error(e, "request failed"))?;

        let status = response.status();
        tracing::info!(
            provider = "openai",
            model = %req.model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "openai stream response"
        );

        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(normalize_http_error(status, &detail));
        }

        let stream = response.bytes_stream().boxed();
        let frame_stream = crate::sse::sse_frames(stream);

        Ok(Box::pin(openai_sse_to_events(frame_stream, req.model.clone())) as ChatStream)
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OpenAiAdapter {
    async fn embed(
        &self,
        key: &SecretKey,
        req: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        let url = format!("{}/v1/embeddings", self.base_url);

        let model = if req.model.is_empty() {
            "text-embedding-3-small"
        } else {
            &req.model
        };

        let body = serde_json::json!({
            "model": model,
            "input": req.inputs,
        });

        let elapsed = std::time::Instant::now();

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key.expose()))
            .json(&body);
        if let Some(ref rid) = req.request_id {
            req_builder = req_builder.header("X-Request-ID", rid);
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| normalize_error(e, "request failed"))?;

        let status = response.status();
        tracing::info!(
            provider = "openai",
            model = %model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            status = %status.as_u16(),
            "openai embed response"
        );

        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(normalize_http_error(status, &detail));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: format!("response parse: {e}"),
        })?;

        let embeddings: Vec<Vec<f32>> = json["data"]
            .as_array()
            .map(|data| {
                data.iter()
                    .map(|entry| {
                        entry["embedding"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let response_model = json["model"].as_str().unwrap_or(model).to_string();
        let input = json["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
        let output = json["usage"]["total_tokens"].as_u64().map(|v| v as u32);

        tracing::info!(
            provider = "openai",
            model = %response_model,
            request_id = req.request_id.as_deref().unwrap_or(""),
            latency_ms = elapsed.elapsed().as_millis() as u64,
            num_embeddings = embeddings.len(),
            "openai embed succeeded"
        );

        Ok(EmbeddingResponse {
            embeddings,
            model: response_model,
            usage: TokenUsage { input, output },
        })
    }
}

fn normalize_error(e: reqwest::Error, context: &str) -> ProviderError {
    if e.is_timeout() {
        ProviderError {
            category: ErrorCategory::Timeout,
            retriable: true,
            detail: format!("{context}: timeout"),
        }
    } else if e.is_connect() {
        ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: format!("{context}: connection error"),
        }
    } else {
        ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: format!("{context}: request failed"),
        }
    }
}

fn openai_sse_to_events(
    frame_stream: impl futures::Stream<Item = Result<crate::sse::SseFrame, ProviderError>>
        + Send
        + 'static,
    request_model: String,
) -> impl futures::Stream<Item = Result<StreamEvent, ProviderError>> + Send + 'static {
    let recorded_usage = std::sync::Arc::new(std::sync::Mutex::new(None::<TokenUsage>));
    let response_model = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let finish = std::sync::Arc::new(std::sync::Mutex::new(FinishReason::Other));

    frame_stream.filter_map(move |frame_result| {
        let req_model = request_model.clone();
        let recorded_usage = std::sync::Arc::clone(&recorded_usage);
        let response_model = std::sync::Arc::clone(&response_model);
        let finish = std::sync::Arc::clone(&finish);
        async move {
            match frame_result {
                Ok(frame) => {
                    if frame.data.trim() == "[DONE]" {
                        let usage = recorded_usage.lock().unwrap().take().unwrap_or_default();
                        let model = response_model.lock().unwrap().take().unwrap_or(req_model);
                        let fin = finish.lock().unwrap().clone();
                        return Some(Ok(StreamEvent::Done {
                            usage,
                            model,
                            finish: fin,
                        }));
                    }

                    match serde_json::from_str::<serde_json::Value>(&frame.data) {
                        Ok(json) => {
                            if let Some(choices) = json["choices"].as_array() {
                                if let Some(choice) = choices.first() {
                                    if let Some(delta) = choice["delta"].as_object() {
                                        if let Some(content) =
                                            delta.get("content").and_then(|c| c.as_str())
                                        {
                                            if !content.is_empty() {
                                                return Some(Ok(StreamEvent::Delta(
                                                    content.to_string(),
                                                )));
                                            }
                                        }
                                    }
                                    if let Some(fr) = choice["finish_reason"].as_str() {
                                        *finish.lock().unwrap() = match fr {
                                            "stop" => FinishReason::Stop,
                                            "length" => FinishReason::Length,
                                            _ => FinishReason::Other,
                                        };
                                    }
                                }
                            }
                            if json.get("usage").is_some() {
                                let input =
                                    json["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
                                let output = json["usage"]["completion_tokens"]
                                    .as_u64()
                                    .map(|v| v as u32);
                                *recorded_usage.lock().unwrap() =
                                    Some(TokenUsage { input, output });
                            }
                            if let Some(model) = json["model"].as_str() {
                                *response_model.lock().unwrap() = Some(model.to_string());
                            }
                            None
                        }
                        Err(e) => Some(Err(ProviderError {
                            category: ErrorCategory::InvalidRequest,
                            retriable: false,
                            detail: format!("malformed stream frame: {e}"),
                        })),
                    }
                }
                Err(e) => Some(Err(e)),
            }
        }
    })
}

fn normalize_http_error(status: reqwest::StatusCode, detail: &str) -> ProviderError {
    let message = extract_error_message(detail);
    match status.as_u16() {
        401 | 403 => ProviderError {
            category: ErrorCategory::Authentication,
            retriable: false,
            detail: message,
        },
        429 => ProviderError {
            category: ErrorCategory::RateLimited,
            retriable: true,
            detail: message,
        },
        s if s >= 500 => ProviderError {
            category: ErrorCategory::Unavailable,
            retriable: true,
            detail: message,
        },
        _ => ProviderError {
            category: ErrorCategory::InvalidRequest,
            retriable: false,
            detail: message,
        },
    }
}

fn extract_error_message(body: &str) -> String {
    let detail = if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            msg.to_string()
        } else {
            body.chars()
                .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                .collect::<String>()
        }
    } else {
        body.chars()
            .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
            .collect::<String>()
    };
    sanitize_error_detail(&detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_client() -> (reqwest::Client, MockServer) {
        let mock_server = MockServer::start().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        (client, mock_server)
    }

    #[tokio::test]
    async fn happy_path_request_body_and_response() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "Hi there!"}, "finish_reason": "stop"}],
                "model": "gpt-4",
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            })))
            .mount(&mock)
            .await;

        let req = ChatRequest {
            system: Some("You are helpful".into()),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".into(),
            }],
            model: "gpt-4".into(),
            max_output_tokens: Some(100),
            temperature: Some(0.7),
            request_id: None,
        };

        let key = SecretKey::new("sk-test-key".into());
        let result = adapter.complete(&key, &req).await.unwrap();
        assert_eq!(result.content, "Hi there!");
        assert_eq!(result.model, "gpt-4");
        assert_eq!(result.usage.input, Some(10));
        assert_eq!(result.usage.output, Some(5));
        assert_eq!(result.finish, FinishReason::Stop);
    }

    #[tokio::test]
    async fn missing_usage_returns_none() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": "OK"}, "finish_reason": "stop"}],
                "model": "gpt-4"
            })))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "Hi".into(),
            }],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let result = adapter.complete(&key, &req).await.unwrap();
        assert_eq!(result.usage.input, None);
        assert_eq!(result.usage.output, None);
    }

    #[tokio::test]
    async fn error_401_authentication() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-bad".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::Authentication));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn error_429_rate_limited() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::RateLimited));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn error_500_unavailable() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::Unavailable));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn error_400_invalid_request() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("unknown model"))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::InvalidRequest));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn error_detail_extracts_message_from_json_body() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(
                serde_json::json!({"error": {"message": "Incorrect API key provided"}}),
            ))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-bad".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::InvalidRequest));
        assert_eq!(err.detail, "Incorrect API key provided");
    }

    #[tokio::test]
    async fn error_403_maps_to_authentication() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-bad".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };
        let err = adapter.complete(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::Authentication));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn embed_happy_path() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .and(header("Authorization", "Bearer sk-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": "list",
                "data": [
                    {"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]},
                    {"object": "embedding", "index": 1, "embedding": [0.4, 0.5, 0.6]}
                ],
                "model": "text-embedding-3-small",
                "usage": {"prompt_tokens": 5, "total_tokens": 5}
            })))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test-key".into());
        let req = EmbeddingRequest {
            model: "text-embedding-3-small".into(),
            inputs: vec!["hello".into(), "world".into()],
            request_id: None,
        };

        let result = adapter.embed(&key, &req).await.unwrap();
        assert_eq!(result.embeddings.len(), 2);
        assert_eq!(result.embeddings[0], vec![0.1f32, 0.2, 0.3]);
        assert_eq!(result.embeddings[1], vec![0.4f32, 0.5, 0.6]);
        assert_eq!(result.model, "text-embedding-3-small");
        assert_eq!(result.usage.input, Some(5));
        assert_eq!(result.usage.output, Some(5));
    }

    #[tokio::test]
    async fn embed_default_model() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": "list",
                "data": [
                    {"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]}
                ],
                "model": "text-embedding-3-small",
                "usage": {"prompt_tokens": 3, "total_tokens": 3}
            })))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test".into());
        let req = EmbeddingRequest {
            model: "".into(),
            inputs: vec!["test".into()],
            request_id: None,
        };

        let result = adapter.embed(&key, &req).await.unwrap();
        assert_eq!(result.model, "text-embedding-3-small");
    }

    #[tokio::test]
    async fn embed_error_401_authentication() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-bad".into());
        let req = EmbeddingRequest {
            model: "text-embedding-3-small".into(),
            inputs: vec!["hello".into()],
            request_id: None,
        };

        let err = adapter.embed(&key, &req).await.unwrap_err();
        assert!(matches!(err.category, ErrorCategory::Authentication));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn embed_request_id_header() {
        let (client, mock) = test_client().await;
        let adapter = OpenAiAdapter::new(client, mock.uri());

        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .and(header("Authorization", "Bearer sk-test-key"))
            .and(header("X-Request-ID", "req-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": "list",
                "data": [
                    {"object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3]}
                ],
                "model": "text-embedding-3-small",
                "usage": {"prompt_tokens": 3, "total_tokens": 3}
            })))
            .mount(&mock)
            .await;

        let key = SecretKey::new("sk-test-key".into());
        let req = EmbeddingRequest {
            model: "text-embedding-3-small".into(),
            inputs: vec!["hello".into()],
            request_id: Some("req-123".into()),
        };

        let result = adapter.embed(&key, &req).await.unwrap();
        assert_eq!(result.embeddings.len(), 1);
    }
}
