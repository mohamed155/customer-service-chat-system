use std::sync::Arc;

use super::{model, queries};
use crate::outbox::emit_customer_message_in_tx;
use crate::AiAgentStatus;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Query parameter types
// ---------------------------------------------------------------------------

/// Query string for `GET /tenant/conversations`.
///
/// `status` accepts one of the `ConversationStatus` variants, the special
/// sentinel `all`, or is omitted for "no status filter".  `assignee`
/// accepts `me`, `unassigned`, or a membership UUID.  `channel` is one of
/// the channel vocabulary (`email`, `phone`, `web_chat`, `whatsapp`,
/// `telegram`).  `escalated` filters to conversations with an active
/// escalation when present.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct InboxQueryParams {
    #[param(value_type = Option<String>, example = "open")]
    pub status: Option<String>,
    #[param(value_type = Option<String>, example = "me")]
    pub assignee: Option<String>,
    #[param(value_type = Option<String>, example = "email")]
    pub channel: Option<String>,
    #[param(value_type = Option<bool>)]
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

/// Pagination metadata returned alongside a keyset-paginated list.  Mirrors
/// the inline `pagination` field of the list responses (`data` envelope
/// shape — research Decision 4).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pagination {
    next_cursor: Option<String>,
    has_more: bool,
}

/// `{data, pagination}` envelope for keyset-paginated lists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T: ToSchema> {
    data: Vec<T>,
    pagination: Pagination,
}

// ---------------------------------------------------------------------------
// OpenAPI doc-only response envelopes
// ---------------------------------------------------------------------------
//
// These wrappers mirror the inline `json!({"data": ...})` shapes the handlers
// emit, with a concrete `ToSchema` element type so `#[utoipa::path]` can
// attach a concrete `body = ...` schema to each operation (FR-005, FR-008).
// The handlers continue to build their envelopes with the private
// `PaginatedResponse<T>` / inline `json!` macros — the wrapper types exist
// purely for the OpenAPI surface.

/// `GET /tenant/conversations` response envelope (`{data, pagination}`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConversationsListResponse {
    pub data: Vec<model::Conversation>,
    pub pagination: Pagination,
}

/// `GET /tenant/conversations/{id}/messages` response envelope
/// (`{data, pagination}`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MessagesListResponse {
    pub data: Vec<model::Message>,
    pub pagination: Pagination,
}

/// `POST /tenant/conversations`, `PATCH /tenant/conversations/{id}` and the
/// composite `GET /tenant/conversations/{id}` all return the same
/// `{data: ConversationDetail}` shape — the escalation context merged by
/// the server-side handler is documented separately on the composite
/// endpoint (T019).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConversationDetailResponse {
    pub data: model::ConversationDetail,
}

/// `POST /tenant/conversations/{id}/messages` response envelope
/// (`{data: AddMessageResponse}`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddMessageResponseEnvelope {
    pub data: model::AddMessageResponse,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_from_str(s: &str) -> model::ConversationStatus {
    serde_json::from_value(Value::String(s.to_owned())).unwrap_or(model::ConversationStatus::Open)
}

