use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use kernel::{ApiError, ApiJson, ErrorEnvelope, InMemoryRateLimitStore};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info_span};
use uuid::Uuid;

use crate::model::{
    CreateSessionPayload, PublicWidgetConfigDto, SendMessagePayload, SessionResponseDto,
    WidgetConversationDto, WidgetConversationResponse, WidgetMessageDto, WidgetMessageResponse,
    WidgetMessageResponseData,
};
use crate::origin::origin_allowed;
use crate::queries;

#[derive(Debug, Deserialize)]
pub struct ConfigQuery {
    pub widget_id: String,
}

#[utoipa::path(
    get,
    path = "/widget/v1/config",
    tag = "widget-public",
    operation_id = "get_widget_config",
    summary = "Get public widget configuration",
    params(
        ("widgetId" = String, Query, description = "Widget public ID"),
    ),
    responses(
        (status = 200, description = "Widget config.", body = PublicWidgetConfigDto),
        (status = 404, description = "Widget not found.", body = ErrorEnvelope),
        (status = 403, description = "Origin not allowed.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn get_config(
    State(pool): State<PgPool>,
    Query(query): Query<ConfigQuery>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
) -> Response {
    let span = info_span!("widget_config_lookup", widget_id = %query.widget_id);
    let _guard = span.enter();

    let instance = match queries::find_instance_by_public_id(&pool, &query.widget_id).await {
        Ok(Some(i)) => i,
        Ok(None) => return ApiError::not_found("Widget not found").into_response(),
        Err(e) => {
            error!(%e, "get_config: db error");
            return ApiError::internal_error("Failed to look up widget").into_response();
        }
    };

    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    let referer = headers.get("referer").and_then(|v| v.to_str().ok());
    if !origin_allowed(&instance.allowed_domains, origin, referer) {
        return ApiError::new_with_code(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "Origin not allowed",
        )
        .into_response();
    }

    let dto = PublicWidgetConfigDto {
        widget_id: instance.public_id,
        display_name: instance.display_name,
        primary_color: instance.primary_color,
        welcome_message: instance.welcome_message,
        position: instance.position,
        theme: instance.theme,
        enabled: instance.enabled,
    };

    (StatusCode::OK, Json(dto)).into_response()
}

#[utoipa::path(
    post,
    path = "/widget/v1/sessions",
    tag = "widget-public",
    operation_id = "create_widget_session",
    summary = "Create an anonymous widget session",
    request_body = CreateSessionPayload,
    responses(
        (status = 201, description = "Session created.", body = SessionResponseDto),
        (status = 404, description = "Widget not found.", body = ErrorEnvelope),
        (status = 403, description = "Origin not allowed.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn create_session(
    State(pool): State<PgPool>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
    ApiJson(payload): ApiJson<CreateSessionPayload>,
) -> Response {
    let span = info_span!("widget_session_mint", widget_id = %payload.widget_id);
    let _guard = span.enter();

    let instance = match queries::find_instance_by_public_id(&pool, &payload.widget_id).await {
        Ok(Some(i)) => i,
        Ok(None) => return ApiError::not_found("Widget not found").into_response(),
        Err(e) => {
            error!(%e, "create_session: db error");
            return ApiError::internal_error("Failed to look up widget").into_response();
        }
    };

    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    let referer = headers.get("referer").and_then(|v| v.to_str().ok());
    if !origin_allowed(&instance.allowed_domains, origin, referer) {
        return ApiError::new_with_code(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "Origin not allowed",
        )
        .into_response();
    }

    let token = crate::session::generate_token();
    let token_hash = crate::session::hash_token(&token);
    let expires_at =
        chrono::Utc::now() + chrono::Duration::hours(crate::session::SESSION_TTL_HOURS);

    let session = match queries::insert_session(
        &pool,
        instance.tenant_id,
        instance.id,
        &token_hash,
        expires_at,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            error!(%e, "create_session: insert failed");
            return ApiError::internal_error("Failed to create session").into_response();
        }
    };

    let dto = SessionResponseDto {
        session_token: token,
        expires_at: session.expires_at,
    };

    (StatusCode::CREATED, Json(dto)).into_response()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_message_sender(kind: &str, display_name: Option<&str>) -> (String, Option<String>) {
    match kind {
        "customer" => ("visitor".into(), None),
        "ai" => ("assistant".into(), None),
        "reply" => ("agent".into(), display_name.map(|s| s.to_owned())),
        "system" => ("system".into(), None),
        _ => ("system".into(), None),
    }
}

fn build_conversation_view(
    conv_id: Uuid,
    status: &str,
    escalated: bool,
    assigned: bool,
    team_online: bool,
    messages: Vec<conversations::model::Message>,
) -> WidgetConversationDto {
    let handling = if status == "resolved" || status == "closed" {
        "closed"
    } else if escalated || assigned {
        "human"
    } else {
        "ai"
    };

    let ended_note = handling == "closed";

    let mapped: Vec<WidgetMessageDto> = messages
        .into_iter()
        .filter(|m| m.kind != conversations::model::MessageKind::Note)
        .map(|m| {
            let kind_str = serde_json::to_value(&m.kind)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_owned()))
                .unwrap_or_default();
            let display_name = if m.kind == conversations::model::MessageKind::Reply {
                Some(m.sender.display_name.clone())
            } else {
                None
            };
            let (sender, sender_display_name) =
                map_message_sender(&kind_str, display_name.as_deref());
            WidgetMessageDto {
                id: m.id,
                sender,
                sender_display_name,
                body: m.body,
                created_at: m.created_at,
            }
        })
        .collect();

    WidgetConversationDto {
        id: conv_id,
        handling: handling.to_owned(),
        team_online,
        ended_note,
        messages: mapped,
    }
}

