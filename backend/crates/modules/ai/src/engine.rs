use rand::Rng;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::agent_config::{self, AgentConfigurationRow};
use crate::agent_prompt;
use crate::generation_record::{self, GenerationOutcome, GenerationRecord};
use crate::prompt_store;
use crate::prompt_validate;
use crate::{AiCallContext, AiCallError, AiInput, AiService};

/// Output of [`assemble_context`]: the prepared `AiInput` plus metadata about
/// the retrieval step that the caller can use for confidence derivation and
/// citation persistence.
pub struct AssembledContext {
    pub input: AiInput,
    pub retrieved_chunks: Vec<knowledge::retrieval::RetrievedChunk>,
    pub retrieval_degraded: bool,
}

/// Build the full `AiInput` (system message, history, knowledge block) for a
/// generation run, exactly reproducing the prompt composition logic that was
/// previously inline in `agent_responder.rs` (Phase B).
///
/// This is deterministic per Principle IV: identical `row`, `history`,
/// `channel`, and retrieved chunks produce byte-for-byte equal prompt text.
pub async fn assemble_context(
    pool: &PgPool,
    ai: &AiService,
    tenant_id: Uuid,
    conversation_id: Uuid,
    row: &AgentConfigurationRow,
    is_platform_persona: bool,
    channel: &str,
) -> sqlx::Result<AssembledContext> {
    let prompt_bootstrap = prompt_store::load_bootstrap(pool, tenant_id).await?;
    let (prompt_content, _prompt_version) = match &prompt_bootstrap {
        Some((p, v)) => (v.content.clone(), p.active_version),
        None => (String::new(), 0_i32),
    };

    let business_rules: Vec<String> =
        serde_json::from_value(row.business_rules.clone()).unwrap_or_default();

    let system_content = if is_platform_persona || prompt_content.is_empty() {
        prompt_content
    } else {
        let tenant_name = tenancy::authorize::fetch_tenant(pool, tenant_id)
            .await
            .map(|t| t.name)
            .unwrap_or_default();
        let customer_name =
            conversations::queries::customer_display_name(pool, tenant_id, conversation_id)
                .await?
                .unwrap_or_else(|| "the customer".to_string());

        let mut vars = HashMap::new();
        vars.insert("agent_name", row.name.clone());
        vars.insert("tenant_name", tenant_name);
        vars.insert("customer_name", customer_name);
        vars.insert("channel", channel.to_string());

        prompt_validate::render_prompt(&prompt_content, &vars)
    };

    let system_message = agent_prompt::compose_system_message(
        &row.name,
        &system_content,
        &row.tone,
        &business_rules,
    );

    let history =
        conversations::queries::recent_history(pool, tenant_id, conversation_id, 20).await?;

    let query_string: String = history
        .iter()
        .rev()
        .filter(|(kind, _)| kind == "customer")
        .take(4)
        .map(|(_, body)| body.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    let messages: Vec<ai_providers::Message> = history
        .into_iter()
        .map(|(kind, body)| {
            let role = match kind.as_str() {
                "customer" => ai_providers::Role::User,
                _ => ai_providers::Role::Assistant,
            };
            ai_providers::Message { role, content: body }
        })
        .collect();

    let mut input = AiInput {
        system: Some(system_message),
        messages,
    };

    let mut degraded = false;
    let mut retrieved_chunks: Vec<knowledge::retrieval::RetrievedChunk> = Vec::new();

    if !query_string.is_empty() {
        let retrieval_start = Instant::now();
        let embed_ctx = AiCallContext {
            tenant_id,
            request_id: None,
        };
        let query_len = query_string.len();

        let retrieval_result = tokio::time::timeout(
            std::time::Duration::from_millis(800),
            async move {
                let embeddings = ai.embed_platform(embed_ctx, vec![query_string]).await?;
                let embedding = embeddings.into_iter().next().ok_or_else(|| {
                    AiCallError::Internal("empty embedding result".into())
                })?;
                knowledge::retrieval::search(pool, tenant_id, &embedding, 5, 0.70)
                    .await
                    .map_err(|e| AiCallError::Internal(e.to_string()))
            },
        )
        .await;

        let elapsed_ms = retrieval_start.elapsed().as_millis() as u64;

        match retrieval_result {
            Ok(Ok(chunks)) => {
                retrieved_chunks = chunks;
            }
            _ => {
                degraded = true;
            }
        }

        if !retrieved_chunks.is_empty() {
            let mut knowledge_block =
                String::from("\n\n=== Knowledge Context ===\n");
            for chunk in &retrieved_chunks {
                knowledge_block.push_str(&format!(
                    "Source: \"{}\" (relevance: {:.2})\n{}\n\n",
                    chunk.item_title, chunk.similarity, chunk.content
                ));
            }
            knowledge_block.push_str("=== End Knowledge Context ===");

            if let Some(ref mut system) = input.system {
                system.push_str(&knowledge_block);
            }
        }

        let candidates = retrieved_chunks.len();
        let top_score = retrieved_chunks
            .first()
            .map(|c| c.similarity)
            .unwrap_or(0.0);
        tracing::info!(
            target: "rag",
            tenant_id = %tenant_id,
            conversation_id = %conversation_id,
            query_len = query_len,
            candidates = candidates,
            returned = candidates,
            top_score = top_score,
            elapsed_ms = elapsed_ms,
            degraded = degraded,
            "rag.retrieve"
        );
    }

    Ok(AssembledContext {
        input,
        retrieved_chunks,
        retrieval_degraded: degraded,
    })
}

const RETRY_BASE_MS: &[u64] = &[200, 1000];

fn retry_delay(retry: u32) -> Duration {
    let base_ms = RETRY_BASE_MS[retry as usize];
    let jitter_factor: f64 = rand::thread_rng().gen_range(0.75..=1.25);
    Duration::from_millis((base_ms as f64 * jitter_factor) as u64)
}

/// Output of [`generate`] — the full provider response content plus metadata.
pub struct GenerationOutput {
    pub content: String,
    pub provider: String,
    pub model: String,
    pub usage: ai_providers::TokenUsage,
    pub finish_length: bool,
    pub continuation_used: bool,
    pub usage_record_id: Option<Uuid>,
}

/// Call the streaming provider, collect the full response, and return the
/// assembled output. Uses `stream_with_override` when `provider_override` is
/// `Some`, otherwise uses the platform-resolved `stream`.
///
/// Retry/fallback behaviour (US2):
/// - Up to 3 provider attempts total, only on retriable errors
/// - Exponential backoff with jitter between retries
/// - 45-second outer deadline (returns `AiCallError::Provider` with `Timeout`)
/// - On mid-stream retriable failure after partial content: continuation request
/// - Empty/whitespace-only content is treated as non-retriable failure
pub async fn generate(
    ai: &AiService,
    ctx: AiCallContext,
    input: AiInput,
    provider_override: Option<(&str, &str)>,
) -> Result<GenerationOutput, AiCallError> {
    use futures::StreamExt;
    let system = input.system.clone();
    let base_messages = input.messages;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);

    let mut content = String::new();
    let mut final_provider = String::new();
    let mut final_model = String::new();
    let mut final_usage = ai_providers::TokenUsage::default();
    let mut finish_length = false;
    let mut continuation_used = false;
    let mut last_error_category: Option<ai_providers::ErrorCategory> = None;

    for attempt in 0..3 {
        if tokio::time::Instant::now() >= deadline {
            return Err(AiCallError::Provider {
                category: ai_providers::ErrorCategory::Timeout,
                provider: final_provider.clone(),
                model: final_model.clone(),
            });
        }

        // Build messages — add continuation context if we have partial content
        let attempt_messages = if continuation_used && !content.trim().is_empty() {
            let mut msgs = base_messages.clone();
            msgs.push(ai_providers::Message {
                role: ai_providers::Role::Assistant,
                content: content.clone(),
            });
            msgs.push(ai_providers::Message {
                role: ai_providers::Role::User,
                content: "Continue the previous assistant message exactly where it stopped. Do not repeat any text already written. Do not add any preamble.".into(),
            });
            msgs
        } else {
            base_messages.clone()
        };

        // Reset per-attempt accumulation
        let mut chunk = String::new();

        let attempt_input = AiInput {
            system: system.clone(),
            messages: attempt_messages,
        };

        let mut stream = if let Some((provider, model)) = provider_override {
            ai.stream_with_override(ctx.clone(), attempt_input, provider, model)
                .await?
        } else {
            ai.stream(ctx.clone(), attempt_input).await?
        };

        let mut stream_failed = false;
        let mut had_partial = false;

        while let Some(event) = stream.next().await {
            match event {
                crate::AiStreamEvent::Delta(text) => {
                    chunk.push_str(&text);
                    had_partial = true;
                }
                crate::AiStreamEvent::Done(result) => {
                    final_provider = result.provider;
                    final_model = result.model;
                    final_usage = result.usage;
                    finish_length =
                        matches!(result.finish, ai_providers::FinishReason::Length);
                }
                crate::AiStreamEvent::Error { category } => {
                    stream_failed = true;
                    last_error_category = Some(category);
                    break;
                }
            }
        }

        if !stream_failed {
            // Success — stitch partial content if continuation was used
            if continuation_used && had_partial {
                content.push_str(&chunk);
            } else if !continuation_used {
                content = chunk;
            }

            // Empty/whitespace-only is non-retriable failure
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return Err(AiCallError::Provider {
                    category: ai_providers::ErrorCategory::InvalidRequest,
                    provider: final_provider,
                    model: final_model,
                });
            }

            return Ok(GenerationOutput {
                content,
                provider: final_provider,
                model: final_model,
                usage: final_usage,
                finish_length,
                continuation_used,
                usage_record_id: None,
            });
        }

        // Stream failed — decide next step
        let category = last_error_category.unwrap_or(ai_providers::ErrorCategory::Unavailable);

        if !category.retriable() {
            return Err(AiCallError::Provider {
                category,
                provider: final_provider,
                model: final_model,
            });
        }

        // Retriable — save partial content for continuation
        if continuation_used {
            // Continuation also failed — discard and fall through to exhaustion
            content.push_str(&chunk);
        } else if had_partial {
            content = chunk;
            continuation_used = true;
        }

        // Apply backoff before next attempt (except last)
        if attempt < 2 {
            tokio::time::sleep(retry_delay(attempt)).await;
        }
    }

    Err(AiCallError::Provider {
        category: last_error_category.unwrap_or(ai_providers::ErrorCategory::Unavailable),
        provider: final_provider,
        model: final_model,
    })
}

