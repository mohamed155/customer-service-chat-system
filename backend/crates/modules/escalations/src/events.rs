use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use futures::Stream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info};
use uuid::Uuid;

use identity::Principal;
use tenancy::TenantContext;

use crate::audit;
use crate::presence;
use crate::routing;

pub struct GuardedStream {
    pub guard: presence::PresenceGuard,
    pub inner: BroadcastStream<presence::Event>,
    pub seq: u64,
    pub tenant_id: Uuid,
    pub membership_id: Uuid,
}

impl Stream for GuardedStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(presence::Event::EscalationAssigned(ev)))) => {
                self.seq += 1;
                let event_type = "escalation.assigned";
                let data = serde_json::to_string(&ev).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default()
                    .event(event_type)
                    .data(data)
                    .id(self.seq.to_string()))))
            }
            Poll::Ready(Some(Ok(presence::Event::EscalationQueued(ev)))) => {
                self.seq += 1;
                let event_type = "escalation.queued";
                let data = serde_json::to_string(&ev).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default()
                    .event(event_type)
                    .data(data)
                    .id(self.seq.to_string()))))
            }
            Poll::Ready(Some(Ok(presence::Event::EscalationRemoved(ev)))) => {
                self.seq += 1;
                let event_type = "escalation.removed";
                let data = serde_json::to_string(&ev).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default()
                    .event(event_type)
                    .data(data)
                    .id(self.seq.to_string()))))
            }
            Poll::Ready(Some(Ok(presence::Event::AvailabilityChanged(ev)))) => {
                let is_target = ev.membership_id == self.membership_id;
                if is_target {
                    self.seq += 1;
                    let event_type = "availability.changed";
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    Poll::Ready(Some(Ok(Event::default()
                        .event(event_type)
                        .data(data)
                        .id(self.seq.to_string()))))
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(Some(Ok(presence::Event::ConversationAi(ev)))) => {
                self.seq += 1;
                let (event_type, data) = match ev {
                    crate::model::ConversationAiEvent::Started(payload) => {
                        ("ai.message.started", serde_json::to_string(&payload).unwrap_or_default())
                    }
                    crate::model::ConversationAiEvent::Delta(payload) => {
                        ("ai.message.delta", serde_json::to_string(&payload).unwrap_or_default())
                    }
                    crate::model::ConversationAiEvent::Completed(payload) => {
                        ("ai.message.completed", serde_json::to_string(&payload).unwrap_or_default())
                    }
                    crate::model::ConversationAiEvent::Superseded(payload) => {
                        ("ai.message.superseded", serde_json::to_string(&payload).unwrap_or_default())
                    }
                    crate::model::ConversationAiEvent::Failed(payload) => {
                        ("ai.message.failed", serde_json::to_string(&payload).unwrap_or_default())
                    }
                };
                Poll::Ready(Some(Ok(Event::default()
                    .event(event_type)
                    .data(data)
                    .id(self.seq.to_string()))))
            }
            Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(n)))) => {
                info!(%n, "SSE stream lagged, skipping");
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Server-Sent Events stream of real-time escalations and availability
/// changes scoped to the caller's tenant.
///
/// # Event payload schemas
///
/// Each event's `data` field is the JSON serialization of one of:
/// - [`crate::model::EscalationAssignedEvent`] (SSE event `escalation.assigned`)
/// - [`crate::model::EscalationQueuedEvent`]   (SSE event `escalation.queued`)
/// - [`crate::model::EscalationRemovedEvent`]  (SSE event `escalation.removed`)
/// - [`crate::model::AvailabilityChangedEvent`] (SSE event `availability.changed`)
///
/// Keep-alive comments are emitted every 20 seconds.
#[utoipa::path(
    get,
    path = "/tenant/events",
    tag = "escalations",
    responses(
        (status = 200, description = "Server-Sent Events stream of escalations and availability changes. Content-Type: text/event-stream. Each event's data is a JSON object — see schema components for the event payload shapes.", content_type = "text/event-stream", body = String),
        (status = 401, description = "Authentication required", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn stream_events(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
) -> Response {
    let membership_id = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .bind(principal.user_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(mid)) => mid,
        Ok(None) => {
            return kernel::ApiError::not_found("Membership not found in tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id for SSE");
            return kernel::ApiError::internal_error("Failed to start event stream")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (guard, rx) = runtime.connect(ctx.tenant_id, membership_id);

    // Reconnect drain: if the agent is already available (reconnect within grace window),
    // drain one queued escalation for them.
    let pool_for_drain = pool.clone();
    let tenant_id = ctx.tenant_id;
    let runtime_for_drain = runtime.clone();
    tokio::spawn(async move {
        let avail_state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM agent_availability \
             WHERE tenant_id = $1 AND membership_id = $2",
        )
        .bind(tenant_id)
        .bind(membership_id)
        .fetch_optional(&pool_for_drain)
        .await
        .ok()
        .flatten();
        if avail_state.as_deref() == Some("available") {
            let present_ids = runtime_for_drain
                .present_membership_ids_async(tenant_id)
                .await;
            let mut tx = match pool_for_drain.begin().await {
                Ok(tx) => tx,
                Err(_) => return,
            };
            if let Err(e) = routing::drain_one_for_membership_in_tx(
                &mut tx,
                tenant_id,
                membership_id,
                &present_ids,
                Uuid::nil(),
            )
            .await
            {
                tracing::error!(%e, "reconnect drain failed");
                tx.rollback().await.ok();
                return;
            }
            tx.commit().await.ok();
        }
    });

    let stream = GuardedStream {
        guard,
        inner: BroadcastStream::new(rx),
        seq: 0,
        tenant_id: ctx.tenant_id,
        membership_id,
    };

    let sse =
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text(""));

    sse.into_response()
}

// ---------------------------------------------------------------------------
// Outbox consumer
// ---------------------------------------------------------------------------

pub async fn process_escalation_outbox_once(
    pool: &sqlx::PgPool,
    runtime: &Arc<presence::Runtime>,
) -> sqlx::Result<bool> {
    let claim_token = Uuid::new_v4();
    let maybe_row: Option<(i64, Uuid, String, serde_json::Value)> = sqlx::query_as(
        "UPDATE outbox_events \
         SET claimed_at = now(), claim_token = $1 \
         WHERE id = ( \
             SELECT id FROM outbox_events \
             WHERE event_type IN ('conversation.status_changed', 'conversation.assignment_changed') \
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

    let (event_id, tenant_id, event_type, payload) = match maybe_row {
        Some(row) => row,
        None => return Ok(false),
    };

    let origin = payload.get("origin").and_then(|v| v.as_str()).unwrap_or("");
    if origin == "escalations" {
        sqlx::query("DELETE FROM outbox_events WHERE id = $1")
            .bind(event_id)
            .execute(pool)
            .await?;
        return Ok(true);
    }

    let conversation_id: Uuid = payload
        .get("conversationId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let mut tx = pool.begin().await?;

    let result: Result<(), sqlx::Error> = async {
        match event_type.as_str() {
            "conversation.status_changed" => {
                let new_status = payload
                    .get("newStatus")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let prev_status = payload
                    .get("prevStatus")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if new_status == "resolved" || new_status == "closed" {
                    close_active_escalation_in_tx(
                        &mut tx,
                        tenant_id,
                        conversation_id,
                        new_status,
                        runtime,
                    )
                    .await?;

                    // Freed load: try to drain a queued escalation
                    if prev_status == "open" || prev_status == "pending" {
                        let present_ids = runtime.present_membership_ids_async(tenant_id).await;
                        routing::drain_any_in_tx(&mut tx, tenant_id, &present_ids, Uuid::nil())
                            .await?;
                    }
                }
            }
            "conversation.assignment_changed" => {
                let new_membership = payload
                    .get("newMembershipId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());
                let old_membership = payload
                    .get("oldMembershipId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());
                relabel_reassignment_in_tx(
                    &mut tx,
                    tenant_id,
                    conversation_id,
                    new_membership,
                    runtime,
                )
                .await?;

                // Freed load: old assignee lost a conversation
                if old_membership.is_some() && old_membership != new_membership {
                    let present_ids = runtime.present_membership_ids_async(tenant_id).await;
                    routing::drain_any_in_tx(&mut tx, tenant_id, &present_ids, Uuid::nil()).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            tx.commit().await?;
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(pool)
                .await?;
            Ok(true)
        }
        Err(e) => {
            tx.rollback().await?;
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

async fn close_active_escalation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    cause: &str,
    runtime: &Arc<presence::Runtime>,
) -> sqlx::Result<()> {
    let escalation_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE escalations SET status = 'closed', closed_at = now() \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status IN ('queued', 'assigned') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(eid) = escalation_id {
        conversations::queries::set_escalated_in_tx(tx, tenant_id, conversation_id, None).await?;
        audit::record_escalation_closed(tx, Uuid::nil(), tenant_id, eid, cause).await?;
        let notification =
            presence::Event::EscalationRemoved(crate::model::EscalationRemovedEvent {
                v: 1,
                escalation_id: eid,
                cause: "closed".into(),
            });
        runtime.broadcast(tenant_id, notification);
    }
    Ok(())
}

async fn relabel_reassignment_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    new_membership_id: Option<Uuid>,
    runtime: &Arc<presence::Runtime>,
) -> sqlx::Result<()> {
    // Case 1: queued escalation → transition to assigned (CHECK requires all assignment columns)
    if let Some(mid) = new_membership_id {
        let queued: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE escalations SET status = 'assigned', routing_reason = 'manual_reassignment', \
             assigned_membership_id = $1, assigned_at = now() \
             WHERE tenant_id = $2 AND conversation_id = $3 \
             AND status = 'queued' \
             RETURNING id",
        )
        .bind(mid)
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some((eid,)) = queued {
            audit::record_escalation_assigned(
                tx,
                Uuid::nil(),
                tenant_id,
                eid,
                "manual_reassignment",
                &[],
                0,
                mid,
            )
            .await?;
            let notification =
                presence::Event::EscalationAssigned(crate::model::EscalationAssignedEvent {
                    v: 1,
                    escalation_id: eid,
                    conversation_id,
                    reason: String::new(),
                    routing_reason: crate::model::RoutingReason::ManualReassignment,
                    matched_skills: Vec::new(),
                    assigned_at: chrono::Utc::now(),
                });
            runtime.broadcast(tenant_id, notification);
            return Ok(());
        }
    }

    // Case 2: already-assigned escalation → relabel
    let escalation_id: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE escalations SET routing_reason = 'manual_reassignment', \
         assigned_membership_id = COALESCE($3, assigned_membership_id), \
         assigned_at = COALESCE(assigned_at, now()) \
         WHERE tenant_id = $1 AND conversation_id = $2 \
         AND status = 'assigned' \
         AND routing_reason IS DISTINCT FROM 'manual_reassignment' \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(new_membership_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some((eid,)) = escalation_id {
        if let Some(mid) = new_membership_id {
            audit::record_escalation_assigned(
                tx,
                Uuid::nil(),
                tenant_id,
                eid,
                "manual_reassignment",
                &[],
                0,
                mid,
            )
            .await?;
            let notification =
                presence::Event::EscalationAssigned(crate::model::EscalationAssignedEvent {
                    v: 1,
                    escalation_id: eid,
                    conversation_id,
                    reason: String::new(),
                    routing_reason: crate::model::RoutingReason::ManualReassignment,
                    matched_skills: Vec::new(),
                    assigned_at: chrono::Utc::now(),
                });
            runtime.broadcast(tenant_id, notification);
        }
    }
    Ok(())
}

pub async fn run_escalation_outbox_worker(
    pool: sqlx::PgPool,
    runtime: Arc<presence::Runtime>,
) -> ! {
    loop {
        match process_escalation_outbox_once(&pool, &runtime).await {
            Ok(true) => {}
            Ok(false) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                error!(%e, "escalation outbox consumer error");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
