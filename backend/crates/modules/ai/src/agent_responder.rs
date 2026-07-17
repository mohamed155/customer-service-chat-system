use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use std::collections::HashMap;

use crate::agent_config::{self, AgentConfigurationRow, EscalationRule};
use crate::agent_prompt;
use crate::agent_rules::{self, RuleMatch, BASELINE_ESCALATION_REASON};
use crate::prompt_store;
use crate::prompt_validate;

pub async fn process_agent_responder_once(
    pool: &PgPool,
    ai: &crate::AiService,
    presence: &Arc<escalations::presence::Runtime>,
) -> sqlx::Result<bool> {
    // Phase A — Claim + Read (no long-held locks)

    // 1. Claim one unprocessed outbox_events row
    let claim_token = Uuid::new_v4();
    let maybe_row: Option<(i64, Uuid, String, serde_json::Value)> = sqlx::query_as(
        "UPDATE outbox_events \
         SET claimed_at = now(), claim_token = $1 \
         WHERE id = ( \
             SELECT id FROM outbox_events \
             WHERE event_type = 'conversation.customer_message' \
             AND claimed_at IS NULL \
             ORDER BY created_at ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id, tenant_id, aggregate_id, payload",
    )
    .bind(claim_token)
    .fetch_optional(pool)
    .await?;

    let (event_id, tenant_id, _aggregate_id, payload) = match maybe_row {
        Some(row) => row,
        None => return Ok(false),
    };

    let conversation_id: Uuid = payload["conversation_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| sqlx::Error::Protocol("missing conversation_id in outbox payload".into()))?;

    let message_id: Uuid = payload["message_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| sqlx::Error::Protocol("missing message_id in outbox payload".into()))?;

    let channel: String = payload["channel"].as_str().unwrap_or("").to_string();

    // 2. Load live agent config
    let live_config = agent_config::load_live(pool, tenant_id).await?;

    let (row, is_platform_persona) = match live_config {
        Some(cfg) => (cfg, false),
        None => {
            // 3. Unconfigured tenant
            let state =
                conversations::queries::conversation_ai_state(pool, tenant_id, conversation_id)
                    .await?;
            let (status, ai_handling) = match state {
                Some(s) => s,
                None => {
                    // Conversation not found — delete event
                    sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                        .bind(event_id)
                        .execute(pool)
                        .await?;
                    return Ok(true);
                }
            };

            if matches!(status.as_str(), "resolved" | "closed")
                || ai_handling.as_deref() == Some("human")
            {
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }

            if ai_handling.is_none() {
                if !conversations::queries::has_system_message(pool, tenant_id, conversation_id)
                    .await?
                {
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
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }

            // ai_handling = 'platform_ai' — use a platform default persona
            let platform_row = AgentConfigurationRow {
                id: Uuid::nil(),
                tenant_id,
                name: "Assistant".into(),
                is_default: false,
                avatar_kind: "none".into(),
                avatar_preset: None,
                tone: "professional".into(),
                business_rules: serde_json::Value::Array(Vec::new()),
                escalation_rules: serde_json::Value::Array(Vec::new()),
                enabled_channels: serde_json::Value::Array(Vec::new()),
                provider: None,
                model: None,
                version: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                deleted_at: None,
            };
            (platform_row, true)
        }
    };

    // 4. Configured tenant — gates
    if !is_platform_persona {
        // Gate on channel
        let channels: Vec<String> =
            serde_json::from_value(row.enabled_channels.clone()).unwrap_or_default();
        if channels.is_empty() || !channels.contains(&channel) {
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
            return Ok(true);
        }

        // Gate on conversation state
        let state =
            conversations::queries::conversation_ai_state(pool, tenant_id, conversation_id).await?;
        if let Some((status, _)) = state {
            if matches!(status.as_str(), "resolved" | "closed") {
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }
        }

        // Gate on open escalation
        if escalations::routing::has_open_escalation(pool, tenant_id, conversation_id).await? {
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
            return Ok(true);
        }

        // Get message body
        let body = conversations::queries::message_body(pool, tenant_id, message_id).await?;
        let body = match body {
            Some(b) => b,
            None => {
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }
        };

        // Parse escalation rules and evaluate
        let rules: Vec<EscalationRule> =
            serde_json::from_value(row.escalation_rules.clone()).unwrap_or_default();
        let rule_match = agent_rules::evaluate(&body, &rules);

        match rule_match {
            RuleMatch::Baseline => {
                let present_ids = presence.present_membership_ids_async(tenant_id).await;
                let mut tx = pool.begin().await?;
                let outcome = escalations::routing::route_new_escalation_in_tx(
                    &mut tx,
                    pool,
                    tenant_id,
                    conversation_id,
                    BASELINE_ESCALATION_REASON,
                    &[],
                    &[],
                    &present_ids,
                    Uuid::nil(),
                )
                .await;
                handle_routing_outcome(outcome, tx, pool).await;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }
            RuleMatch::Tenant {
                rule_name,
                required_skill_ids,
                ..
            } => {
                let present_ids = presence.present_membership_ids_async(tenant_id).await;
                let mut tx = pool.begin().await?;
                let outcome = escalations::routing::route_new_escalation_in_tx(
                    &mut tx,
                    pool,
                    tenant_id,
                    conversation_id,
                    &rule_name,
                    &required_skill_ids,
                    &[],
                    &present_ids,
                    Uuid::nil(),
                )
                .await;
                handle_routing_outcome(outcome, tx, pool).await;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event_id)
                    .execute(pool)
                    .await?;
                return Ok(true);
            }
            RuleMatch::None => {}
        }
    }

    // Phase B — Vendor call (no transaction, no lock)

    // 5. Load prompt content
    let prompt_bootstrap = prompt_store::load_bootstrap(pool, tenant_id).await?;
    let (prompt_content, prompt_version) = match &prompt_bootstrap {
        Some((p, v)) => (v.content.clone(), p.active_version),
        None => (String::new(), 0_i32),
    };

    // 6. Compose system message with variable substitution
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
        vars.insert("channel", channel.clone());

        prompt_validate::render_prompt(&prompt_content, &vars)
    };

    let system_message = agent_prompt::compose_system_message(
        &row.name,
        &system_content,
        &row.tone,
        &business_rules,
    );

    // 7. Get recent history
    let history =
        conversations::queries::recent_history(pool, tenant_id, conversation_id, 20).await?;

    // T024: Build context-aware search query from customer messages
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
            ai_providers::Message {
                role,
                content: body,
            }
        })
        .collect();

    let mut input = crate::AiInput {
        system: Some(system_message),
        messages,
    };

    // T025: Insert retrieval step
    let mut degraded = false;
    let mut retrieved_chunks: Vec<knowledge::retrieval::RetrievedChunk> = Vec::new();

    if !query_string.is_empty() {
        let retrieval_start = std::time::Instant::now();
        let embed_ctx = crate::AiCallContext {
            tenant_id,
            request_id: None,
        };
        let query_len = query_string.len();

        let retrieval_result = tokio::time::timeout(
            std::time::Duration::from_millis(800),
            async move {
                let embeddings = ai.embed_platform(embed_ctx, vec![query_string]).await?;
                let embedding = embeddings.into_iter().next().ok_or_else(|| {
                    crate::AiCallError::Internal("empty embedding result".into())
                })?;
                knowledge::retrieval::search(pool, tenant_id, &embedding, 5, 0.70)
                    .await
                    .map_err(|e| crate::AiCallError::Internal(e.to_string()))
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

        // T026: Inject passages as system message block
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

        // T027: Emit rag.retrieve tracing span
        let candidates = retrieved_chunks.len();
        let top_score = retrieved_chunks
            .first()
            .map(|c| c.similarity)
            .unwrap_or(0.0);
        tracing::info!(
            target: "rag",
            tenant_id = %tenant_id,
            conversation_id = %conversation_id,
            message_id = %message_id,
            query_len = query_len,
            candidates = candidates,
            returned = candidates,
            top_score = top_score,
            elapsed_ms = elapsed_ms,
            degraded = degraded,
            "rag.retrieve"
        );
    }

    let ctx = crate::AiCallContext {
        tenant_id,
        request_id: None,
    };

    // 8. Resolve provider/model and call AI
    let vendor_result = if let (Some(provider), Some(model)) = (&row.provider, &row.model) {
        if agent_config::credential_resolves(pool, tenant_id, provider).await {
            ai.complete_with_override(ctx, input, provider, model).await
        } else {
            ai.complete(ctx, input).await
        }
    } else {
        ai.complete(ctx, input).await
    };

    let reply_body = match vendor_result {
        Ok(result) => result.content,
        Err(crate::AiCallError::NotConfigured) => {
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
            return Ok(true);
        }
        Err(e) => {
            tracing::warn!(?e, "agent responder: vendor AI call failed");
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
            return Ok(true);
        }
    };

    // Phase C — Insert reply (short transaction)

    // 9. Idempotency check
    let already_replied =
        conversations::queries::has_ai_reply_since(pool, tenant_id, conversation_id, message_id)
            .await?;

    if !already_replied {
        let mut tx = pool.begin().await?;
        let ai_message_id = conversations::queries::insert_ai_reply_in_tx(
            &mut tx,
            tenant_id,
            conversation_id,
            &reply_body,
        )
        .await?;

        // T032: Persist citations when retrieval produced chunks
        if !retrieved_chunks.is_empty() {
            let citations: Vec<conversations::model::CitationToInsert> = retrieved_chunks
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
    }

    // Delete outbox row
    sqlx::query("DELETE FROM outbox_events WHERE id = $1")
        .bind(event_id)
        .execute(pool)
        .await?;

    tracing::info!(prompt_version, "agent responder: reply sent");
    Ok(true)
}

async fn handle_routing_outcome(
    outcome: Result<escalations::routing::RouteOutcome, escalations::routing::RouteError>,
    tx: sqlx::Transaction<'_, sqlx::Postgres>,
    _pool: &PgPool,
) {
    match outcome {
        Ok(_) => {
            if let Err(e) = tx.commit().await {
                tracing::error!(?e, "agent responder: commit escalation routing failed");
            }
        }
        Err(e) => {
            tracing::warn!(?e, "agent responder: escalation routing failed");
            let _ = tx.rollback().await;
        }
    }
}

pub async fn run_agent_responder_worker(
    pool: PgPool,
    ai: crate::AiService,
    presence: Arc<escalations::presence::Runtime>,
) -> ! {
    loop {
        match process_agent_responder_once(&pool, &ai, &presence).await {
            Ok(true) => {}
            Ok(false) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(e) => {
                tracing::error!(%e, "agent responder consumer error");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