/// Run a full generation cycle: assemble context, call the provider, store
/// the reply with citations, and write a generation record.
///
/// Returns `Ok(())` even when the generation fails (fallback paths are handled
/// internally). Errors that prevent deletion of the outbox event are
/// propagated.
pub async fn run_generation(
    pool: &PgPool,
    ai: &AiService,
    presence: &Arc<escalations::presence::Runtime>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    trigger_message_id: Uuid,
    event_id: i64,
    row: &AgentConfigurationRow,
    is_platform_persona: bool,
    channel: &str,
) -> sqlx::Result<()> {
    let generation_id = Uuid::new_v4();
    let _span = tracing::info_span!(
        "engine.generate",
        tenant_id = %tenant_id,
        conversation_id = %conversation_id,
        trigger_message_id = %trigger_message_id,
        generation_id = %generation_id,
    );

    let start = Instant::now();

    // 1. Assemble context (prompt, history, retrieval)
    let assembled = assemble_context(
        pool,
        ai,
        tenant_id,
        conversation_id,
        row,
        is_platform_persona,
        channel,
    )
    .await?;

    // 2. Determine provider/model override
    let provider_override = if let (Some(provider), Some(model)) = (&row.provider, &row.model) {
        if agent_config::credential_resolves(pool, tenant_id, provider).await {
            Some((provider.as_str(), model.as_str()))
        } else {
            None
        }
    } else {
        None
    };

    let ctx = AiCallContext {
        tenant_id,
        request_id: None,
    };

    // 3. Call the provider
    let gen_result = generate(ai, ctx, assembled.input, provider_override).await;

    let latency_ms = start.elapsed().as_millis() as i32;

    match gen_result {
        Ok(output) => {
            // 4. Idempotency check
            let already_replied = conversations::queries::has_ai_reply_since(
                pool,
                tenant_id,
                conversation_id,
                trigger_message_id,
            )
            .await?;

            if !already_replied {
                let mut tx = pool.begin().await?;

                let ai_message_id = conversations::queries::insert_ai_reply_in_tx(
                    &mut tx,
                    tenant_id,
                    conversation_id,
                    &output.content,
                )
                .await?;

                if !assembled.retrieved_chunks.is_empty() {
                    let citations: Vec<conversations::model::CitationToInsert> = assembled
                        .retrieved_chunks
                        .iter()
                        .enumerate()
                        .map(|(i, chunk)| conversations::model::CitationToInsert {
                            knowledge_item_id: chunk.item_id,
                            item_title: chunk.item_title.clone(),
                            passage_text: chunk.content.clone(),
                            relevance_score: chunk.similarity as f32,
                            ordinal: i as i32,
                        })
                        .collect();
                    conversations::queries::insert_citations_in_tx(
                        &mut tx,
                        tenant_id,
                        ai_message_id,
                        &citations,
                    )
                    .await?;
                }

                tx.commit().await?;

                // 5. Write generation record
                let retrieval_top = assembled
                    .retrieved_chunks
                    .first()
                    .map(|c| c.similarity as f32);
                let rec = GenerationRecord {
                    id: generation_id,
                    tenant_id,
                    conversation_id,
                    trigger_message_id,
                    response_message_id: Some(ai_message_id),
                    usage_record_id: output.usage_record_id,
                    provider: Some(output.provider),
                    model: Some(output.model),
                    outcome: GenerationOutcome::Success,
                    error_category: None,
                    attempts: 1,
                    continuation_used: output.continuation_used,
                    retrieval_chunk_count: assembled.retrieved_chunks.len() as i16,
                    retrieval_top_similarity: retrieval_top,
                    retrieval_degraded: assembled.retrieval_degraded,
                    confidence_score: None,
                    latency_ms,
                    request_id: None,
                    created_at: Some(chrono::Utc::now()),
                };
                let _ = generation_record::insert(pool, &rec).await;
            }

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;

            tracing::info!("engine: reply sent");
        }
        Err(AiCallError::NotConfigured) => {
            if is_platform_persona {
                let has_ack =
                    conversations::queries::has_system_message(pool, tenant_id, conversation_id)
                        .await?;
                if !has_ack {
                    let mut tx = pool.begin().await?;
                    conversations::queries::insert_auto_ack_in_tx(
                        &mut tx,
                        tenant_id,
                        conversation_id,
                        "Thank you for your message. A team member will be with you shortly.",
                    )
                    .await?;
                    tx.commit().await?;
                }
            }
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
        }
        Err(AiCallError::Provider {
            category,
            provider: _provider_name,
            model: _model_name,
        }) => {
            // Provider retries exhausted — fallback + escalation
            let fallback_body = "I'm sorry — I'm having trouble responding right now. A team member will follow up shortly.";
            let last_category = Some(category.as_str().to_string());

            let fallback_result: Result<(), sqlx::Error> = async {
                let mut tx = pool.begin().await?;
                conversations::queries::insert_fallback_in_tx(
                    &mut tx,
                    tenant_id,
                    conversation_id,
                    fallback_body,
                )
                .await?;

                let present_ids = presence.present_membership_ids_async(tenant_id).await;
                let _ = escalations::routing::route_new_escalation_in_tx(
                    &mut tx,
                    pool,
                    tenant_id,
                    conversation_id,
                    "AI assistant unavailable",
                    &[],
                    &[],
                    &present_ids,
                    Uuid::nil(),
                )
                .await;

                tx.commit().await.map_err(|e| {
                    tracing::error!(%e, "engine: fallback tx commit failed");
                    e
                })
            }
            .await;

            let (outcome, error_category) = match fallback_result {
                Ok(()) => (GenerationOutcome::Fallback, last_category),
                Err(e) => {
                    tracing::error!(%e, "engine: fallback insert itself failed");
                    (GenerationOutcome::Failed, Some(format!("fallback_error: {e}")))
                }
            };

            let rec = GenerationRecord {
                id: generation_id,
                tenant_id,
                conversation_id,
                trigger_message_id,
                response_message_id: None,
                usage_record_id: None,
                provider: None,
                model: None,
                outcome,
                error_category,
                attempts: 1,
                continuation_used: false,
                retrieval_chunk_count: assembled.retrieved_chunks.len() as i16,
                retrieval_top_similarity: assembled.retrieved_chunks.first().map(|c| c.similarity as f32),
                retrieval_degraded: assembled.retrieval_degraded,
                confidence_score: None,
                latency_ms,
                request_id: None,
                created_at: Some(chrono::Utc::now()),
            };
            let _ = generation_record::insert(pool, &rec).await;

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
        }
        Err(e) => {
            tracing::warn!(?e, "engine: unexpected generation error");
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_context_determinism_placeholder() {
        // T007a: prompt-determinism test will be added here.
        // Verifies that `assemble_context` with identical inputs produces
        // byte-for-byte equal AiInput values.
    }
}
