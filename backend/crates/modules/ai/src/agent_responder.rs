use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::agent_config::{self, AgentConfigurationRow, EscalationRule};
use crate::agent_rules::{self, RuleMatch, BASELINE_ESCALATION_REASON};
use tools::approval::cancel_pending_for_conversation;

pub async fn process_agent_responder_once(
    pool: &PgPool,
    ai: &crate::AiService,
    presence: &Arc<escalations::presence::Runtime>,
) -> sqlx::Result<bool> {
    // Phase A — Claim + Read (no long-held locks)

    // 1. Claim one unprocessed outbox_events row
    let claim_token = Uuid::new_v4();
    let maybe_row: Option<(i64, Uuid, String, String, serde_json::Value)> = sqlx::query_as(
        "UPDATE outbox_events \
         SET claimed_at = now(), claim_token = $1 \
         WHERE id = ( \
             SELECT id FROM outbox_events \
             WHERE event_type IN ('conversation.customer_message', 'ai.tool_decision') \
             AND claimed_at IS NULL \
             ORDER BY created_at ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id, tenant_id, aggregate_id, event_type, payload",
    )
    .bind(claim_token)
    .fetch_optional(pool)
    .await?;

    let (event_id, tenant_id, _aggregate_id, event_type, payload) = match maybe_row {
        Some(row) => row,
        None => return Ok(false),
    };

    // Handle ai.tool_decision events — dispatch to follow-up generation
    if event_type.as_str() == "ai.tool_decision" {
        let conversation_id: Uuid = payload["conversationId"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                sqlx::Error::Protocol("missing conversationId in ai.tool_decision payload".into())
            })?;

        let tool_request_id: Uuid = payload["toolRequestId"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                sqlx::Error::Protocol("missing toolRequestId in ai.tool_decision payload".into())
            })?;

        let outcome: String = payload["outcome"].as_str().unwrap_or("").to_string();

        crate::engine::run_followup_generation(
            pool,
            ai,
            presence,
            tenant_id,
            conversation_id,
            tool_request_id,
            &outcome,
        )
        .await?;

        sqlx::query("DELETE FROM outbox_events WHERE id = $1")
            .bind(event_id)
            .execute(pool)
            .await?;
        return Ok(true);
    }

    let conversation_id: Uuid = payload["conversation_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| sqlx::Error::Protocol("missing conversation_id in outbox payload".into()))?;

    let message_id: Uuid = payload["message_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| sqlx::Error::Protocol("missing message_id in outbox payload".into()))?;

    let channel: String = payload["channel"].as_str().unwrap_or("").to_string();

    // 2a. Claim-time coalescing: delete older unclaimed events for the same
    // conversation (their content is already in the history the engine loads)
    let _ = sqlx::query(
        "DELETE FROM outbox_events \
         WHERE event_type = 'conversation.customer_message' \
         AND claimed_at IS NULL \
         AND payload->>'conversation_id' = $1::text \
         AND id != $2",
    )
    .bind(conversation_id.to_string())
    .bind(event_id)
    .execute(pool)
    .await;

    // 2b. Load live agent config
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
                    let ack_mid = conversations::queries::insert_auto_ack_in_tx(
                        &mut tx,
                        tenant_id,
                        conversation_id,
                        "Thank you for your message. A team member will be with you shortly.",
                    )
                    .await?;
                    if channel == "whatsapp" {
                        conversations::outbox::emit_whatsapp_outbound_in_tx(
                            &mut tx, tenant_id, conversation_id, ack_mid,
                        ).await?;
                    }
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
                // Cancel any pending tool requests for this conversation
                if let Ok(ids) =
                    cancel_pending_for_conversation(&mut tx, tenant_id, conversation_id).await
                {
                    for id in &ids {
                        let updated_ev = escalations::model::ToolRequestUpdated {
                            id: *id,
                            conversation_id,
                            status: "cancelled".into(),
                            decided_by_display_name: None,
                            duration_ms: None,
                            has_result: false,
                            error: None,
                        };
                        presence.broadcast(
                            tenant_id,
                            escalations::presence::Event::ConversationTool(
                                escalations::presence::ConversationToolEvent::Updated(updated_ev),
                            ),
                        );
                    }
                }
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
                // Cancel any pending tool requests for this conversation
                if let Ok(ids) =
                    cancel_pending_for_conversation(&mut tx, tenant_id, conversation_id).await
                {
                    for id in &ids {
                        let updated_ev = escalations::model::ToolRequestUpdated {
                            id: *id,
                            conversation_id,
                            status: "cancelled".into(),
                            decided_by_display_name: None,
                            duration_ms: None,
                            has_result: false,
                            error: None,
                        };
                        presence.broadcast(
                            tenant_id,
                            escalations::presence::Event::ConversationTool(
                                escalations::presence::ConversationToolEvent::Updated(updated_ev),
                            ),
                        );
                    }
                }
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

    // Phase B + C — Delegate to the engine
    crate::engine::run_generation(
        pool,
        ai,
        presence,
        tenant_id,
        conversation_id,
        message_id,
        event_id,
        &row,
        is_platform_persona,
        &channel,
    )
    .await?;

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
