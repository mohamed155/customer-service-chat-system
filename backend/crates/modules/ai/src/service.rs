use crate::crypto;
use crate::model::{AiConfigRow, FallbackEntry};
use crate::resolution::{resolve_config, resolve_credential, Scope};
use crate::usage;
use ai_providers::{self, TokenUsage};
use futures::stream::BoxStream;
use rand::Rng;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

const RETRY_BASE_MS: &[u64] = &[200, 1000];

#[derive(Clone, Debug)]
pub struct AiCallContext {
    pub tenant_id: Uuid,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AiInput {
    pub system: Option<String>,
    pub messages: Vec<ai_providers::Message>,
}

#[derive(Clone, Debug)]
pub struct AiCallResult {
    pub content: String,
    pub provider: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish: ai_providers::FinishReason,
}

#[derive(Clone, Debug)]
pub enum AiCallError {
    NotConfigured,
    Provider {
        category: ai_providers::ErrorCategory,
        provider: String,
        model: String,
    },
    Internal(String),
}

pub enum AiStreamEvent {
    Delta(String),
    Done(AiCallResult),
    Error {
        category: ai_providers::ErrorCategory,
    },
}

pub type AiResultStream = BoxStream<'static, AiStreamEvent>;

#[derive(Clone)]
pub struct AiService(Arc<AiServiceInner>);

struct AiServiceInner {
    pool: PgPool,
    registry: ai_providers::Registry,
    master_key: Option<crypto::MasterKey>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct Attempt {
    pub provider: String,
    pub model: String,
    pub key: ai_providers::SecretKey,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[doc(hidden)]
pub async fn run_attempts(
    registry: &ai_providers::Registry,
    attempts: &[Attempt],
    system: Option<String>,
    messages: Vec<ai_providers::Message>,
) -> Result<AiCallResult, AiCallError> {
    run_attempts_traced(registry, attempts, None, system, messages).await
}

#[doc(hidden)]
pub async fn run_attempts_traced(
    registry: &ai_providers::Registry,
    attempts: &[Attempt],
    request_id: Option<&str>,
    system: Option<String>,
    messages: Vec<ai_providers::Message>,
) -> Result<AiCallResult, AiCallError> {
    let mut last_error: Option<AiCallError> = None;

    for attempt in attempts {
        let provider = match registry.resolve(&attempt.provider) {
            Some(p) => p,
            None => {
                tracing::warn!(provider = %attempt.provider, "unknown provider in run_attempts");
                continue;
            }
        };

        let req = ai_providers::ChatRequest {
            system: system.clone(),
            messages: messages.clone(),
            model: attempt.model.clone(),
            max_output_tokens: attempt.max_output_tokens,
            temperature: attempt.temperature,
            request_id: request_id.map(String::from),
        };

        for retry in 0..=2 {
            let attempt_start = Instant::now();
            match provider.complete(&attempt.key, &req).await {
                Ok(completion) => {
                    let latency = attempt_start.elapsed();
                    tracing::info!(
                        provider = %attempt.provider,
                        model = %attempt.model,
                        attempt = retry + 1,
                        outcome = "success",
                        latency_ms = latency.as_millis() as u64,
                        request_id = request_id.unwrap_or(""),
                        "AI provider call succeeded"
                    );
                    return Ok(AiCallResult {
                        content: completion.content,
                        provider: attempt.provider.clone(),
                        model: completion.model,
                        usage: completion.usage,
                        finish: completion.finish,
                    });
                }
                Err(err) if !err.retriable => {
                    let latency = attempt_start.elapsed();
                    tracing::info!(
                        provider = %attempt.provider,
                        model = %attempt.model,
                        attempt = retry + 1,
                        outcome = "non_retriable_error",
                        category = %err.category.as_str(),
                        latency_ms = latency.as_millis() as u64,
                        request_id = request_id.unwrap_or(""),
                        "AI provider call failed with non-retriable error"
                    );
                    return Err(AiCallError::Provider {
                        category: err.category,
                        provider: attempt.provider.clone(),
                        model: attempt.model.clone(),
                    });
                }
                Err(err) => {
                    let latency = attempt_start.elapsed();
                    tracing::info!(
                        provider = %attempt.provider,
                        model = %attempt.model,
                        attempt = retry + 1,
                        outcome = "retriable_error",
                        category = %err.category.as_str(),
                        latency_ms = latency.as_millis() as u64,
                        request_id = request_id.unwrap_or(""),
                        "AI provider call failed, will retry"
                    );
                    last_error = Some(AiCallError::Provider {
                        category: err.category,
                        provider: attempt.provider.clone(),
                        model: attempt.model.clone(),
                    });
                    if retry < 2 {
                        let delay = retry_delay(retry);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or(AiCallError::NotConfigured))
}

fn retry_delay(retry: u32) -> std::time::Duration {
    let base_ms = RETRY_BASE_MS[retry as usize];
    let jitter_factor: f64 = rand::thread_rng().gen_range(0.75..=1.25);
    std::time::Duration::from_millis((base_ms as f64 * jitter_factor) as u64)
}

impl AiService {
    pub fn from_config(pool: PgPool, config: &config::AppConfig) -> Result<Self, String> {
        let master_key = match &config.ai_key_encryption_key {
            Some(k) => Some(crypto::MasterKey::from_base64(k)?),
            None => None,
        };
        let registry_config = ai_providers::RegistryConfig {
            openai_base_url: config.ai_openai_base_url.clone(),
            anthropic_base_url: config.ai_anthropic_base_url.clone(),
            gemini_base_url: config.ai_gemini_base_url.clone(),
        };
        Ok(Self(Arc::new(AiServiceInner {
            pool,
            registry: ai_providers::Registry::new(registry_config),
            master_key,
        })))
    }

    pub async fn complete(
        &self,
        ctx: AiCallContext,
        input: AiInput,
    ) -> Result<AiCallResult, AiCallError> {
        let scope = Scope::Tenant(ctx.tenant_id);

        let resolved = resolve_config(&self.0.pool, scope)
            .await
            .map_err(|e| AiCallError::Internal(e.to_string()))?
            .ok_or(AiCallError::NotConfigured)?;

        let config: &AiConfigRow = &resolved.row;
        let master_key = self
            .0
            .master_key
            .as_ref()
            .ok_or(AiCallError::NotConfigured)?;

        let mut attempts: Vec<Attempt> = Vec::new();

        if let Some((key, _source_is_tenant)) =
            resolve_credential(&self.0.pool, master_key, scope, &config.provider)
                .await
                .map_err(AiCallError::Internal)?
        {
            attempts.push(Attempt {
                provider: config.provider.clone(),
                model: config.model.clone(),
                key,
                max_output_tokens: config.max_output_tokens.map(|v| v as u32),
                temperature: config.temperature,
            });
        }

        let fallbacks: Vec<FallbackEntry> =
            config
                .fallbacks
                .as_array()
                .map(|arr| {
                    arr.iter()
                    .filter_map(|v| match serde_json::from_value::<FallbackEntry>(v.clone()) {
                        Ok(entry) => Some(entry),
                        Err(e) => {
                            tracing::warn!(error = %e, "malformed fallback entry, skipping");
                            None
                        }
                    })
                    .collect()
                })
                .unwrap_or_default();

        for fb in &fallbacks {
            if let Some((key, _source_is_tenant)) =
                resolve_credential(&self.0.pool, master_key, scope, &fb.provider)
                    .await
                    .map_err(AiCallError::Internal)?
            {
                attempts.push(Attempt {
                    provider: fb.provider.clone(),
                    model: fb.model.clone(),
                    key,
                    max_output_tokens: config.max_output_tokens.map(|v| v as u32),
                    temperature: config.temperature,
                });
            }
        }

        if attempts.is_empty() {
            return Err(AiCallError::NotConfigured);
        }

        let started = Instant::now();
        let capture_content = resolved.capture_content;
        let request_content = capture_content.then(|| {
            serde_json::json!({
                "system": &input.system,
                "messages": &input.messages,
            })
        });

        let result = run_attempts_traced(
            &self.0.registry,
            &attempts,
            ctx.request_id.as_deref(),
            input.system,
            input.messages,
        )
        .await;

        let elapsed = started.elapsed();
        let elapsed_millis = elapsed.as_millis();

        match &result {
            Ok(ai_result) => {
                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: ai_result.provider.clone(),
                    model: ai_result.model.clone(),
                    input_tokens: ai_result.usage.input.map(|v| v as i32),
                    output_tokens: ai_result.usage.output.map(|v| v as i32),
                    status: "success",
                    error_category: None,
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content,
                    response_content: if capture_content {
                        Some(ai_result.content.clone())
                    } else {
                        None
                    },
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record successful AI usage");
                }
            }
            Err(AiCallError::Provider {
                category,
                provider,
                model,
            }) => {
                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: provider.clone(),
                    model: model.clone(),
                    input_tokens: None,
                    output_tokens: None,
                    status: "failure",
                    error_category: Some(category.as_str()),
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content: if capture_content {
                        request_content
                    } else {
                        None
                    },
                    response_content: None,
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record failed AI usage");
                }
            }
            Err(AiCallError::NotConfigured) | Err(AiCallError::Internal(_)) => {}
        }

        result
    }

    pub async fn complete_with_override(
        &self,
        ctx: AiCallContext,
        input: AiInput,
        provider: &str,
        model: &str,
    ) -> Result<AiCallResult, AiCallError> {
        let scope = Scope::Tenant(ctx.tenant_id);

        let resolved = resolve_config(&self.0.pool, scope)
            .await
            .map_err(|e| AiCallError::Internal(e.to_string()))?
            .ok_or(AiCallError::NotConfigured)?;

        let master_key = self
            .0
            .master_key
            .as_ref()
            .ok_or(AiCallError::NotConfigured)?;

        let key = resolve_credential(&self.0.pool, master_key, scope, provider)
            .await
            .map_err(AiCallError::Internal)?
            .ok_or(AiCallError::NotConfigured)?
            .0;

        let attempts = vec![Attempt {
            provider: provider.to_string(),
            model: model.to_string(),
            key,
            max_output_tokens: None,
            temperature: None,
        }];

        let started = Instant::now();
        let capture_content = resolved.capture_content;
        let request_content = capture_content.then(|| {
            serde_json::json!({
                "system": &input.system,
                "messages": &input.messages,
            })
        });

        let result = run_attempts_traced(
            &self.0.registry,
            &attempts,
            ctx.request_id.as_deref(),
            input.system,
            input.messages,
        )
        .await;

        let elapsed = started.elapsed();
        let elapsed_millis = elapsed.as_millis();

        match &result {
            Ok(ai_result) => {
                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: ai_result.provider.clone(),
                    model: ai_result.model.clone(),
                    input_tokens: ai_result.usage.input.map(|v| v as i32),
                    output_tokens: ai_result.usage.output.map(|v| v as i32),
                    status: "success",
                    error_category: None,
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content,
                    response_content: if capture_content {
                        Some(ai_result.content.clone())
                    } else {
                        None
                    },
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record successful AI usage");
                }
            }
            Err(AiCallError::Provider {
                category,
                provider,
                model,
            }) => {
                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: provider.clone(),
                    model: model.clone(),
                    input_tokens: None,
                    output_tokens: None,
                    status: "failure",
                    error_category: Some(category.as_str()),
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content: if capture_content {
                        request_content
                    } else {
                        None
                    },
                    response_content: None,
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record failed AI usage");
                }
            }
            Err(AiCallError::NotConfigured) | Err(AiCallError::Internal(_)) => {}
        }

        result
    }

    pub async fn embed_platform(
        &self,
        ctx: AiCallContext,
        inputs: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, AiCallError> {
        let scope = Scope::Platform;

        let resolved = resolve_config(&self.0.pool, scope)
            .await
            .map_err(|e| AiCallError::Internal(e.to_string()))?
            .ok_or(AiCallError::NotConfigured)?;

        let config = &resolved.row;
        let embedding_model = config
            .embedding_model
            .clone()
            .ok_or(AiCallError::NotConfigured)?;
        let provider_name = config.provider.clone();

        let master_key = self
            .0
            .master_key
            .as_ref()
            .ok_or(AiCallError::NotConfigured)?;

        let (key, _source_is_tenant) = resolve_credential(&self.0.pool, master_key, scope, &provider_name)
            .await
            .map_err(AiCallError::Internal)?
            .ok_or(AiCallError::NotConfigured)?;

        let embedding_provider = self.0.registry.embedding_provider(&provider_name).ok_or_else(|| {
            tracing::warn!(provider = %provider_name, "provider does not support embeddings");
            AiCallError::NotConfigured
        })?;

        let req = ai_providers::EmbeddingRequest {
            model: embedding_model,
            inputs,
            request_id: ctx.request_id.clone(),
        };

        let started = Instant::now();
        let result = embedding_provider.embed(&key, &req).await;
        let elapsed = started.elapsed();
        let elapsed_millis = elapsed.as_millis();

        match result {
            Ok(response) => {
                for (i, vec) in response.embeddings.iter().enumerate() {
                    if vec.len() != 1536 {
                        return Err(AiCallError::Internal(format!(
                            "embedding dimension mismatch at index {i}: expected 1536, got {}",
                            vec.len()
                        )));
                    }
                }

                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: provider_name,
                    model: response.model.clone(),
                    input_tokens: response.usage.input.map(|v| v as i32),
                    output_tokens: response.usage.output.map(|v| v as i32),
                    status: "success",
                    error_category: None,
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content: None,
                    response_content: None,
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record successful embedding usage");
                }

                Ok(response.embeddings)
            }
            Err(err) => {
                let w = usage::UsageWrite {
                    tenant_id: ctx.tenant_id,
                    provider: provider_name.clone(),
                    model: req.model.clone(),
                    input_tokens: None,
                    output_tokens: None,
                    status: "failure",
                    error_category: Some(err.category.as_str()),
                    streamed: false,
                    latency_ms: elapsed_millis as i32,
                    request_id: ctx.request_id,
                    request_content: None,
                    response_content: None,
                };
                if let Err(e) = usage::insert(&self.0.pool, w).await {
                    tracing::error!(%e, "failed to record failed embedding usage");
                }

                Err(AiCallError::Provider {
                    category: err.category,
                    provider: provider_name,
                    model: req.model,
                })
            }
        }
    }

    pub async fn stream(
        &self,
        ctx: AiCallContext,
        input: AiInput,
    ) -> Result<AiResultStream, AiCallError> {
        let scope = Scope::Tenant(ctx.tenant_id);

        let resolved = resolve_config(&self.0.pool, scope)
            .await
            .map_err(|e| AiCallError::Internal(e.to_string()))?
            .ok_or(AiCallError::NotConfigured)?;

        let config: &AiConfigRow = &resolved.row;
        let master_key = self
            .0
            .master_key
            .as_ref()
            .ok_or(AiCallError::NotConfigured)?;
        let pool = self.0.pool.clone();
        let _registry = self.0.registry.clone(); // Registry is not Clone — use ref
        let capture_content = resolved.capture_content;
        let request_id = ctx.request_id.clone();

        let mut attempts: Vec<Attempt> = Vec::new();

        if let Some((key, _source_is_tenant)) =
            resolve_credential(&self.0.pool, master_key, scope, &config.provider)
                .await
                .map_err(AiCallError::Internal)?
        {
            attempts.push(Attempt {
                provider: config.provider.clone(),
                model: config.model.clone(),
                key,
                max_output_tokens: config.max_output_tokens.map(|v| v as u32),
                temperature: config.temperature,
            });
        }

        let fallbacks: Vec<FallbackEntry> =
            config
                .fallbacks
                .as_array()
                .map(|arr| {
                    arr.iter()
                    .filter_map(|v| match serde_json::from_value::<FallbackEntry>(v.clone()) {
                        Ok(entry) => Some(entry),
                        Err(e) => {
                            tracing::warn!(error = %e, "malformed fallback entry, skipping");
                            None
                        }
                    })
                    .collect()
                })
                .unwrap_or_default();

        for fb in &fallbacks {
            if let Some((key, _source_is_tenant)) =
                resolve_credential(&self.0.pool, master_key, scope, &fb.provider)
                    .await
                    .map_err(AiCallError::Internal)?
            {
                attempts.push(Attempt {
                    provider: fb.provider.clone(),
                    model: fb.model.clone(),
                    key,
                    max_output_tokens: config.max_output_tokens.map(|v| v as u32),
                    temperature: config.temperature,
                });
            }
        }

        if attempts.is_empty() {
            return Err(AiCallError::NotConfigured);
        }

        let started = Instant::now();
        let provider_kind = ai_providers::ProviderKind::from_str(&config.provider)
            .unwrap_or(ai_providers::ProviderKind::OpenAi);

        // Check if streaming is supported
        if !provider_kind.supports_streaming() {
            let request_content = if capture_content {
                Some(serde_json::json!({"system": &input.system, "messages": &input.messages}))
            } else {
                None
            };

            let complete_result = run_attempts_traced(
                &self.0.registry,
                &attempts,
                ctx.request_id.as_deref(),
                input.system,
                input.messages,
            )
            .await?;

            let elapsed = started.elapsed();
            let content = complete_result.content.clone();

            // Record usage
            let w = usage::UsageWrite {
                tenant_id: ctx.tenant_id,
                provider: complete_result.provider.clone(),
                model: complete_result.model.clone(),
                input_tokens: complete_result.usage.input.map(|v| v as i32),
                output_tokens: complete_result.usage.output.map(|v| v as i32),
                status: "success",
                error_category: None,
                streamed: false,
                latency_ms: elapsed.as_millis() as i32,
                request_id,
                request_content,
                response_content: if capture_content {
                    Some(content.clone())
                } else {
                    None
                },
            };
            if let Err(e) = usage::insert(&pool, w).await {
                tracing::error!(%e, "failed to record non-streaming AI usage");
            }

            let events = vec![
                AiStreamEvent::Delta(content),
                AiStreamEvent::Done(complete_result),
            ];
            return Ok(Box::pin(futures::stream::iter(events)));
        }

        // Real streaming path via channel — with retry/failover before first delta
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let capture = capture_content;
        let pool = pool.clone();
        let registry = self.0.registry.clone();
        let ctx_tenant_id = ctx.tenant_id;
        let ctx_request_id = request_id;
        let input_system = input.system.clone();
        let input_messages = input.messages.clone();

        let ctx_request_id_clone = ctx_request_id.clone();

        tokio::spawn(async move {
            use futures::StreamExt;

            let mut content_accumulated = String::new();
            let started = Instant::now();
            let mut first_delta_sent = false;
            let mut last_error: Option<AiCallError> = None;

            for attempt in &attempts {
                if first_delta_sent {
                    break;
                }

                let provider = match registry.resolve(&attempt.provider) {
                    Some(p) => p,
                    None => {
                        tracing::warn!(provider = %attempt.provider, "stream: unknown provider");
                        continue;
                    }
                };

                let stream_req = ai_providers::ChatRequest {
                    system: input_system.clone(),
                    messages: input_messages.clone(),
                    model: attempt.model.clone(),
                    max_output_tokens: attempt.max_output_tokens,
                    temperature: attempt.temperature,
                    request_id: ctx_request_id_clone.clone(),
                };

                for retry in 0..=2 {
                    if first_delta_sent {
                        break;
                    }

                    let attempt_start = Instant::now();
                    match provider.stream(&attempt.key, &stream_req).await {
                        Ok(mut vendor_stream) => {
                            'stream_loop: while let Some(event) = vendor_stream.next().await {
                                match event {
                                    Ok(ai_providers::StreamEvent::Delta(text)) => {
                                        first_delta_sent = true;
                                        content_accumulated.push_str(&text);
                                        if tx.send(AiStreamEvent::Delta(text)).await.is_err() {
                                            let w = usage::UsageWrite {
                                                tenant_id: ctx_tenant_id,
                                                provider: attempt.provider.clone(),
                                                model: attempt.model.clone(),
                                                input_tokens: None,
                                                output_tokens: None,
                                                status: "failure",
                                                error_category: Some("unavailable"),
                                                streamed: true,
                                                latency_ms: started.elapsed().as_millis() as i32,
                                                request_id: ctx_request_id_clone.clone(),
                                                request_content: if capture {
                                                    Some(
                                                        serde_json::json!({"system": &input_system, "messages": &input_messages}),
                                                    )
                                                } else {
                                                    None
                                                },
                                                response_content: if capture {
                                                    Some(content_accumulated.clone())
                                                } else {
                                                    None
                                                },
                                            };
                                            let _ = usage::insert(&pool, w).await;
                                            return;
                                        }
                                    }
                                    Ok(ai_providers::StreamEvent::Done {
                                        usage,
                                        model,
                                        finish,
                                    }) => {
                                        let call_result = AiCallResult {
                                            content: content_accumulated.clone(),
                                            provider: attempt.provider.clone(),
                                            model: model.clone(),
                                            usage: usage.clone(),
                                            finish,
                                        };

                                        let w = usage::UsageWrite {
                                            tenant_id: ctx_tenant_id,
                                            provider: attempt.provider.clone(),
                                            model,
                                            input_tokens: usage.input.map(|v| v as i32),
                                            output_tokens: usage.output.map(|v| v as i32),
                                            status: "success",
                                            error_category: None,
                                            streamed: true,
                                            latency_ms: started.elapsed().as_millis() as i32,
                                            request_id: ctx_request_id_clone.clone(),
                                            request_content: if capture {
                                                Some(
                                                    serde_json::json!({"system": input_system, "messages": input_messages}),
                                                )
                                            } else {
                                                None
                                            },
                                            response_content: if capture {
                                                Some(content_accumulated.clone())
                                            } else {
                                                None
                                            },
                                        };
                                        if let Err(e) = usage::insert(&pool, w).await {
                                            tracing::error!(%e, "failed to record streaming AI usage");
                                        }

                                        let _ = tx.send(AiStreamEvent::Done(call_result)).await;
                                        return;
                                    }
                                    Err(err) => {
                                        if !first_delta_sent {
                                            let latency = attempt_start.elapsed();
                                            tracing::info!(
                                                provider = %attempt.provider,
                                                model = %attempt.model,
                                                attempt = retry + 1,
                                                outcome = if err.retriable { "stream_retriable_error" } else { "stream_non_retriable" },
                                                category = %err.category.as_str(),
                                                latency_ms = latency.as_millis() as u64,
                                                request_id = ctx_request_id_clone.as_deref().unwrap_or(""),
                                                "stream vendor error"
                                            );
                                            if err.retriable && retry < 2 {
                                                let delay = retry_delay(retry);
                                                tokio::time::sleep(delay).await;
                                                break 'stream_loop;
                                            }
                                        }
                                        let w = usage::UsageWrite {
                                            tenant_id: ctx_tenant_id,
                                            provider: attempt.provider.clone(),
                                            model: attempt.model.clone(),
                                            input_tokens: None,
                                            output_tokens: None,
                                            status: "failure",
                                            error_category: Some(err.category.as_str()),
                                            streamed: true,
                                            latency_ms: started.elapsed().as_millis() as i32,
                                            request_id: ctx_request_id_clone.clone(),
                                            request_content: if capture {
                                                Some(
                                                    serde_json::json!({"system": &input_system, "messages": &input_messages}),
                                                )
                                            } else {
                                                None
                                            },
                                            response_content: if capture {
                                                Some(content_accumulated.clone())
                                            } else {
                                                None
                                            },
                                        };
                                        if let Err(e) = usage::insert(&pool, w).await {
                                            tracing::error!(%e, "failed to record partial streaming AI usage");
                                        }
                                        let _ = tx
                                            .send(AiStreamEvent::Error {
                                                category: err.category,
                                            })
                                            .await;
                                        return;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            let latency = attempt_start.elapsed();
                            tracing::info!(
                                provider = %attempt.provider,
                                model = %attempt.model,
                                attempt = retry + 1,
                                outcome = "stream_start_error",
                                category = %err.category.as_str(),
                                latency_ms = latency.as_millis() as u64,
                                request_id = ctx_request_id_clone.as_deref().unwrap_or(""),
                                "stream start failed"
                            );
                            last_error = Some(AiCallError::Provider {
                                category: err.category,
                                provider: attempt.provider.clone(),
                                model: attempt.model.clone(),
                            });
                            if err.retriable && retry < 2 {
                                let delay = retry_delay(retry);
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                            break;
                        }
                    }
                }
            }

            if !first_delta_sent {
                let use_model = last_error
                    .as_ref()
                    .and_then(|e| {
                        if let AiCallError::Provider { model, .. } = e {
                            Some(model.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                let use_category = last_error.as_ref().and_then(|e| {
                    if let AiCallError::Provider { category, .. } = e {
                        Some(*category)
                    } else {
                        None
                    }
                });
                if let Some(cat) = use_category {
                    let w = usage::UsageWrite {
                        tenant_id: ctx_tenant_id,
                        provider: String::new(),
                        model: use_model,
                        input_tokens: None,
                        output_tokens: None,
                        status: "failure",
                        error_category: Some(cat.as_str()),
                        streamed: true,
                        latency_ms: started.elapsed().as_millis() as i32,
                        request_id: ctx_request_id_clone.clone(),
                        request_content: if capture {
                            Some(
                                serde_json::json!({"system": &input_system, "messages": &input_messages}),
                            )
                        } else {
                            None
                        },
                        response_content: None,
                    };
                    let _ = usage::insert(&pool, w).await;
                    let _ = tx.send(AiStreamEvent::Error { category: cat }).await;
                } else {
                    let _ = tx
                        .send(AiStreamEvent::Error {
                            category: ai_providers::ErrorCategory::Unavailable,
                        })
                        .await;
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)) as AiResultStream)
    }

    pub async fn stream_with_override(
        &self,
        ctx: AiCallContext,
        input: AiInput,
        provider: &str,
        model: &str,
    ) -> Result<AiResultStream, AiCallError> {
        let scope = Scope::Tenant(ctx.tenant_id);

        let resolved = resolve_config(&self.0.pool, scope)
            .await
            .map_err(|e| AiCallError::Internal(e.to_string()))?
            .ok_or(AiCallError::NotConfigured)?;

        let master_key = self
            .0
            .master_key
            .as_ref()
            .ok_or(AiCallError::NotConfigured)?;

        let key = resolve_credential(&self.0.pool, master_key, scope, provider)
            .await
            .map_err(AiCallError::Internal)?
            .ok_or(AiCallError::NotConfigured)?
            .0;

        let attempt = Attempt {
            provider: provider.to_string(),
            model: model.to_string(),
            key,
            max_output_tokens: None,
            temperature: None,
        };

        let pool = self.0.pool.clone();
        let capture_content = resolved.capture_content;
        let request_id = ctx.request_id.clone();
        let provider_kind = ai_providers::ProviderKind::from_str(provider)
            .unwrap_or(ai_providers::ProviderKind::OpenAi);

        if !provider_kind.supports_streaming() {
            let request_content = if capture_content {
                Some(serde_json::json!({"system": &input.system, "messages": &input.messages}))
            } else {
                None
            };

            let started = Instant::now();
            let complete_result = run_attempts_traced(
                &self.0.registry,
                &[attempt],
                ctx.request_id.as_deref(),
                input.system,
                input.messages,
            )
            .await?;
            let elapsed = started.elapsed();

            let w = usage::UsageWrite {
                tenant_id: ctx.tenant_id,
                provider: complete_result.provider.clone(),
                model: complete_result.model.clone(),
                input_tokens: complete_result.usage.input.map(|v| v as i32),
                output_tokens: complete_result.usage.output.map(|v| v as i32),
                status: "success",
                error_category: None,
                streamed: true,
                latency_ms: elapsed.as_millis() as i32,
                request_id,
                request_content,
                response_content: if capture_content {
                    Some(complete_result.content.clone())
                } else {
                    None
                },
            };
            if let Err(e) = usage::insert(&pool, w).await {
                tracing::error!(%e, "failed to record streaming AI usage (non-streaming fallback)");
            }

            let content = complete_result.content.clone();
            let events = vec![
                AiStreamEvent::Delta(content),
                AiStreamEvent::Done(complete_result),
            ];
            return Ok(Box::pin(futures::stream::iter(events)));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let capture = capture_content;
        let pool = pool.clone();
        let registry = self.0.registry.clone();
        let ctx_tenant_id = ctx.tenant_id;
        let ctx_request_id = request_id;
        let input_system = input.system.clone();
        let input_messages = input.messages.clone();

        tokio::spawn(async move {
            use futures::StreamExt;

            let mut content_accumulated = String::new();
            let started = Instant::now();

            let provider_obj = match registry.resolve(&attempt.provider) {
                Some(p) => p,
                None => {
                    tracing::warn!(provider = %attempt.provider, "stream_with_override: unknown provider");
                    let _ = tx
                        .send(AiStreamEvent::Error {
                            category: ai_providers::ErrorCategory::Unavailable,
                        })
                        .await;
                    return;
                }
            };

            let stream_req = ai_providers::ChatRequest {
                system: input_system.clone(),
                messages: input_messages.clone(),
                model: attempt.model.clone(),
                max_output_tokens: attempt.max_output_tokens,
                temperature: attempt.temperature,
                request_id: ctx_request_id.clone(),
            };

            let attempt_start = started;
            match provider_obj.stream(&attempt.key, &stream_req).await {
                Ok(mut vendor_stream) => {
                    while let Some(event) = vendor_stream.next().await {
                        match event {
                            Ok(ai_providers::StreamEvent::Delta(text)) => {
                                content_accumulated.push_str(&text);
                                if tx.send(AiStreamEvent::Delta(text)).await.is_err() {
                                    let w = usage::UsageWrite {
                                        tenant_id: ctx_tenant_id,
                                        provider: attempt.provider.clone(),
                                        model: attempt.model.clone(),
                                        input_tokens: None,
                                        output_tokens: None,
                                        status: "failure",
                                        error_category: Some("unavailable"),
                                        streamed: true,
                                        latency_ms: started.elapsed().as_millis() as i32,
                                        request_id: ctx_request_id.clone(),
                                        request_content: if capture {
                                            Some(serde_json::json!({"system": &input_system, "messages": &input_messages}))
                                        } else {
                                            None
                                        },
                                        response_content: if capture {
                                            Some(content_accumulated.clone())
                                        } else {
                                            None
                                        },
                                    };
                                    let _ = usage::insert(&pool, w).await;
                                    return;
                                }
                            }
                            Ok(ai_providers::StreamEvent::Done {
                                usage,
                                model,
                                finish,
                            }) => {
                                let call_result = AiCallResult {
                                    content: content_accumulated.clone(),
                                    provider: attempt.provider.clone(),
                                    model: model.clone(),
                                    usage: usage.clone(),
                                    finish,
                                };

                                let w = usage::UsageWrite {
                                    tenant_id: ctx_tenant_id,
                                    provider: attempt.provider.clone(),
                                    model,
                                    input_tokens: usage.input.map(|v| v as i32),
                                    output_tokens: usage.output.map(|v| v as i32),
                                    status: "success",
                                    error_category: None,
                                    streamed: true,
                                    latency_ms: started.elapsed().as_millis() as i32,
                                    request_id: ctx_request_id.clone(),
                                    request_content: if capture {
                                        Some(serde_json::json!({"system": input_system, "messages": input_messages}))
                                    } else {
                                        None
                                    },
                                    response_content: if capture {
                                        Some(content_accumulated.clone())
                                    } else {
                                        None
                                    },
                                };
                                if let Err(e) = usage::insert(&pool, w).await {
                                    tracing::error!(%e, "failed to record streaming AI usage");
                                }

                                let _ = tx.send(AiStreamEvent::Done(call_result)).await;
                                return;
                            }
                            Err(err) => {
                                let w = usage::UsageWrite {
                                    tenant_id: ctx_tenant_id,
                                    provider: attempt.provider.clone(),
                                    model: attempt.model.clone(),
                                    input_tokens: None,
                                    output_tokens: None,
                                    status: "failure",
                                    error_category: Some(err.category.as_str()),
                                    streamed: true,
                                    latency_ms: started.elapsed().as_millis() as i32,
                                    request_id: ctx_request_id.clone(),
                                    request_content: if capture {
                                        Some(serde_json::json!({"system": &input_system, "messages": &input_messages}))
                                    } else {
                                        None
                                    },
                                    response_content: if capture {
                                        Some(content_accumulated.clone())
                                    } else {
                                        None
                                    },
                                };
                                if let Err(e) = usage::insert(&pool, w).await {
                                    tracing::error!(%e, "failed to record streaming AI usage error");
                                }
                                let _ = tx
                                    .send(AiStreamEvent::Error {
                                        category: err.category,
                                    })
                                    .await;
                                return;
                            }
                        }
                    }

                    let w = usage::UsageWrite {
                        tenant_id: ctx_tenant_id,
                        provider: attempt.provider.clone(),
                        model: attempt.model.clone(),
                        input_tokens: None,
                        output_tokens: None,
                        status: "failure",
                        error_category: Some("unavailable"),
                        streamed: true,
                        latency_ms: started.elapsed().as_millis() as i32,
                        request_id: ctx_request_id.clone(),
                        request_content: if capture {
                            Some(serde_json::json!({"system": &input_system, "messages": &input_messages}))
                        } else {
                            None
                        },
                        response_content: if capture {
                            Some(content_accumulated.clone())
                        } else {
                            None
                        },
                    };
                    let _ = usage::insert(&pool, w).await;
                    let _ = tx
                        .send(AiStreamEvent::Error {
                            category: ai_providers::ErrorCategory::Unavailable,
                        })
                        .await;
                }
                Err(err) => {
                    let latency = attempt_start.elapsed();
                    tracing::info!(
                        provider = %attempt.provider,
                        model = %attempt.model,
                        outcome = "stream_start_error",
                        category = %err.category.as_str(),
                        latency_ms = latency.as_millis() as u64,
                        request_id = ctx_request_id.as_deref().unwrap_or(""),
                        "stream_with_override start failed"
                    );
                    let w = usage::UsageWrite {
                        tenant_id: ctx_tenant_id,
                        provider: attempt.provider.clone(),
                        model: attempt.model.clone(),
                        input_tokens: None,
                        output_tokens: None,
                        status: "failure",
                        error_category: Some(err.category.as_str()),
                        streamed: true,
                        latency_ms: started.elapsed().as_millis() as i32,
                        request_id: ctx_request_id.clone(),
                        request_content: if capture {
                            Some(serde_json::json!({"system": &input_system, "messages": &input_messages}))
                        } else {
                            None
                        },
                        response_content: None,
                    };
                    if let Err(e) = usage::insert(&pool, w).await {
                        tracing::error!(%e, "failed to record streaming AI usage error");
                    }
                    let _ = tx
                        .send(AiStreamEvent::Error {
                            category: err.category,
                        })
                        .await;
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)) as AiResultStream)
    }

    pub fn registry(&self) -> &ai_providers::Registry {
        &self.0.registry
    }

    pub fn master_key(&self) -> Option<&crypto::MasterKey> {
        self.0.master_key.as_ref()
    }
}

#[async_trait::async_trait]
impl knowledge::indexer::Embedder for AiService {
    async fn embed(
        &self,
        tenant_id: Uuid,
        texts: Vec<String>,
        request_id: String,
    ) -> Result<Vec<Vec<f32>>, knowledge::indexer::EmbedError> {
        let ctx = AiCallContext {
            tenant_id,
            request_id: Some(request_id),
        };
        match self.embed_platform(ctx, texts).await {
            Ok(embeddings) => Ok(embeddings),
            Err(AiCallError::Provider { category, .. }) if category.retriable() => {
                Err(knowledge::indexer::EmbedError::Retriable(
                    category.as_str().to_string(),
                ))
            }
            Err(AiCallError::Provider { category, .. }) => {
                Err(knowledge::indexer::EmbedError::Permanent(
                    category.as_str().to_string(),
                ))
            }
            Err(AiCallError::NotConfigured) => {
                Err(knowledge::indexer::EmbedError::NotConfigured)
            }
            Err(AiCallError::Internal(e)) => {
                Err(knowledge::indexer::EmbedError::Permanent(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_providers::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    struct TestProvider {
        results: Vec<Result<ChatCompletion, ProviderError>>,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait::async_trait]
    impl ChatProvider for TestProvider {
        async fn complete(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatCompletion, ProviderError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst) as usize;
            if idx < self.results.len() {
                self.results[idx].clone()
            } else {
                Err(ProviderError {
                    category: ErrorCategory::Unavailable,
                    retriable: true,
                    detail: "no more results".into(),
                })
            }
        }

        async fn stream(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(futures::stream::empty()))
        }
    }

    fn make_registry(openai: Arc<dyn ChatProvider>, anthropic: Arc<dyn ChatProvider>) -> Registry {
        Registry::new(RegistryConfig::new())
            .with_override("openai", openai)
            .with_override("anthropic", anthropic)
    }

    fn make_attempt(provider: &str, model: &str) -> Attempt {
        Attempt {
            provider: provider.to_string(),
            model: model.to_string(),
            key: SecretKey::new("sk-test".into()),
            max_output_tokens: None,
            temperature: None,
        }
    }

    fn ok_result(content: &str) -> ChatCompletion {
        ChatCompletion {
            content: content.to_string(),
            model: "test-model".into(),
            usage: TokenUsage {
                input: Some(10),
                output: Some(5),
            },
            finish: FinishReason::Stop,
        }
    }

    fn err_result(cat: ErrorCategory) -> ProviderError {
        ProviderError {
            category: cat,
            retriable: cat.retriable(),
            detail: format!("{:?}", cat),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_success_on_first_attempt() {
        let count = Arc::new(AtomicU32::new(0));
        let openai = Arc::new(TestProvider {
            results: vec![Ok(ok_result("hello"))],
            call_count: count.clone(),
        });
        let anthropic = Arc::new(TestProvider {
            results: vec![Ok(ok_result("fallback"))],
            call_count: Arc::new(AtomicU32::new(0)),
        });
        let registry = make_registry(openai, anthropic);

        let result = run_attempts(&registry, &[make_attempt("openai", "gpt-4")], None, vec![])
            .await
            .unwrap();
        assert_eq!(result.content, "hello");
        assert_eq!(result.provider, "openai");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_retries_then_failover() {
        let count_primary = Arc::new(AtomicU32::new(0));
        let primary = Arc::new(TestProvider {
            results: vec![
                Err(err_result(ErrorCategory::Unavailable)),
                Err(err_result(ErrorCategory::Unavailable)),
                Err(err_result(ErrorCategory::Unavailable)),
            ],
            call_count: count_primary.clone(),
        });
        let count_fallback = Arc::new(AtomicU32::new(0));
        let fallback = Arc::new(TestProvider {
            results: vec![Ok(ok_result("fallback-ok"))],
            call_count: count_fallback.clone(),
        });
        let registry = make_registry(primary, fallback);

        let handle = tokio::spawn(async move {
            run_attempts(
                &registry,
                &[
                    make_attempt("openai", "gpt-4"),
                    make_attempt("anthropic", "claude-3"),
                ],
                None,
                vec![],
            )
            .await
        });
        tokio::time::advance(Duration::from_secs(10)).await;
        let result = handle.await.unwrap().unwrap();

        assert_eq!(result.content, "fallback-ok");
        assert_eq!(result.provider, "anthropic");
        assert_eq!(count_primary.load(Ordering::SeqCst), 3);
        assert_eq!(count_fallback.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_non_retriable_aborts_immediately() {
        let count_primary = Arc::new(AtomicU32::new(0));
        let primary = Arc::new(TestProvider {
            results: vec![Err(err_result(ErrorCategory::Authentication))],
            call_count: count_primary.clone(),
        });
        let count_fallback = Arc::new(AtomicU32::new(0));
        let fallback = Arc::new(TestProvider {
            results: vec![Ok(ok_result("should-not-be-called"))],
            call_count: count_fallback.clone(),
        });
        let registry = make_registry(primary, fallback);

        let err = run_attempts(
            &registry,
            &[
                make_attempt("openai", "gpt-4"),
                make_attempt("anthropic", "claude-3"),
            ],
            None,
            vec![],
        )
        .await
        .unwrap_err();

        assert!(
            matches!(&err, AiCallError::Provider { category: ErrorCategory::Authentication, provider, .. } if provider == "openai")
        );
        assert_eq!(count_primary.load(Ordering::SeqCst), 1);
        assert_eq!(count_fallback.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn test_all_exhausted_returns_last_error() {
        let count = Arc::new(AtomicU32::new(0));
        let primary = Arc::new(TestProvider {
            results: vec![
                Err(err_result(ErrorCategory::Unavailable)),
                Err(err_result(ErrorCategory::Unavailable)),
                Err(err_result(ErrorCategory::Unavailable)),
            ],
            call_count: count.clone(),
        });
        let registry = make_registry(
            primary,
            Arc::new(TestProvider {
                results: vec![
                    Err(err_result(ErrorCategory::RateLimited)),
                    Err(err_result(ErrorCategory::RateLimited)),
                    Err(err_result(ErrorCategory::RateLimited)),
                ],
                call_count: Arc::new(AtomicU32::new(0)),
            }),
        );

        let handle = tokio::spawn(async move {
            run_attempts(
                &registry,
                &[
                    make_attempt("openai", "gpt-4"),
                    make_attempt("anthropic", "claude-3"),
                ],
                None,
                vec![],
            )
            .await
        });
        tokio::time::advance(Duration::from_secs(10)).await;
        let err = handle.await.unwrap().unwrap_err();

        match err {
            AiCallError::Provider {
                category,
                provider,
                model,
            } => {
                assert_eq!(provider, "anthropic");
                assert_eq!(model, "claude-3");
                assert!(matches!(category, ErrorCategory::RateLimited));
            }
            _ => panic!("expected Provider error"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_single_attempt_success() {
        let count = Arc::new(AtomicU32::new(0));
        let provider = Arc::new(TestProvider {
            results: vec![Ok(ok_result("direct"))],
            call_count: count.clone(),
        });
        let registry = make_registry(
            provider,
            Arc::new(TestProvider {
                results: vec![],
                call_count: Arc::new(AtomicU32::new(0)),
            }),
        );

        let result = run_attempts(&registry, &[make_attempt("openai", "gpt-4")], None, vec![])
            .await
            .unwrap();
        assert_eq!(result.content, "direct");
        assert_eq!(result.model, "test-model");
    }

    // -----------------------------------------------------------------------
    // Stream mock tests
    // -----------------------------------------------------------------------

    struct StreamingTestProvider {
        stream_events: Vec<Result<StreamEvent, ProviderError>>,
        complete_result: Result<ChatCompletion, ProviderError>,
    }

    #[async_trait::async_trait]
    impl ChatProvider for StreamingTestProvider {
        async fn complete(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatCompletion, ProviderError> {
            self.complete_result.clone()
        }

        async fn stream(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(futures::stream::iter(self.stream_events.clone())))
        }
    }

    #[tokio::test]
    async fn test_stream_deltas_accumulate() {
        use futures::StreamExt;

        let provider = Arc::new(StreamingTestProvider {
            stream_events: vec![
                Ok(StreamEvent::Delta("Hello ".into())),
                Ok(StreamEvent::Delta("World".into())),
                Ok(StreamEvent::Done {
                    usage: TokenUsage {
                        input: Some(5),
                        output: Some(2),
                    },
                    model: "gpt-4".into(),
                    finish: FinishReason::Stop,
                }),
            ],
            complete_result: Ok(ok_result("fallback")),
        });

        let registry = Registry::new(RegistryConfig::new()).with_override("openai", provider);
        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };

        let resolved = registry.resolve("openai").unwrap();
        let mut stream = resolved.stream(&key, &req).await.unwrap();

        let mut deltas = Vec::new();
        let mut usage = None;
        while let Some(event) = stream.next().await {
            match event.unwrap() {
                StreamEvent::Delta(text) => deltas.push(text),
                StreamEvent::Done { usage: u, .. } => usage = Some(u),
            }
        }

        assert_eq!(deltas, vec!["Hello ", "World"]);
        assert_eq!(usage.unwrap().input, Some(5));
    }

    #[tokio::test]
    async fn test_stream_error() {
        use futures::StreamExt;

        let provider = Arc::new(StreamingTestProvider {
            stream_events: vec![Err(ProviderError {
                category: ErrorCategory::Authentication,
                retriable: false,
                detail: "invalid key".into(),
            })],
            complete_result: Ok(ok_result("fallback")),
        });

        let registry = Registry::new(RegistryConfig::new()).with_override("openai", provider);
        let key = SecretKey::new("sk-test".into());
        let req = ChatRequest {
            system: None,
            messages: vec![],
            model: "gpt-4".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
        };

        let resolved = registry.resolve("openai").unwrap();
        let mut stream = resolved.stream(&key, &req).await.unwrap();
        let event = stream.next().await.unwrap();

        assert!(matches!(
            event,
            Err(ProviderError {
                category: ErrorCategory::Authentication,
                ..
            })
        ));
    }
}
