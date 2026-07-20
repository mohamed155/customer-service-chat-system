use std::sync::Arc;
use std::time::Duration;

use escalations::presence;
use sqlx::PgPool;
use sqlx::Row;
use tracing::error;
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

// ── Outbox consumer ─────────────────────────────────────────────────

pub async fn process_notification_outbox_once(
    pool: &PgPool,
    presence: &Arc<presence::Runtime>,
) -> Result<bool, sqlx::Error> {
    let claim_token = Uuid::new_v4();
    let maybe_row = sqlx::query(
        "UPDATE outbox_events \
         SET claimed_at = now(), claim_token = $1 \
         WHERE id = ( \
             SELECT id FROM outbox_events \
             WHERE event_type IN ('notification.requested', 'notification.resolved') \
             AND claimed_at IS NULL \
             ORDER BY created_at ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id, tenant_id, event_type, payload",
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
            error!("notification outbox: invalid tenant_id in event {event_id}");
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
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

                let subj_type = parse_subject_type(&req.subject_type)?;
                let recipients = recipients::resolve(
                    pool,
                    req.tenant_id,
                    &req.kind,
                    req.subject_id,
                    req.actor_membership_id,
                    req.target_membership_id,
                )
                .await?;

                if recipients.is_empty() {
                    return Ok(());
                }

                let notification_req = NotificationRequest {
                    tenant_id: req.tenant_id,
                    kind: req.kind,
                    subject_type: subj_type,
                    subject_id: req.subject_id,
                    actor_membership_id: req.actor_membership_id,
                    target_membership_id: req.target_membership_id,
                    dedupe_key: req.dedupe_key,
                    title: req.title,
                    body: req.body,
                };

                let inserted = queries::fan_out(pool, &notification_req, &recipients).await?;

                for (notification_id, membership_id) in inserted {
                    let count = queries::unread_count(pool, req.tenant_id, membership_id).await?;
                    broadcast_notification_created(
                        presence,
                        req.tenant_id,
                        membership_id,
                        notification_id,
                        count,
                    );
                }
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

                for membership_id in affected {
                    let count = queries::unread_count(pool, ev.tenant_id, membership_id).await?;
                    broadcast_notification_cleared(
                        presence,
                        ev.tenant_id,
                        membership_id,
                        count,
                    );
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
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
            Ok(true)
        }
        Err(e) => {
            error!(error = %e, event_type = %event_type, event_id = %event_id, "notification outbox processing error");
            sqlx::query(
                "UPDATE outbox_events SET claimed_at = NULL, claim_token = NULL WHERE id = $1",
            )
            .bind(event_id)
            .execute(pool)
            .await?;
            Err(e)
        }
    }
}

// ── Worker loop ──────────────────────────────────────────────────────

pub async fn run_notification_outbox_worker(
    pool: PgPool,
    presence: Arc<presence::Runtime>,
) -> ! {
    tracing::info!("notifications outbox worker started");
    loop {
        match process_notification_outbox_once(&pool, &presence).await {
            Ok(true) => {}
            Ok(false) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                error!(error = %e, "notification outbox worker error");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
