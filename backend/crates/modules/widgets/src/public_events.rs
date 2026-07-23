use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use futures::Stream;
use sqlx::PgPool;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, info_span};
use uuid::Uuid;

use kernel::{ApiError, ErrorEnvelope};

use crate::origin::origin_allowed;
use crate::queries;

static SSE_SUBSCRIBED: AtomicU64 = AtomicU64::new(0);
static SSE_DROPPED: AtomicU64 = AtomicU64::new(0);

struct WidgetEventStream {
    inner: BroadcastStream<escalations::presence::Event>,
    conversation_id: Uuid,
    team_online: bool,
    seq: u64,
}

impl Stream for WidgetEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(ev))) => {
                let (event_type, payload) = match self.filter_and_map(&ev) {
                    Some(result) => result,
                    None => {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                };
                self.seq += 1;
                Poll::Ready(Some(Ok(Event::default()
                    .event(event_type)
                    .data(payload)
                    .id(self.seq.to_string()))))
            }
            Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(n)))) => {
                info!(%n, "widget SSE stream lagged, skipping");
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => {
                SSE_DROPPED.fetch_add(1, Ordering::Relaxed);
                info!("widget SSE stream dropped");
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl WidgetEventStream {
    fn filter_and_map(&self, ev: &escalations::presence::Event) -> Option<(&'static str, String)> {
        match ev {
            escalations::presence::Event::ConversationAi(ai_ev) => {
                let conversation_id = match &ai_ev {
                    escalations::model::ConversationAiEvent::Started(p) => p.conversation_id,
                    escalations::model::ConversationAiEvent::Delta(p) => p.conversation_id,
                    escalations::model::ConversationAiEvent::Completed(p) => p.conversation_id,
                    escalations::model::ConversationAiEvent::Superseded(p) => p.conversation_id,
                    escalations::model::ConversationAiEvent::Failed(p) => p.conversation_id,
                };
                if conversation_id != self.conversation_id {
                    return None;
                }
                match ai_ev {
                    escalations::model::ConversationAiEvent::Delta(payload) => {
                        let data = serde_json::json!({
                            "messageId": null,
                            "text": payload.text,
                        });
                        Some(("ai.delta", serde_json::to_string(&data).unwrap_or_default()))
                    }
                    escalations::model::ConversationAiEvent::Completed(payload) => {
                        let msg_view = serde_json::json!({
                            "id": payload.message["id"],
                            "sender": "assistant",
                            "senderDisplayName": null,
                            "body": payload.message["body"],
                            "createdAt": chrono::Utc::now(),
                        });
                        let data = serde_json::json!({
                            "message": msg_view,
                        });
                        Some((
                            "message.created",
                            serde_json::to_string(&data).unwrap_or_default(),
                        ))
                    }
                    escalations::model::ConversationAiEvent::Started(_)
                    | escalations::model::ConversationAiEvent::Superseded(_)
                    | escalations::model::ConversationAiEvent::Failed(_) => None,
                }
            }
            escalations::presence::Event::EscalationAssigned(ev) => {
                if ev.conversation_id != self.conversation_id {
                    return None;
                }
                let data = serde_json::json!({
                    "handling": "human",
                    "teamOnline": self.team_online,
                });
                Some((
                    "conversation.updated",
                    serde_json::to_string(&data).unwrap_or_default(),
                ))
            }
            escalations::presence::Event::EscalationQueued(ev) => {
                if ev.conversation_id != self.conversation_id {
                    return None;
                }
                let data = serde_json::json!({
                    "handling": "human",
                    "teamOnline": self.team_online,
                });
                Some((
                    "conversation.updated",
                    serde_json::to_string(&data).unwrap_or_default(),
                ))
            }
            escalations::presence::Event::ConversationTool(_)
            | escalations::presence::Event::EscalationRemoved(_)
            | escalations::presence::Event::AvailabilityChanged(_)
            | escalations::presence::Event::NotificationCreated(_)
            | escalations::presence::Event::NotificationCleared(_)
            | escalations::presence::Event::ConversationMessageStatus(_) => None,
        }
    }
}

#[utoipa::path(
    get,
    path = "/widget/v1/conversations/{conversationId}/events",
    tag = "widget-public",
    operation_id = "stream_widget_events",
    summary = "SSE stream of widget conversation events",
    params(
        ("conversationId" = Uuid, Path, description = "Conversation ID"),
    ),
    responses(
        (status = 200, description = "SSE event stream.", content_type = "text/event-stream"),
        (status = 401, description = "Session invalid.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn stream_events(
    State(pool): State<PgPool>,
    Extension(runtime): Extension<Arc<escalations::presence::Runtime>>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
    Path(conversation_id): Path<Uuid>,
) -> Response {
    let span = info_span!("widget_sse_subscribe", conversation_id = %conversation_id);
    let _guard = span.enter();

    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match crate::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    let referer = headers.get("referer").and_then(|v| v.to_str().ok());
    let instance =
        match queries::find_instance_by_id(&pool, session.tenant_id, session.widget_instance_id)
            .await
        {
            Ok(Some(i)) => i,
            Ok(None) => {
                return ApiError::not_found("Widget instance not found").into_response();
            }
            Err(e) => {
                error!(%e, "stream_events: instance lookup failed");
                return ApiError::internal_error("Failed to look up widget instance")
                    .into_response();
            }
        };
    if !origin_allowed(&instance.allowed_domains, origin, referer) {
        return ApiError::new_with_code(
            axum::http::StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "Origin not allowed",
        )
        .into_response();
    }

    let conv_owner: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT customer_id, tenant_id FROM conversations \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(%e, "stream_events: lookup failed");
    })
    .ok()
    .flatten();

    let (customer_id, conv_tenant_id) = match conv_owner {
        Some(c) => c,
        None => return ApiError::not_found("Conversation not found").into_response(),
    };

    if conv_tenant_id != session.tenant_id || session.customer_id != Some(customer_id) {
        return ApiError::not_found("Conversation not found").into_response();
    }

    let (_guard, rx) = runtime.connect(session.tenant_id, Uuid::nil());

    let team_online = !runtime
        .present_membership_ids_async(session.tenant_id)
        .await
        .is_empty();
    let count = SSE_SUBSCRIBED.fetch_add(1, Ordering::Relaxed) + 1;
    info!(count, "widget SSE subscribed");

    let stream = WidgetEventStream {
        inner: BroadcastStream::new(rx),
        conversation_id,
        team_online,
        seq: 0,
    };

    let sse =
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text(""));

    sse.into_response()
}
