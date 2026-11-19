use std::sync::Arc;
use std::time::Duration;

use escalations::presence;
use sqlx::PgPool;
use sqlx::Row;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::emit::NotificationRequest;
use crate::model::{NotificationKind, SubjectType};
use crate::{queries, recipients};

// ── Deserialization helpers ──────────────────────────────────────────

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationRequestedPayload {
    tenant_id: Uuid,
    kind: NotificationKind,
    subject_type: String,
    subject_id: Uuid,
    actor_membership_id: Option<Uuid>,
    target_membership_id: Option<Uuid>,
    dedupe_key: String,
    title: String,
    body: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationResolvedPayload {
    tenant_id: Uuid,
    subject_type: String,
    subject_id: Uuid,
    resolved_by_membership_id: Option<Uuid>,
}

/// Events the AI engine publishes (`ai::outbox`, MOH-195). `notifications` reads them by name and
/// shape, not through the `ai` crate: the event type and the keys are the contract, and a server
/// test fails if the two sides drift.
///
/// These payload structs read only the keys a notification needs and ignore the rest (no
/// `deny_unknown_fields`) on purpose. Events outlive the code that wrote them: a producer that adds
/// a key must not make an older consumer reject every event until it is redeployed, and other
/// consumers may read keys this one does not (`conversationId` on a tool approval).
pub const AI_TOOL_APPROVAL_REQUESTED: &str = "ai.tool_approval_requested";
pub const AI_RESPONSE_FAILED: &str = "ai.response_failed";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolApprovalRequestedPayload {
    tenant_id: Uuid,
    tool_request_id: Uuid,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseFailedPayload {
    tenant_id: Uuid,
    conversation_id: Uuid,
    failed_at: chrono::DateTime<chrono::Utc>,
}

/// What a notification says for each AI event. The wording, the kind and the dedupe rule are
/// `notifications`' to decide: `ai` only says what happened.
fn notification_for_tool_approval(p: &ToolApprovalRequestedPayload) -> NotificationRequest {
    NotificationRequest {
        tenant_id: p.tenant_id,
        kind: NotificationKind::ToolApprovalRequired,
        subject_type: SubjectType::ToolRequest,
        subject_id: p.tool_request_id,
        actor_membership_id: None,
        target_membership_id: None,
        dedupe_key: crate::emit::dedupe_key_tool_approval(p.tool_request_id),
        title: "Tool approval required".into(),
        body: Some("A tool action requires your approval.".into()),
    }
}

/// The dedupe key buckets by the time the failure happened (the event's own, so a redelivered
/// event lands in the same bucket).
fn notification_for_failed_response(p: &ResponseFailedPayload) -> NotificationRequest {
    NotificationRequest {
        tenant_id: p.tenant_id,
        kind: NotificationKind::AiResponseFailed,
        subject_type: SubjectType::Conversation,
        subject_id: p.conversation_id,
        actor_membership_id: None,
        target_membership_id: None,
        dedupe_key: crate::emit::dedupe_key_ai_failed(p.conversation_id, &p.failed_at),
        title: "AI response failed".into(),
        body: Some("The AI was unable to generate a response for this conversation.".into()),
    }
}

fn parse_subject_type(s: &str) -> Result<SubjectType, sqlx::Error> {
    match s {
        "conversation" => Ok(SubjectType::Conversation),
        "escalation" => Ok(SubjectType::Escalation),
        "tool_request" => Ok(SubjectType::ToolRequest),
        _ => Err(sqlx::Error::Protocol(format!("invalid subject_type: {s}"))),
    }
}

// ── Broadcast helpers ────────────────────────────────────────────────

fn broadcast_notification_created(
    presence: &presence::Runtime,
    tenant_id: Uuid,
    membership_id: Uuid,
    notification_id: Uuid,
    unread_count: i64,
) {
    let event = presence::Event::NotificationCreated(presence::NotificationBadgeEvent {
        membership_id,
        notification_id: Some(notification_id),
        unread_count,
    });
    presence.broadcast(tenant_id, event);
}

fn broadcast_notification_cleared(
    presence: &presence::Runtime,
    tenant_id: Uuid,
    membership_id: Uuid,
    unread_count: i64,
) {
    let event = presence::Event::NotificationCleared(presence::NotificationBadgeEvent {
        membership_id,
        notification_id: None,
        unread_count,
    });
    presence.broadcast(tenant_id, event);
}

/// Creates the notification a request asks for, for each person who should get it, and tells their
/// open dashboards the new unread count. Used for `notification.requested` and for the AI events.
async fn fan_out_request(
    pool: &PgPool,
    presence: &Arc<presence::Runtime>,
    request: NotificationRequest,
) -> Result<(), sqlx::Error> {
    let recipients = recipients::resolve(
        pool,
        request.tenant_id,
        &request.kind,
        request.subject_id,
        request.actor_membership_id,
        request.target_membership_id,
    )
    .await?;

    if recipients.is_empty() {
        debug!(
            tenant_id = %request.tenant_id,
            kind = ?request.kind,
            subject_id = %request.subject_id,
            "notification request: no recipients resolved"
        );
        return Ok(());
    }

    let inserted = queries::fan_out(pool, &request, &recipients).await?;

    info!(
        count = inserted.len(),
        tenant_id = %request.tenant_id,
        kind = ?request.kind,
        subject_id = %request.subject_id,
        "notification request: created notifications"
    );

    for (notification_id, membership_id) in inserted {
        let count = queries::unread_count(pool, request.tenant_id, membership_id).await?;
        broadcast_notification_created(
            presence,
            request.tenant_id,
            membership_id,
            notification_id,
            count,
        );
    }
    Ok(())
}

// ── Outbox consumer ─────────────────────────────────────────────────

pub async fn process_notification_outbox_once(
    pool: &PgPool,
    presence: &Arc<presence::Runtime>,
) -> Result<bool, sqlx::Error> {
    // uuid-v4-ok: claim token, not a key
    let claim_token = Uuid::new_v4();
    // Across tenants, so it goes through the audited function `xtenant.outbox_claim`.
    let maybe_row =
        sqlx::query("SELECT id, tenant_id, event_type, payload FROM xtenant.outbox_claim($1, $2)")
            .bind(
                &[
                    "notification.requested",
                    "notification.resolved",
                    AI_TOOL_APPROVAL_REQUESTED,
                    AI_RESPONSE_FAILED,
                ][..],
            )
            .bind(claim_token)
            .fetch_optional(pool)
            .await?;

    let row = match maybe_row {
        Some(r) => r,
        None => return Ok(false),
    };

    let event_id: Uuid = row.get("id");
    let tenant_id_str: String = row.get("tenant_id");
    let _tenant_id = match Uuid::parse_str(&tenant_id_str) {
        Ok(id) => id,
        Err(_) => {
            error!(event_id = %event_id, tenant_id_str = %tenant_id_str, "notification outbox: invalid tenant_id in event");
            sqlx::query("SELECT xtenant.outbox_ack($1)")
                .bind(event_id)
                .execute(pool)
                .await?;
            return Ok(true);
        }
    };
    let event_type: String = row.get("event_type");
    let payload: serde_json::Value = row.get("payload");

    let result: Result<(), sqlx::Error> = async {
        match event_type.as_str() {
            "notification.requested" => {
                let req: NotificationRequestedPayload = serde_json::from_value(payload)
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                let subject_type = parse_subject_type(&req.subject_type)?;
                fan_out_request(
                    pool,
                    presence,
                    NotificationRequest {
                        tenant_id: req.tenant_id,
                        kind: req.kind,
                        subject_type,
                        subject_id: req.subject_id,
                        actor_membership_id: req.actor_membership_id,
                        target_membership_id: req.target_membership_id,
                        dedupe_key: req.dedupe_key,
                        title: req.title,
                        body: req.body,
                    },
                )
                .await?;
            }
            AI_TOOL_APPROVAL_REQUESTED => {
                let ev: ToolApprovalRequestedPayload = serde_json::from_value(payload)
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                fan_out_request(pool, presence, notification_for_tool_approval(&ev)).await?;
            }
            AI_RESPONSE_FAILED => {
                let ev: ResponseFailedPayload = serde_json::from_value(payload)
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                fan_out_request(pool, presence, notification_for_failed_response(&ev)).await?;
            }
            "notification.resolved" => {
                let ev: NotificationResolvedPayload = serde_json::from_value(payload)
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

                let subj_type = parse_subject_type(&ev.subject_type)?;
                let affected = queries::resolve_subject(
                    pool,
                    ev.tenant_id,
                    &subj_type,
                    ev.subject_id,
                    ev.resolved_by_membership_id,
                )
                .await?;

                info!(
                    count = affected.len(),
                    tenant_id = %ev.tenant_id,
                    subject_type = %ev.subject_type,
                    subject_id = %ev.subject_id,
                    "notification.resolved: resolved notifications"
                );

                for membership_id in affected {
                    let count = queries::unread_count(pool, ev.tenant_id, membership_id).await?;
                    broadcast_notification_cleared(presence, ev.tenant_id, membership_id, count);
                }
            }
            _ => {
                error!(
                    event_type = %event_type,
                    "notification outbox: unexpected event type, deleting"
                );
            }
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            sqlx::query("SELECT xtenant.outbox_ack($1)")
                .bind(event_id)
                .execute(pool)
                .await?;
            Ok(true)
        }
        Err(e) => {
            error!(error = %e, event_type = %event_type, event_id = %event_id, "notification outbox processing error");
            sqlx::query("SELECT xtenant.outbox_release($1)")
                .bind(event_id)
                .execute(pool)
                .await?;
            Err(e)
        }
    }
}

// ── Worker loop ──────────────────────────────────────────────────────

pub async fn run_notification_outbox_worker(pool: PgPool, presence: Arc<presence::Runtime>) -> ! {
    tracing::info!("notifications outbox worker started");
    loop {
        match process_notification_outbox_once(&pool, &presence).await {
            Ok(true) => {}
            Ok(false) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                error!(error = %e, "notification outbox worker error, will retry in 5s");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
