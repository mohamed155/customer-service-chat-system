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

/// Context for broadcasting AI engine events to the tenant SSE stream and
/// performing mid-stream supersede checks.
pub struct BroadcastCtx {
    pub presence: Arc<escalations::presence::Runtime>,
    pub pool: PgPool,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub trigger_message_id: Uuid,
}

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

/// Error from [`generate`] — either a provider error or a supersede/cancel signal.
#[derive(Debug)]
pub enum GenerateError {
    Provider(AiCallError),
    Superseded { reason: String },
}

impl From<AiCallError> for GenerateError {
    fn from(e: AiCallError) -> Self {
        GenerateError::Provider(e)
    }
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
///
/// When `broadcast_ctx` is provided, deltas are broadcast to the tenant SSE
/// stream (throttled to ~4/s) for real-time streaming UI.
pub async fn generate(
    ai: &AiService,
    ctx: AiCallContext,
    input: AiInput,
    provider_override: Option<(&str, &str)>,
    broadcast_ctx: Option<&BroadcastCtx>,
) -> Result<GenerationOutput, GenerateError> {
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

    // Broadcast started before the first provider attempt
    if let Some(bc) = broadcast_ctx {
        let started_ev = escalations::presence::Event::ConversationAi(
            escalations::model::ConversationAiEvent::Started(
                escalations::model::ConversationAiStarted {
                    conversation_id: bc.conversation_id,
                    generation_id: bc.generation_id,
                    trigger_message_id: bc.trigger_message_id,
                    started_at: chrono::Utc::now(),
                },
            ),
        );
        bc.presence
            .broadcast(bc.tenant_id, started_ev);
    }

    for attempt in 0..3 {
        if tokio::time::Instant::now() >= deadline {
            return Err(GenerateError::Provider(AiCallError::Provider {
                category: ai_providers::ErrorCategory::Timeout,
                provider: final_provider.clone(),
                model: final_model.clone(),
            }));
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
                .await
                .map_err(GenerateError::Provider)?
        } else {
            ai.stream(ctx.clone(), attempt_input)
                .await
                .map_err(GenerateError::Provider)?
        };

        let mut stream_failed = false;
        let mut had_partial = false;

        let mut last_broadcast = Instant::now();
        let mut last_supersede_check = Instant::now();

        while let Some(event) = stream.next().await {
            match event {
                crate::AiStreamEvent::Delta(text) => {
                    chunk.push_str(&text);
                    had_partial = true;

                    // Broadcast delta (throttled ~4/s) when context is provided
                    if let Some(bc) = broadcast_ctx {
                        let now = Instant::now();
                        if now.duration_since(last_broadcast) >= Duration::from_millis(250) {
                            last_broadcast = now;
                            let delta_ev = escalations::presence::Event::ConversationAi(
                                escalations::model::ConversationAiEvent::Delta(
                                    escalations::model::ConversationAiDelta {
                                        conversation_id: bc.conversation_id,
                                        generation_id: bc.generation_id,
                                        text: text.clone(),
                                    },
                                ),
                            );
                            bc.presence
                                .broadcast(bc.tenant_id, delta_ev);
                        }

                        // Mid-stream supersede checks (~1/s)
                        if now.duration_since(last_supersede_check) >= Duration::from_secs(1) {
                            last_supersede_check = now;
                            let has_newer = conversations::queries::has_customer_message_after(
                                &bc.pool,
                                bc.tenant_id,
                                bc.conversation_id,
                                bc.trigger_message_id,
                            )
                            .await
                            .unwrap_or(false);
                            if has_newer {
                                return Err(GenerateError::Superseded {
                                    reason: "newer_message".into(),
                                });
                            }
                            let has_esc = escalations::routing::has_open_escalation(
                                &bc.pool,
                                bc.tenant_id,
                                bc.conversation_id,
                            )
                            .await
                            .unwrap_or(false);
                            if has_esc {
                                return Err(GenerateError::Superseded {
                                    reason: "escalated".into(),
                                });
                            }
                        }
                    }
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
                return Err(GenerateError::Provider(AiCallError::Provider {
                    category: ai_providers::ErrorCategory::InvalidRequest,
                    provider: final_provider,
                    model: final_model,
                }));
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
            return Err(GenerateError::Provider(AiCallError::Provider {
                category,
                provider: final_provider,
                model: final_model,
            }));
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

    Err(GenerateError::Provider(AiCallError::Provider {
        category: last_error_category.unwrap_or(ai_providers::ErrorCategory::Unavailable),
        provider: final_provider,
        model: final_model,
    }))
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

    // 3. Create broadcast context for SSE events
    let broadcast_ctx = BroadcastCtx {
        presence: presence.clone(),
        pool: pool.clone(),
        tenant_id,
        conversation_id,
        generation_id,
        trigger_message_id,
    };

    // 4. Call the provider
    let gen_result = generate(ai, ctx, assembled.input, provider_override, Some(&broadcast_ctx)).await;

    let latency_ms = start.elapsed().as_millis() as i32;

    // Pre-commit re-check: if a newer message arrived or escalation opened
    // during generation, discard the result
    let has_newer = conversations::queries::has_customer_message_after(
        pool, tenant_id, conversation_id, trigger_message_id,
    )
    .await
    .unwrap_or(false);
    let has_esc = escalations::routing::has_open_escalation(pool, tenant_id, conversation_id)
        .await
        .unwrap_or(false);
    let already_replied =
        conversations::queries::has_ai_reply_since(pool, tenant_id, conversation_id, trigger_message_id)
            .await
            .unwrap_or(false);

    let superseded = has_newer || has_esc || already_replied;

    match gen_result {
        Ok(output) if !superseded => {
            let mid = {
                let mut tx = pool.begin().await?;
                let mid = conversations::queries::insert_ai_reply_in_tx(
                    &mut tx, tenant_id, conversation_id, &output.content,
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
                        &mut tx, tenant_id, mid, &citations,
                    )
                    .await?;
                }
                tx.commit().await?;
                mid
            };

            let retrieval_top = assembled.retrieved_chunks.first().map(|c| c.similarity as f32);
            let rec = GenerationRecord {
                id: generation_id,
                tenant_id,
                conversation_id,
                trigger_message_id,
                response_message_id: Some(mid),
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

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;

            let completed_msg = serde_json::json!({
                "id": mid, "kind": "ai", "body": output.content, "confidence": null,
            });
            presence.broadcast(
                tenant_id,
                escalations::presence::Event::ConversationAi(
                    escalations::model::ConversationAiEvent::Completed(
                        escalations::model::ConversationAiCompleted {
                            conversation_id,
                            generation_id,
                            message: completed_msg,
                        },
                    ),
                ),
            );
            tracing::info!("engine: reply sent");
        }
        Ok(_) | Err(GenerateError::Superseded { .. }) => {
            // Superseded — newer message, escalation, or idempotency hit
            let reason = match &gen_result {
                Err(GenerateError::Superseded { reason }) => reason.clone(),
                _ => {
                    if has_newer { "newer_message".into() }
                    else if has_esc { "escalated".into() }
                    else { "already_replied".into() }
                }
            };
            let outcome = if has_esc || reason == "escalated" {
                GenerationOutcome::CancelledEscalation
            } else {
                GenerationOutcome::Superseded
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
                error_category: None,
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

            presence.broadcast(
                tenant_id,
                escalations::presence::Event::ConversationAi(
                    escalations::model::ConversationAiEvent::Superseded(
                        escalations::model::ConversationAiSuperseded {
                            conversation_id,
                            generation_id,
                            reason: if reason == "escalated" {
                                escalations::model::SupersededReason::Escalated
                            } else {
                                escalations::model::SupersededReason::NewerMessage
                            },
                        },
                    ),
                ),
            );

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
        }
        Err(GenerateError::Provider(AiCallError::NotConfigured)) => {
            if is_platform_persona {
                let has_ack =
                    conversations::queries::has_system_message(pool, tenant_id, conversation_id)
                        .await?;
                if !has_ack {
                    let mut tx = pool.begin().await?;
                    conversations::queries::insert_auto_ack_in_tx(
                        &mut tx, tenant_id, conversation_id,
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
        Err(GenerateError::Provider(AiCallError::Provider {
            category,
            ..
        })) => {
            let fallback_body = "I'm sorry — I'm having trouble responding right now. A team member will follow up shortly.";
            let last_category = Some(category.as_str().to_string());

            let fallback_ok = match async {
                let mut tx = pool.begin().await?;
                conversations::queries::insert_fallback_in_tx(
                    &mut tx, tenant_id, conversation_id, fallback_body,
                )
                .await?;
                let present_ids = presence.present_membership_ids_async(tenant_id).await;
                let _ = escalations::routing::route_new_escalation_in_tx(
                    &mut tx, pool, tenant_id, conversation_id,
                    "AI assistant unavailable", &[], &[], &present_ids, Uuid::nil(),
                )
                .await;
                tx.commit().await.map_err(|e| { tracing::error!(%e, "engine: fallback tx commit failed"); e })
            }
            .await
            {
                Ok(()) => true,
                Err(e) => {
                    tracing::error!(%e, "engine: fallback insert itself failed");
                    false
                }
            };

            let (outcome, error_category) = if fallback_ok {
                (GenerationOutcome::Fallback, last_category)
            } else {
                (GenerationOutcome::Failed, Some("fallback_insert_failed".into()))
            };

            let rec = GenerationRecord {
                id: generation_id,
                tenant_id, conversation_id, trigger_message_id,
                response_message_id: None, usage_record_id: None,
                provider: None, model: None, outcome, error_category,
                attempts: 1, continuation_used: false,
                retrieval_chunk_count: assembled.retrieved_chunks.len() as i16,
                retrieval_top_similarity: assembled.retrieved_chunks.first().map(|c| c.similarity as f32),
                retrieval_degraded: assembled.retrieval_degraded,
                confidence_score: None, latency_ms,
                request_id: None, created_at: Some(chrono::Utc::now()),
            };
            let _ = generation_record::insert(pool, &rec).await;

            presence.broadcast(
                tenant_id,
                escalations::presence::Event::ConversationAi(
                    escalations::model::ConversationAiEvent::Failed(
                        escalations::model::ConversationAiFailed {
                            conversation_id,
                            generation_id,
                            category: match category {
                                ai_providers::ErrorCategory::Authentication => escalations::model::FailureCategory::Authentication,
                                ai_providers::ErrorCategory::RateLimited => escalations::model::FailureCategory::RateLimited,
                                ai_providers::ErrorCategory::Unavailable => escalations::model::FailureCategory::Unavailable,
                                ai_providers::ErrorCategory::Timeout => escalations::model::FailureCategory::Timeout,
                                ai_providers::ErrorCategory::InvalidRequest => escalations::model::FailureCategory::InvalidRequest,
                            },
                        },
                    ),
                ),
            );

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
        }
        Err(GenerateError::Provider(AiCallError::Internal(e))) => {
            tracing::warn!(%e, "engine: internal generation error");
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
