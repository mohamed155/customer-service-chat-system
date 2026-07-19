use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use kernel::{ApiError, ApiJson, InMemoryRateLimitStore};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info_span;
use uuid::Uuid;

use crate::model::{
    PendingFeedbackDto, PendingFeedbackResponse, SubmitFeedbackPayload, WidgetFeedbackDto,
    WidgetFeedbackResponse, WidgetFeedbackResponseData,
};
use crate::queries;

#[utoipa::path(
    post,
    path = "/widget/v1/conversations/{conversationId}/feedback",
    tag = "widget-public",
    operation_id = "submit_feedback",
    summary = "Submit feedback for an ended conversation",
    params(
        ("conversationId" = Uuid, Path, description = "Conversation ID"),
    ),
    request_body = SubmitFeedbackPayload,
    responses(
        (status = 201, description = "Feedback created.", body = WidgetFeedbackResponse),
        (status = 200, description = "Feedback already existed.", body = WidgetFeedbackResponse),
        (status = 401, description = "Session invalid.", body = kernel::ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = kernel::ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = kernel::ErrorEnvelope),
        (status = 429, description = "Rate limited.", body = kernel::ErrorEnvelope),
    ),
    security(())
)]
pub async fn submit_feedback(
    State(pool): State<PgPool>,
    Extension(store): Extension<Arc<InMemoryRateLimitStore>>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
    Path(conversation_id): Path<Uuid>,
    ApiJson(payload): ApiJson<SubmitFeedbackPayload>,
) -> Response {
    let span = info_span!("feedback_submit", conversation_id = %conversation_id);
    let _guard = span.enter();

    // Validate rating range
    if !(1..=5).contains(&payload.rating) {
        return ApiError::unprocessable_entity("Rating must be between 1 and 5").into_response();
    }

    // Validate and normalize comment
    let comment = payload.comment.as_deref().map(|c| c.trim().to_owned());
    let comment = comment
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(|c| c.to_owned());
    if let Some(ref c) = comment {
        if c.chars().count() > 2000 {
            return ApiError::unprocessable_entity("Comment must be 2000 characters or fewer")
                .into_response();
        }
    }

    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match widgets::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    // Rate limit: per-session 10/60s
    if !store.check(
        &format!("session:{}", session.id),
        10,
        std::time::Duration::from_secs(60),
    ) {
        return ApiError::rate_limited("Too many submissions, try again later").into_response();
    }

    // Rate limit: per-tenant 600/60s
    if !store.check(
        &format!("tenant:{}", session.tenant_id),
        600,
        std::time::Duration::from_secs(60),
    ) {
        return ApiError::rate_limited("Too many submissions, try again later").into_response();
    }

    let customer_id = match session.customer_id {
        Some(id) => id,
        None => return ApiError::not_found("Conversation not found").into_response(),
    };

    // Ownership + status check
    let conv = match queries::find_conversation_for_session(
        &pool,
        session.tenant_id,
        customer_id,
        conversation_id,
    )
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return ApiError::not_found("Conversation not found").into_response(),
        Err(e) => {
            tracing::error!(%e, "submit_feedback: db error");
            return ApiError::internal_error("Failed to look up conversation").into_response();
        }
    };

    let (status, channel, assigned_membership_id) = conv;

    // Must be ended
    if status != "resolved" && status != "closed" {
        return ApiError::unprocessable_entity("conversation_not_ended").into_response();
    }

    // Resolve AI agent configuration (US4)
    let agent_configuration_id =
        match queries::resolve_ai_agent_configuration(&pool, session.tenant_id, conversation_id)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(%e, "submit_feedback: resolve_ai_agent_configuration failed");
                None
            }
        };

    // Insert (idempotent)
    let result = match queries::insert_feedback(
        &pool,
        session.tenant_id,
        conversation_id,
        Some(session.id),
        &channel,
        agent_configuration_id,
        assigned_membership_id,
        payload.rating,
        comment.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "submit_feedback: insert failed");
            return ApiError::internal_error("Failed to store feedback").into_response();
        }
    };

    match result {
        Some(row) => {
            let dto = WidgetFeedbackDto {
                rating: row.rating,
                comment: row.comment,
                submitted_at: row.submitted_at,
            };
            (
                StatusCode::CREATED,
                Json(WidgetFeedbackResponse {
                    data: WidgetFeedbackResponseData { feedback: dto },
                }),
            )
                .into_response()
        }
        None => {
            // Duplicate — read back existing record
            let existing = match queries::find_feedback_by_conversation(
                &pool,
                session.tenant_id,
                conversation_id,
            )
            .await
            {
                Ok(Some(r)) => r,
                Ok(None) => {
                    return ApiError::internal_error("Feedback not found after insert")
                        .into_response();
                }
                Err(e) => {
                    tracing::error!(%e, "submit_feedback: read-back failed");
                    return ApiError::internal_error("Failed to read back feedback")
                        .into_response();
                }
            };
            let dto = WidgetFeedbackDto {
                rating: existing.rating,
                comment: existing.comment,
                submitted_at: existing.submitted_at,
            };
            (
                StatusCode::OK,
                Json(WidgetFeedbackResponse {
                    data: WidgetFeedbackResponseData { feedback: dto },
                }),
            )
                .into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/widget/v1/feedback/pending",
    tag = "widget-public",
    operation_id = "get_pending_feedback",
    summary = "Get pending feedback for the session's ended conversation",
    responses(
        (status = 200, description = "Pending feedback or null.", body = PendingFeedbackResponse),
        (status = 401, description = "Session invalid.", body = kernel::ErrorEnvelope),
    ),
    security(())
)]
pub async fn get_pending_feedback(
    State(pool): State<PgPool>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
) -> Response {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match widgets::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    let customer_id = match session.customer_id {
        Some(id) => id,
        None => {
            return (StatusCode::OK, Json(PendingFeedbackResponse { data: None })).into_response();
        }
    };

    let pending = match queries::find_pending_feedback(&pool, session.tenant_id, customer_id).await
    {
        Ok(Some((conv_id, ended_at))) => Some(PendingFeedbackDto {
            conversation_id: conv_id,
            ended_at,
        }),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(%e, "get_pending_feedback: db error");
            return ApiError::internal_error("Failed to look up pending feedback").into_response();
        }
    };

    (
        StatusCode::OK,
        Json(PendingFeedbackResponse { data: pending }),
    )
        .into_response()
}