// ---------------------------------------------------------------------------
// T027: GET /widget/v1/conversation
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/widget/v1/conversation",
    tag = "widget-public",
    operation_id = "get_widget_conversation",
    summary = "Get the session's current conversation",
    responses(
        (status = 200, description = "Conversation view or null.", body = WidgetConversationResponse),
        (status = 401, description = "Session invalid.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn get_conversation(
    State(pool): State<PgPool>,
    Extension(runtime): Extension<Arc<escalations::presence::Runtime>>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
) -> Response {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match crate::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    if session.customer_id.is_none() {
        return (
            StatusCode::OK,
            Json(WidgetConversationResponse { data: None }),
        )
            .into_response();
    }
    let customer_id = session.customer_id.unwrap();

    let conv_row: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id, status, escalated_at IS NOT NULL, assigned_membership_id IS NOT NULL \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 \
           AND status NOT IN ('resolved', 'closed') \
           AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT 1",
    )
    .bind(session.tenant_id)
    .bind(customer_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, "get_conversation: db error");
        ApiError::internal_error("Failed to look up conversation")
    })
    .ok()
    .flatten();

    let conv = match conv_row {
        Some(c) => c,
        None => {
            return (
                StatusCode::OK,
                Json(WidgetConversationResponse { data: None }),
            )
                .into_response();
        }
    };

    let messages = conversations::queries::timeline_query_in_tx(
        &mut pool.begin().await.unwrap(),
        &pool,
        session.tenant_id,
        conv.0,
        None,
        100,
    )
    .await
    .map(|(msgs, _, _)| msgs)
    .unwrap_or_default();

    let team_online = !runtime
        .present_membership_ids_async(session.tenant_id)
        .await
        .is_empty();
    let dto = build_conversation_view(conv.0, &conv.1, conv.2, conv.3, team_online, messages);

    (
        StatusCode::OK,
        Json(WidgetConversationResponse { data: Some(dto) }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// T028: POST /widget/v1/conversations
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/widget/v1/conversations",
    tag = "widget-public",
    operation_id = "create_widget_conversation",
    summary = "Create or reuse a widget conversation",
    responses(
        (status = 201, description = "Conversation created.", body = WidgetConversationResponse),
        (status = 200, description = "Existing conversation returned.", body = WidgetConversationResponse),
        (status = 401, description = "Session invalid.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn create_conversation(
    State(pool): State<PgPool>,
    Extension(runtime): Extension<Arc<escalations::presence::Runtime>>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
) -> Response {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match crate::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(%e, "create_conversation: begin tx failed");
            return ApiError::internal_error("Failed to start transaction").into_response();
        }
    };

    let customer_id = match queries::ensure_customer_for_session(&mut tx, &pool, &session).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(%e, "create_conversation: ensure customer failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to resolve customer").into_response();
        }
    };

    let team_online = runtime
        .present_membership_ids_async(session.tenant_id)
        .await;

    let existing: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id, status, escalated_at IS NOT NULL, assigned_membership_id IS NOT NULL \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 \
           AND status NOT IN ('resolved', 'closed') \
           AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT 1",
    )
    .bind(session.tenant_id)
    .bind(customer_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(%e, "create_conversation: find existing failed");
    })
    .ok()
    .flatten();

    if let Some((conv_id, status, escalated, assigned)) = existing {
        let messages = conversations::queries::timeline_query_in_tx(
            &mut tx,
            &pool,
            session.tenant_id,
            conv_id,
            None,
            100,
        )
        .await
        .map(|(msgs, _, _)| msgs)
        .unwrap_or_default();

        if let Err(e) = tx.commit().await {
            tracing::error!(%e, "create_conversation: commit failed");
            return ApiError::internal_error("Failed to commit").into_response();
        }

        let team_online = !team_online.is_empty();
        let dto =
            build_conversation_view(conv_id, &status, escalated, assigned, team_online, messages);
        return (
            StatusCode::OK,
            Json(WidgetConversationResponse { data: Some(dto) }),
        )
            .into_response();
    }

    let conv_detail = match conversations::queries::create_conversation_in_tx(
        &mut tx,
        session.tenant_id,
        customer_id,
        "widget",
        "",
        conversations::queries::ConversationActor::Visitor { customer_id },
        Some(session.widget_instance_id),
    )
    .await
    {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(%e, "create_conversation: create failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to create conversation").into_response();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "create_conversation: commit failed");
        return ApiError::internal_error("Failed to commit").into_response();
    }

    let status_str = serde_json::to_value(&conv_detail.status)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_owned()))
        .unwrap_or_else(|| "open".to_string());

    let team_online = !team_online.is_empty();
    let dto = build_conversation_view(
        conv_detail.id,
        &status_str,
        false,
        false,
        team_online,
        Vec::new(),
    );

    (
        StatusCode::CREATED,
        Json(WidgetConversationResponse { data: Some(dto) }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// T029: POST /widget/v1/conversations/{conversationId}/messages
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/widget/v1/conversations/{conversationId}/messages",
    tag = "widget-public",
    operation_id = "send_widget_message",
    summary = "Send a message to a widget conversation",
    params(
        ("conversationId" = Uuid, Path, description = "Conversation ID"),
    ),
    request_body = SendMessagePayload,
    responses(
        (status = 201, description = "Message sent.", body = WidgetMessageResponse),
        (status = 401, description = "Session invalid.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 409, description = "Conversation closed.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 429, description = "Rate limited.", body = ErrorEnvelope),
    ),
    security(())
)]
pub async fn send_message(
    State(pool): State<PgPool>,
    Extension(store): Extension<Arc<InMemoryRateLimitStore>>,
    axum::Extension(headers): axum::Extension<axum::http::HeaderMap>,
    Path(conversation_id): Path<Uuid>,
    ApiJson(payload): ApiJson<SendMessagePayload>,
) -> Response {
    let span = info_span!("widget_message_send", conversation_id = %conversation_id);
    let _guard = span.enter();

    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    let session = match crate::session::authenticate_session(&pool, auth).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    // Per-session message rate limit (10/min, matches MESSAGES_PER_SESSION_LIMIT)
    if !store.check(
        &format!("session:{}", session.id),
        10,
        std::time::Duration::from_secs(60),
    ) {
        return ApiError::rate_limited("Too many messages").into_response();
    }
    // Global per-tenant rate limit (600/min, matches GLOBAL_TENANT_LIMIT)
    if !store.check(
        &format!("tenant:{}", session.tenant_id),
        600,
        std::time::Duration::from_secs(60),
    ) {
        return ApiError::rate_limited("Too many requests").into_response();
    }

    let body = payload.body.trim().to_string();
    if body.is_empty() || body.len() > 4000 {
        return ApiError::unprocessable_entity("Body must be between 1 and 4000 characters")
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            error!(%e, "send_message: begin tx failed");
            return ApiError::internal_error("Failed to start transaction").into_response();
        }
    };

    let conv_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(session.tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        error!(%e, "send_message: select for update failed");
    })
    .ok()
    .flatten();

    let status = match conv_status {
        Some(s) => s,
        None => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Conversation not found").into_response();
        }
    };

    if status == "resolved" || status == "closed" {
        let _ = tx.rollback().await;
        return ApiError::new_with_code(
            StatusCode::CONFLICT,
            "conversation_closed",
            "Conversation is closed",
        )
        .into_response();
    }

    let customer_id = match session.customer_id {
        Some(id) => id,
        None => {
            let _ = tx.rollback().await;
            return ApiError::internal_error("Session has no customer").into_response();
        }
    };

    let (message, _status_ref) = match conversations::queries::add_message_in_tx(
        &mut tx,
        session.tenant_id,
        conversation_id,
        "customer",
        &body,
        None,
        None,
        conversations::queries::ConversationActor::Visitor { customer_id },
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            error!(%e, "send_message: add_message failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to save message").into_response();
        }
    };

    if let Err(e) = conversations::outbox::emit_customer_message_in_tx(
        &mut tx,
        session.tenant_id,
        conversation_id,
        message.id,
        "widget",
    )
    .await
    {
        error!(%e, "send_message: emit outbox failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to emit message event").into_response();
    }

    if let Err(e) = tx.commit().await {
        error!(%e, "send_message: commit failed");
        return ApiError::internal_error("Failed to commit").into_response();
    }

    let msg_dto = WidgetMessageDto {
        id: message.id,
        sender: "visitor".into(),
        sender_display_name: None,
        body: message.body,
        created_at: message.created_at,
    };

    let response = WidgetMessageResponse {
        data: WidgetMessageResponseData { message: msg_dto },
    };

    (StatusCode::CREATED, Json(response)).into_response()
}
