use super::{model, queries};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use identity::Principal;
use kernel::{ApiError, ApiJson};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Query parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InboxQueryParams {
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub channel: Option<String>,
    pub escalated: Option<bool>,
    pub cursor: Option<String>,
    pub limit: u32,
}

impl Default for InboxQueryParams {
    fn default() -> Self {
        Self {
            status: None,
            assignee: None,
            channel: None,
            escalated: None,
            cursor: None,
            limit: 25,
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct Pagination {
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PaginatedResponse<T: Serialize> {
    data: Vec<T>,
    pagination: Pagination,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_from_str(s: &str) -> model::ConversationStatus {
    serde_json::from_value(Value::String(s.to_owned()))
        .unwrap_or(model::ConversationStatus::Open)
}

fn kind_from_str(s: &str) -> model::MessageKind {
    serde_json::from_value(Value::String(s.to_owned()))
        .unwrap_or(model::MessageKind::Reply)
}

fn row_to_conversation(row: queries::InboxRow) -> model::Conversation {
    let status = status_from_str(&row.status);
    let assignee = row.assigned_membership_id.map(|mid| model::Assignee {
        membership_id: mid,
        display_name: row.assignee_display_name.unwrap_or_default(),
        active: row.assignee_active.unwrap_or(false),
    });
    let last_message = row.last_message_kind.map(|kind| {
        let preview = row.last_message_preview.unwrap_or_default();
        model::LastMessagePreview {
            kind: kind_from_str(&kind),
            preview,
        }
    });

    model::Conversation {
        id: row.id,
        customer: model::CustomerRef {
            id: row.customer_id,
            display_name: row.customer_display_name,
        },
        channel: row.channel,
        status,
        assignee,
        last_message,
        last_activity_at: row.last_activity_at,
        created_at: row.created_at,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /tenant/conversations` — inbox list.
///
/// Returns a keyset-paginated, filterable list of active conversations
/// for the current tenant, ordered by `last_activity_at DESC, id DESC`.
pub async fn list_conversations(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<Principal>,
    Query(params): Query<InboxQueryParams>,
) -> Response {
    // Validate filter values that must match a known vocabulary or format.
    if let Some(ref status) = params.status {
        if status != "all" {
            // Try as a valid status value via serde round-trip
            let status_ok =
                serde_json::from_value::<model::ConversationStatus>(json!(status)).is_ok();
            if !status_ok {
                return ApiError::unprocessable_entity("Invalid status filter")
                    .with_details(vec![json!({
                        "field": "status",
                        "code": "invalid_value",
                        "message": format!("Unknown status '{status}'. Valid values: open, pending, resolved, closed, all"),
                    })])
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    }

    if let Some(ref assignee) = params.assignee {
        match assignee.as_str() {
            "me" | "unassigned" => {}
            uuid_str => {
                if Uuid::parse_str(uuid_str).is_err() {
                    return ApiError::unprocessable_entity("Invalid assignee filter")
                        .with_details(vec![json!({
                            "field": "assignee",
                            "code": "invalid_value",
                            "message": format!("Unknown assignee '{assignee}'. Valid values: me, unassigned, or a membership UUID"),
                        })])
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }
        }
    }

    if let Some(ref channel) = params.channel {
        // Valid channels (from existing constraint)
        let valid_channels = ["email", "phone", "web_chat", "whatsapp", "telegram"];
        if !valid_channels.contains(&channel.as_str()) {
            return ApiError::unprocessable_entity("Invalid channel filter")
                .with_details(vec![json!({
                    "field": "channel",
                    "code": "invalid_value",
                    "message": format!("Unknown channel '{channel}'. Valid values: email, phone, web_chat, whatsapp, telegram"),
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let limit = params.limit.clamp(1, 100) as i64;

    // Look up the acting member's membership_id for `assignee=me`.
    let acting_membership_id = match sqlx::query_scalar::<_, Uuid>(
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
            return ApiError::not_found("User not found in tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to look up membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin inbox transaction");
            return ApiError::internal_error("Failed to load conversations")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (rows, has_more) = match queries::inbox_query(
        &mut tx,
        ctx.tenant_id,
        acting_membership_id,
        params.status,
        params.assignee,
        params.channel,
        params.escalated,
        params.cursor,
        limit,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, "inbox query failed");
            return ApiError::internal_error("Failed to load conversations")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit inbox transaction");
        return ApiError::internal_error("Failed to load conversations")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let data: Vec<model::Conversation> =
        rows.into_iter().map(row_to_conversation).collect();

    let next_cursor = has_more.then(|| {
        let last = data.last().expect("page with more rows has a last item");
        queries::encode_cursor(last.last_activity_at, last.id)
    });

    Json(PaginatedResponse {
        data,
        pagination: Pagination {
            next_cursor,
            has_more,
        },
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Timeline query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TimelineQueryParams {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl Default for TimelineQueryParams {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers (T028, T040, T050, T059)
// ---------------------------------------------------------------------------

/// `GET /tenant/conversations/{id}` — conversation detail.
///
/// Returns the full conversation detail including participants. Returns 404
/// for missing / cross-tenant / soft-deleted conversations.
pub async fn get_conversation(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
) -> Response {
    let id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return ApiError::validation_failed("Invalid conversation id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin detail transaction");
            return ApiError::internal_error("Failed to load conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let detail = match queries::detail_query_in_tx(&mut tx, ctx.tenant_id, id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, conversation_id = %id, "detail query failed");
            return ApiError::internal_error("Failed to load conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit detail transaction");
        return ApiError::internal_error("Failed to load conversation")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!({ "data": detail })).into_response()
}

/// `GET /tenant/conversations/{id}/messages` — message timeline.
///
/// Returns a keyset-paginated list of messages for the conversation, ordered
/// newest-first. The cursor is an opaque hex string encoding `(created_at, seq)`.
pub async fn get_timeline(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    Query(params): Query<TimelineQueryParams>,
) -> Response {
    let conversation_id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return ApiError::validation_failed("Invalid conversation id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let limit = params.limit.clamp(1, 100) as i64;

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin timeline transaction");
            return ApiError::internal_error("Failed to load messages")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match queries::conversation_row_in_tx(&mut tx, ctx.tenant_id, conversation_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to check conversation existence");
            return ApiError::internal_error("Failed to load messages")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let (messages, has_more, next_cursor) = match queries::timeline_query_in_tx(
        &mut tx,
        ctx.tenant_id,
        conversation_id,
        params.cursor,
        limit,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, conversation_id = %conversation_id, "timeline query failed");
            return ApiError::internal_error("Failed to load messages")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit timeline transaction");
        return ApiError::internal_error("Failed to load messages")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!({
        "data": messages,
        "pagination": {
            "next_cursor": next_cursor,
            "has_more": has_more,
        }
    }))
    .into_response()
}

/// `POST /tenant/conversations/{id}/messages` — add a message.
///
/// Validates kind and body (trimmed, 1-10000 chars). Sets
/// `logged_by_membership_id` for kind=customer and `sender_membership_id`
/// for reply/note. Returns the inserted message and updated conversation
/// status.
pub async fn add_message(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<Principal>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    ApiJson(payload): ApiJson<model::AddMessagePayload>,
) -> Response {
    let conversation_id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return ApiError::validation_failed("Invalid conversation id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let body = payload.body.trim().to_string();
    if body.is_empty() || body.len() > 10000 {
        return ApiError::unprocessable_entity("Message body must be between 1 and 10000 characters")
            .with_details(vec![json!({
                "field": "body",
                "code": "invalid_length",
                "message": "Body must be 1-10000 characters after trimming"
            })])
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let kind_str = serde_json::to_string(&payload.kind)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();

    let valid_kinds = ["customer", "reply", "note"];
    if !valid_kinds.contains(&kind_str.as_str()) {
        return ApiError::unprocessable_entity("Invalid message kind")
            .with_details(vec![json!({
                "field": "kind",
                "code": "invalid_value",
                "message": "Kind must be one of: customer, reply, note"
            })])
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // Resolve the acting member's membership id
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
            return ApiError::not_found("User not found in tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (sender_membership_id, logged_by_membership_id) = match payload.kind {
        model::MessageKind::Customer => (None, Some(membership_id)),
        _ => (Some(membership_id), None),
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin add-message transaction");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match queries::conversation_row_in_tx(&mut tx, ctx.tenant_id, conversation_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to check conversation existence");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let (message, status_ref) = match queries::add_message_in_tx(
        &mut tx,
        ctx.tenant_id,
        conversation_id,
        &kind_str,
        &body,
        sender_membership_id,
        logged_by_membership_id,
        principal.user_id,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, conversation_id = %conversation_id, "add_message failed");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit add-message transaction");
        return ApiError::internal_error("Failed to add message")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!({
        "data": {
            "message": message,
            "conversation": status_ref,
        }
    }))
    .into_response()
}

/// `PATCH /tenant/conversations/{id}` — update status and/or assignment.
///
/// Requires at least one of `status` or `assigned_membership_id` (else 422).
/// Validates assignment target against active memberships. Returns the
/// updated `ConversationDetail`.
pub async fn patch_conversation(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<Principal>,
    path: Result<Path<Uuid>, axum::extract::rejection::PathRejection>,
    ApiJson(payload): ApiJson<model::PatchConversationPayload>,
) -> Response {
    let id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return ApiError::validation_failed("Invalid conversation id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if payload.status.is_none() && payload.assigned_membership_id.is_none() {
        return ApiError::unprocessable_entity("At least one of status or assigned_membership_id is required")
            .with_details(vec![json!({
                "field": "<body>",
                "code": "missing_fields",
                "message": "Specify at least one of: status, assigned_membership_id"
            })])
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let status_str = payload.status.as_ref().map(|s| {
        serde_json::to_string(s)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string()
    });

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin patch transaction");
            return ApiError::internal_error("Failed to update conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match queries::conversation_row_in_tx(&mut tx, ctx.tenant_id, id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to check conversation existence");
            return ApiError::internal_error("Failed to update conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let detail = match queries::patch_conversation_in_tx(
        &mut tx,
        ctx.tenant_id,
        id,
        status_str.as_deref(),
        payload.assigned_membership_id,
        principal.user_id,
    )
    .await
    {
        Ok(detail) => detail,
        Err(error) => {
            tracing::error!(%error, conversation_id = %id, "patch conversation failed");
            let err_msg = error.to_string();
            if err_msg.contains("is not active in tenant") {
                return ApiError::unprocessable_entity("Invalid assignment target")
                    .with_details(vec![json!({
                        "field": "assigned_membership_id",
                        "code": "invalid_value",
                        "message": "The specified membership is not active in this tenant"
                    })])
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            return ApiError::internal_error("Failed to update conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit patch transaction");
        return ApiError::internal_error("Failed to update conversation")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!({ "data": detail })).into_response()
}

/// `POST /tenant/conversations` — create a new conversation.
///
/// Creates a conversation with the first message (agent reply). Validates
/// required fields with field-level 422s. Returns 201 + `ConversationDetail`.
pub async fn create_conversation(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<model::CreateConversationPayload>,
) -> Response {
    let mut details = Vec::new();

    if payload.customer_id.is_nil() {
        details.push(json!({
            "field": "customer_id",
            "code": "required",
            "message": "Customer id is required"
        }));
    }

    let valid_channels = ["email", "phone", "web_chat", "whatsapp", "telegram"];
    if !valid_channels.contains(&payload.channel.as_str()) {
        details.push(json!({
            "field": "channel",
            "code": "invalid_value",
            "message": format!("Channel must be one of: {}", valid_channels.join(", "))
        }));
    }

    let body = payload.message.body.trim().to_string();
    if body.is_empty() || body.len() > 10000 {
        details.push(json!({
            "field": "message.body",
            "code": "invalid_length",
            "message": "Message body must be 1-10000 characters after trimming"
        }));
    }

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // Resolve the acting member's membership id
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
            return ApiError::not_found("User not found in tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin create transaction");
            return ApiError::internal_error("Failed to create conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let detail = match queries::create_conversation_in_tx(
        &mut tx,
        ctx.tenant_id,
        payload.customer_id,
        &payload.channel,
        &body,
        principal.user_id,
        membership_id,
    )
    .await
    {
        Ok(detail) => detail,
        Err(error) => {
            tracing::error!(%error, customer_id = %payload.customer_id, "create conversation failed");
            let err_msg = error.to_string();
            if err_msg.contains("not found in tenant") {
                return ApiError::not_found("Customer not found")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            return ApiError::internal_error("Failed to create conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit create transaction");
        return ApiError::internal_error("Failed to create conversation")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    (StatusCode::CREATED, Json(json!({ "data": detail }))).into_response()
}
