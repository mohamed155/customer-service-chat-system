use std::sync::Arc;

use crate::application::model;
use crate::application::ports::{ConversationStore, EventOutbox};
use crate::application::{
    service, AiAgentStatus, ConversationActor, ConversationError, ConversationEvent,
};
use crate::domain::whatsapp::{fits_in_a_message, reply_window_is_open};
use crate::domain::{
    attribute_staff_message, awaiting_ai_decision, clamp_page_size, is_known_channel,
    validate_body, BodyError, FilterError, InboxFilter, MessageKind, CHANNELS,
};
use crate::infrastructure::{messages, PgEnrichment, PgUnit};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use chrono::Utc;
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
// Handlers
// ---------------------------------------------------------------------------

fn not_found(ctx: &tenancy::TenantContext, message: &'static str) -> Response {
    ApiError::not_found(message)
        .with_request_id(&ctx.request_id)
        .into_response()
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
    let filter = match InboxFilter::parse(
        params.status.as_deref(),
        params.assignee.as_deref(),
        params.channel.as_deref(),
        params.escalated,
    ) {
        Ok(filter) => filter,
        Err(FilterError::Status(status)) => {
            return ApiError::unprocessable_entity("Invalid status filter")
                .with_details(vec![json!({
                    "field": "status",
                    "code": "invalid_value",
                    "message": format!("Unknown status '{status}'. Valid values: open, pending, resolved, closed, all"),
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(FilterError::Assignee(assignee)) => {
            return ApiError::unprocessable_entity("Invalid assignee filter")
                .with_details(vec![json!({
                    "field": "assignee",
                    "code": "invalid_value",
                    "message": format!("Unknown assignee '{assignee}'. Valid values: me, unassigned, or a membership UUID"),
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(FilterError::Channel(channel)) => {
            return ApiError::unprocessable_entity("Invalid channel filter")
                .with_details(vec![json!({
                    "field": "channel",
                    "code": "invalid_value",
                    "message": format!("Unknown channel '{channel}'. Valid values: email, phone, web_chat, whatsapp, telegram"),
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let limit = clamp_page_size(params.limit);

    // Look up the acting member's membership_id for `assignee=me`.
    let mut uow = match ctx.unit_of_work(&pool).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let acting_membership_id = match tenancy::directory::membership_id_of_user(
        uow.conn(),
        ctx.tenant_id,
        principal.user_id,
    )
    .await
    {
        Ok(Some(mid)) => mid,
        // Platform staff browsing a tenant (no membership) may still list
        // conversations; `assignee=me` then matches nothing.
        Ok(None) => Uuid::nil(),
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to look up membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(r) = ctx.commit(uow).await {
        return r;
    }

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin inbox transaction");
            return ApiError::internal_error("Failed to load conversations")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let page = {
        let mut unit = PgUnit::new(&mut tx);
        service::inbox(
            &mut unit,
            ctx.tenant_id,
            &filter.for_member(acting_membership_id),
            params.cursor.as_deref(),
            limit,
        )
        .await
    };
    let mut page = match page {
        Ok(page) => page,
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

    let ids: Vec<Uuid> = page.conversations.iter().map(|c| c.id).collect();
    let system_acks: Vec<(Uuid, bool)> =
        messages::has_system_message_batch(&pool, ctx.tenant_id, &ids)
            .await
            .unwrap_or_default();
    let ack_map: std::collections::HashMap<Uuid, bool> = system_acks.into_iter().collect();

    for conversation in &mut page.conversations {
        let has_system_ack = ack_map.get(&conversation.id).copied().unwrap_or(false);
        conversation.awaiting_ai_decision = awaiting_ai_decision(
            agent_configured,
            has_system_ack,
            platform_ai_available,
            conversation.ai_handling.as_deref(),
        );
    }

    Json(PaginatedResponse {
        data: page.conversations,
        pagination: Pagination {
            next_cursor: page.next_cursor,
            has_more: page.has_more,
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

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin detail transaction");
            return ApiError::internal_error("Failed to load conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let detail = {
        let mut unit = PgUnit::new(&mut tx);
        service::detail(&mut unit, ctx.tenant_id, id).await
    };
    let detail = match detail {
        Ok(Some(detail)) => detail,
        Ok(None) => return not_found(&ctx, "Conversation not found"),
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

    let limit = clamp_page_size(params.limit);

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin timeline transaction");
            return ApiError::internal_error("Failed to load messages")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let page = {
        let mut unit = PgUnit::new(&mut tx);
        match unit.find_live(ctx.tenant_id, conversation_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return not_found(&ctx, "Conversation not found"),
            Err(error) => {
                tracing::error!(%error, "failed to check conversation existence");
                return ApiError::internal_error("Failed to load messages")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
        service::timeline(
            &mut unit,
            &PgEnrichment::new(&pool),
            ctx.tenant_id,
            conversation_id,
            params.cursor.as_deref(),
            limit,
        )
        .await
    };
    let page = match page {
        Ok(page) => page,
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
        "data": page.messages,
        "pagination": {
            "next_cursor": page.next_cursor,
            "has_more": page.has_more,
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

    let body = match validate_body(&payload.body) {
        Ok(body) => body,
        Err(BodyError::Empty | BodyError::TooLong) => {
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
    };

    let mut uow = match ctx.unit_of_work(&pool).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let membership_id = match tenancy::directory::membership_id_of_user(
        uow.conn(),
        ctx.tenant_id,
        principal.user_id,
    )
    .await
    {
        Ok(Some(mid)) => mid,
        Ok(None) => return not_found(&ctx, "User not found in tenant"),
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(r) = ctx.commit(uow).await {
        return r;
    }

    let attribution = attribute_staff_message(payload.kind, membership_id);

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin add-message transaction");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let mut unit = PgUnit::new(&mut tx);

    let conv_channel = match unit.find_live(ctx.tenant_id, conversation_id).await {
        Ok(Some(row)) => row.channel,
        Ok(None) => return not_found(&ctx, "Conversation not found"),
        Err(error) => {
            tracing::error!(%error, "failed to check conversation existence");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if payload.kind == MessageKind::Reply && conv_channel == "whatsapp" {
        if integrations::queries::active_connection_for_slug(&pool, ctx.tenant_id, "whatsapp")
            .await
            .unwrap_or(None)
            .is_none()
        {
            return ApiError::new_with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "whatsapp_channel_disconnected",
                "The WhatsApp channel is disconnected \u{2014} reconnect it to reply.",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }

        let last_customer_message_at =
            messages::last_customer_message_at(&pool, ctx.tenant_id, conversation_id)
                .await
                .unwrap_or(None);
        if !reply_window_is_open(last_customer_message_at, Utc::now()) {
            return ApiError::new_with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "whatsapp_window_expired",
                "The WhatsApp messaging window has expired for this conversation.",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }

        if !fits_in_a_message(&body) {
            return ApiError::new_with_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "whatsapp_body_too_long",
                "Message is too long for WhatsApp (4096 character max).",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
    }

    let added = match service::add_message(
        &mut unit,
        ctx.tenant_id,
        conversation_id,
        payload.kind,
        &body,
        attribution.sender_membership_id,
        attribution.logged_by_membership_id,
        ConversationActor::Staff {
            user_id: principal.user_id,
            membership_id,
        },
    )
    .await
    {
        Ok(added) => added,
        Err(error) => {
            tracing::error!(%error, conversation_id = %conversation_id, "add_message failed");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if payload.kind == MessageKind::Customer {
        if let Err(error) = unit
            .publish(&ConversationEvent::CustomerMessage {
                tenant_id: ctx.tenant_id,
                conversation_id,
                message_id: added.message.id,
                channel: &conv_channel,
            })
            .await
        {
            tracing::error!(%error, conversation_id = %conversation_id, "emit_customer_message failed");
            return ApiError::internal_error("Failed to add message")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if payload.kind == MessageKind::Reply && conv_channel == "whatsapp" {
        if let Err(error) = unit
            .publish(&ConversationEvent::WhatsappOutbound {
                tenant_id: ctx.tenant_id,
                conversation_id,
                message_id: added.message.id,
            })
            .await
        {
            tracing::error!(%error, conversation_id = %conversation_id, "emit_whatsapp_outbound failed");
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
            "message": added.message,
            "conversation": added.conversation,
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

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin patch transaction");
            return ApiError::internal_error("Failed to update conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let mut unit = PgUnit::new(&mut tx);

    match unit.find_live(ctx.tenant_id, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(&ctx, "Conversation not found"),
        Err(error) => {
            tracing::error!(%error, "failed to check conversation existence");
            return ApiError::internal_error("Failed to update conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let detail = match service::patch(
        &mut unit,
        ctx.tenant_id,
        id,
        payload.status,
        payload.assigned_membership_id,
        principal.user_id,
    )
    .await
    {
        Ok(detail) => detail,
        Err(error) => {
            tracing::error!(%error, conversation_id = %id, "patch conversation failed");
            if matches!(error, ConversationError::MembershipNotActive { .. }) {
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

    if !is_known_channel(&payload.channel) {
        details.push(json!({
            "field": "channel",
            "code": "invalid_value",
            "message": format!("Channel must be one of: {}", CHANNELS.join(", "))
        }));
    }

    let body = validate_body(&payload.message.body);
    if body.is_err() {
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
    let body = body.unwrap_or_default();

    let mut uow = match ctx.unit_of_work(&pool).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let membership_id = match tenancy::directory::membership_id_of_user(
        uow.conn(),
        ctx.tenant_id,
        principal.user_id,
    )
    .await
    {
        Ok(Some(mid)) => mid,
        Ok(None) => return not_found(&ctx, "User not found in tenant"),
        Err(error) => {
            tracing::error!(%error, "failed to resolve membership_id");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(r) = ctx.commit(uow).await {
        return r;
    }

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin create transaction");
            return ApiError::internal_error("Failed to create conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let detail = {
        let mut unit = PgUnit::new(&mut tx);
        service::create(
            &mut unit,
            ctx.tenant_id,
            payload.customer_id,
            &payload.channel,
            &body,
            ConversationActor::Staff {
                user_id: principal.user_id,
                membership_id,
            },
            None,
        )
        .await
    };
    let detail = match detail {
        Ok(detail) => detail,
        Err(error) => {
            tracing::error!(%error, customer_id = %payload.customer_id, "create conversation failed");
            if matches!(error, ConversationError::CustomerNotFound { .. }) {
                return not_found(&ctx, "Customer not found");
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