fn kind_from_str(s: &str) -> model::MessageKind {
    serde_json::from_value(Value::String(s.to_owned())).unwrap_or(model::MessageKind::Reply)
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

    let widget_instance = row.widget_instance_id.map(|id| model::WidgetInstanceRef {
        id,
        name: row.widget_instance_name.unwrap_or_default(),
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
        ai_handling: row.ai_handling,
        awaiting_ai_decision: false,
        widget_instance,
        rating: row.feedback_rating,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn compute_awaiting_ai_decision(
    agent_configured: bool,
    has_system_ack: bool,
    platform_ai_available: bool,
    row: &queries::InboxRow,
) -> bool {
    if agent_configured {
        return false;
    }
    if !has_system_ack {
        return false;
    }
    match row.ai_handling.as_deref() {
        None => true,
        Some("platform_ai") => !platform_ai_available,
        Some("human") => false,
        _ => true,
    }
}

/// `GET /tenant/conversations` — inbox list.
///
/// Returns a keyset-paginated, filterable list of active conversations
/// for the current tenant, ordered by `last_activity_at DESC, id DESC`.
#[utoipa::path(
    get,
    path = "/tenant/conversations",
    tag = "conversations",
    operation_id = "list_conversations",
    summary = "List tenant conversations",
    description = "List active (non soft-deleted) conversations belonging to the current tenant \
                  with cursor-based pagination and optional `status` / `assignee` / `channel` / \
                  `escalated` filters. Ordered by `last_activity_at DESC, id DESC`. The response \
                  body is the doc-only `{data, pagination}` envelope (`ConversationsListResponse`); \
                  the `pagination.next_cursor` is opaque and should be passed back verbatim on the \
                  next request. Requires permission: conversations.view",
    params(InboxQueryParams),
    responses(
        (status = 200, description = "Page of conversations (data + pagination).", body = ConversationsListResponse),
        (status = 400, description = "Validation failed (invalid cursor).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (invalid `status`, `assignee`, or `channel` filter).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_conversations(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(ai_status): Extension<Arc<dyn AiAgentStatus>>,
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
        let valid_channels = [
            "email", "phone", "web_chat", "whatsapp", "telegram", "widget",
        ];
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

    let agent_configured = ai_status.agent_configured(ctx.tenant_id).await;
    let platform_ai_available = ai_status.platform_ai_available(ctx.tenant_id).await;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let system_acks: Vec<(Uuid, bool)> =
        queries::has_system_message_batch(&pool, ctx.tenant_id, &ids)
            .await
            .unwrap_or_default();
    let ack_map: std::collections::HashMap<Uuid, bool> = system_acks.into_iter().collect();

    let mut data: Vec<model::Conversation> = Vec::with_capacity(rows.len());
    for row in rows {
        let has_system_ack = ack_map.get(&row.id).copied().unwrap_or(false);
        let awaiting = compute_awaiting_ai_decision(
            agent_configured,
            has_system_ack,
            platform_ai_available,
            &row,
        )
        .await;
        let mut conv = row_to_conversation(row);
        conv.awaiting_ai_decision = awaiting;
        data.push(conv);
    }

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

/// Query string for `GET /tenant/conversations/{id}/messages`.  Mirrors
/// `PageParams` semantics (cursor + limit, clamped to 1..=100) but with a
/// default `limit` of 50 to match the timeline UX.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
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
///
/// **This route is no longer wired through the router** — the active
/// `GET /tenant/conversations/{id}` endpoint is the composite handler
/// `crate::handlers::get_conversation_with_escalation` (T019), which merges
/// the detail with optional escalation context.  The OpenAPI annotation
/// lives on that composite handler; this function is preserved for
/// completeness and continues to be callable in isolation.
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
#[utoipa::path(
    get,
    path = "/tenant/conversations/{id}/messages",
    tag = "conversations",
    operation_id = "get_timeline",
    summary = "Get a conversation's message timeline",
    description = "Return a keyset-paginated list of messages for a conversation, ordered \
                  newest-first. The `pagination.next_cursor` is opaque and should be passed back \
                  verbatim on the next request. Returns 404 when the conversation does not exist \
                  (or belongs to another tenant). Requires permission: conversations.view",
    params(
        ("id" = Uuid, Path, description = "Conversation identifier"),
        TimelineQueryParams,
    ),
    responses(
        (status = 200, description = "Page of messages (data + pagination).", body = MessagesListResponse),
        (status = 400, description = "Validation failed (invalid cursor).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
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
        &pool,
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
#[utoipa::path(
    post,
    path = "/tenant/conversations/{id}/messages",
    tag = "conversations",
    operation_id = "add_message",
    summary = "Add a message to a conversation",
    description = "Insert a new message into a conversation. `kind` is the sender role: \
                  `customer` (logged on behalf of the customer, attributed to the staff \
                  principal via `logged_by`), `reply` (agent reply), or `note` (internal \
                  note, visible only to staff). `body` is trimmed and must be 1..=10000 chars. \
                  Returns 201 with the inserted `Message` and the refreshed `ConversationStatusRef`. \
                  Requires permission: conversations.manage",
    params(("id" = Uuid, Path, description = "Conversation identifier")),
    request_body = model::AddMessagePayload,
    responses(
        (status = 201, description = "Message added.", body = AddMessageResponseEnvelope),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. invalid `kind` or `body` length).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
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
        return ApiError::unprocessable_entity(
            "Message body must be between 1 and 10000 characters",
        )
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

    let valid_kinds = ["customer", "reply", "note", "ai", "system"];
    if !valid_kinds.contains(&kind_str.as_str()) {
        return ApiError::unprocessable_entity("Invalid message kind")
            .with_details(vec![json!({
                "field": "kind",
                "code": "invalid_value",
                "message": "Kind must be one of: customer, reply, note, ai, system"
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

    let (sender_membership_id, logged_by_membership_id) = match &payload.kind {
        model::MessageKind::Customer => (None, Some(membership_id)),
        model::MessageKind::Ai | model::MessageKind::System => (None, None),
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

    let conv_channel =
        match queries::conversation_row_in_tx(&mut tx, ctx.tenant_id, conversation_id).await {
            Ok(Some(row)) => row.channel,
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
        };

    let (message, status_ref) = match queries::add_message_in_tx(
        &mut tx,
        ctx.tenant_id,
        conversation_id,
        &kind_str,
        &body,
        sender_membership_id,
        logged_by_membership_id,
        queries::ConversationActor::Staff {
            user_id: principal.user_id,
            membership_id,
        },
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

    if payload.kind == model::MessageKind::Customer {
        if let Err(error) = emit_customer_message_in_tx(
            &mut tx,
            ctx.tenant_id,
            conversation_id,
            message.id,
            &conv_channel,
        )
        .await
        {
            tracing::error!(%error, conversation_id = %conversation_id, "emit_customer_message failed");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

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
#[utoipa::path(
    patch,
    path = "/tenant/conversations/{id}",
    tag = "conversations",
    operation_id = "patch_conversation",
    summary = "Update a conversation (status / assignment)",
    description = "Partial update of a conversation. At least one of `status` or \
                  `assigned_membership_id` MUST be supplied (422 otherwise). `status` accepts \
                  one of the `ConversationStatus` variants. `assigned_membership_id` is \
                  tri-state on the wire: omit the field to leave the assignment unchanged, \
                  send `null` to unassign, or supply a membership UUID to assign. Returns 200 \
                  with the refreshed `ConversationDetail`. Requires permission: conversations.manage",
    params(("id" = Uuid, Path, description = "Conversation identifier")),
    request_body = model::PatchConversationPayload,
    responses(
        (status = 200, description = "Conversation updated.", body = ConversationDetailResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (no fields supplied, or the target membership is not active in this tenant).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
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
        return ApiError::unprocessable_entity(
            "At least one of status or assigned_membership_id is required",
        )
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
#[utoipa::path(
    post,
    path = "/tenant/conversations",
    tag = "conversations",
    operation_id = "create_conversation",
    summary = "Create a new conversation",
    description = "Create a new conversation with the first message (always a `reply`). \
                  `channel` is one of `email`, `phone`, `web_chat`, `whatsapp`, `telegram`. \
                  `message.body` is trimmed and must be 1..=10000 chars. Returns 201 with the \
                  `ConversationDetail` for the new conversation. Requires permission: conversations.manage",
    request_body = model::CreateConversationPayload,
    responses(
        (status = 201, description = "Conversation created.", body = ConversationDetailResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Customer not found in this tenant.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field, e.g. invalid channel or message body length).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
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

    let valid_channels = [
        "email", "phone", "web_chat", "whatsapp", "telegram", "widget",
    ];
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
        queries::ConversationActor::Staff {
            user_id: principal.user_id,
            membership_id,
        },
        None,
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
