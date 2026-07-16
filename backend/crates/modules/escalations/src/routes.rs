use std::sync::Arc;

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
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use tenancy::TenantContext;

use crate::audit;
use crate::model::{
    Availability, AvailabilityState, CreateSkillPayload, CustomerRef, EscalatePayload, Escalation,
    EscalationAssignedEvent, EscalationQueuedEvent, EscalationRemovedEvent, EscalationStatus,
    QueueEntry, QueueEntryConversationRef, RenameSkillPayload, RequiredSkillRef,
    SetAvailabilityPayload, SetMemberSkillsPayload, Skill,
};
use crate::presence;
use crate::queries;
use crate::routing;

// ---------------------------------------------------------------------------
// OpenAPI doc-only response envelopes
// ---------------------------------------------------------------------------
//
// These wrappers mirror the inline `json!({"data": ...})` shapes the handlers
// emit, with concrete `ToSchema` element types so `#[utoipa::path]` can attach
// a concrete `body = ...` schema to each operation (FR-005, FR-008). The
// handlers continue to build their envelopes inline with `json!` macros —
// the wrapper types exist purely for the OpenAPI surface.

/// Pagination metadata returned alongside a keyset-paginated list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pagination {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// `GET /tenant/escalations/queue` response envelope (`{data, pagination}`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueueListResponse {
    pub data: Vec<QueueEntry>,
    pub pagination: Pagination,
}

/// `GET /tenant/skills` response envelope (`{data}`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SkillListResponse {
    pub data: Vec<Skill>,
}

/// `PUT /tenant/members/{membershipId}/skills` response envelope
/// (`{data}` — the updated skill list).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MemberSkillsResponse {
    pub data: Vec<Skill>,
}

// ---------------------------------------------------------------------------
// Escalate
// ---------------------------------------------------------------------------

/// `POST /tenant/conversations/{id}/escalate` — escalate a conversation.
#[utoipa::path(
    post,
    path = "/tenant/conversations/{id}/escalate",
    tag = "escalations",
    operation_id = "escalate_conversation",
    summary = "Escalate a conversation to a human agent",
    description = "Escalate an active (open/pending) conversation so it is either assigned to \
                  a matching available agent or queued. `reason` is trimmed and must be \
                  1..=2000 characters. `requiredSkillIds` (optional) restricts routing to agents \
                  that hold every listed skill — if no available agent matches, the escalation is \
                  queued. Returns 201 with the new `Escalation` (assigned or queued). 409 is \
                  returned if the conversation already has an active escalation. \
                  Requires permission: conversations.manage",
    params(("id" = Uuid, Path, description = "Conversation identifier")),
    request_body = EscalatePayload,
    responses(
        (status = 201, description = "Escalation created (assigned or queued).", body = Escalation),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 409, description = "Conversation already has an active escalation.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (reason length, or unknown required skill id).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn escalate(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
    Path(conversation_id): Path<Uuid>,
    ApiJson(payload): ApiJson<EscalatePayload>,
) -> Response {
    let reason = payload.reason.trim().to_owned();
    if reason.is_empty() || reason.len() > 2000 {
        return ApiError::unprocessable_entity("Reason must be 1-2000 characters")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let required_skill_ids = payload.required_skill_ids.unwrap_or_default();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "escalate: begin tx failed");
            return ApiError::internal_error("Failed to escalate conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let conv = match sqlx::query_as::<_, (String, String)>(
        "SELECT status, assigned_membership_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "escalate: fetch conversation failed");
            return ApiError::internal_error("Failed to escalate conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if conv.0 == "resolved" || conv.0 == "closed" {
        return ApiError::unprocessable_entity("Cannot escalate a resolved or closed conversation")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if !required_skill_ids.is_empty() {
        let valid =
            queries::skill_ids_exist_in_tenant_in_tx(&mut tx, ctx.tenant_id, &required_skill_ids)
                .await;
        match valid {
            Ok(true) => {}
            Ok(false) => {
                return ApiError::unprocessable_entity("Unknown required skill id")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            Err(e) => {
                tracing::error!(%e, "escalate: skill validation failed");
                return ApiError::internal_error("Failed to escalate conversation")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    }

    let required_skill_names: Vec<String> = if required_skill_ids.is_empty() {
        Vec::new()
    } else {
        match crate::model::sql::skill_names_for_ids_in_tx(
            &mut tx,
            ctx.tenant_id,
            &required_skill_ids,
        )
        .await
        {
            Ok(names) => names,
            Err(e) => {
                tracing::error!(%e, "escalate: resolve skill names failed");
                return ApiError::internal_error("Failed to escalate conversation")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    };

    let present_ids = runtime.present_membership_ids_async(ctx.tenant_id).await;

    match routing::route_new_escalation_in_tx(
        &mut tx,
        &pool,
        ctx.tenant_id,
        conversation_id,
        &reason,
        &required_skill_ids,
        &required_skill_names,
        &present_ids,
        principal.user_id,
    )
    .await
    {
        Ok(outcome) => {
            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "escalate: commit failed");
                return ApiError::internal_error("Failed to escalate conversation")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            match outcome {
                routing::RouteOutcome::Assigned {
                    escalation,
                    assigned_membership_id: _,
                    matched_skill_names,
                } => {
                    let event = EscalationAssignedEvent {
                        v: 1,
                        escalation_id: escalation.id,
                        conversation_id,
                        reason: reason.clone(),
                        routing_reason: crate::model::RoutingReason::SkillMatch,
                        matched_skills: matched_skill_names,
                        assigned_at: Utc::now(),
                    };
                    runtime.broadcast(ctx.tenant_id, presence::Event::EscalationAssigned(event));
                    (StatusCode::CREATED, Json(json!(escalation))).into_response()
                }
                routing::RouteOutcome::Queued { escalation } => {
                    let event = EscalationQueuedEvent {
                        v: 1,
                        escalation_id: escalation.id,
                        conversation_id,
                        escalated_at: Utc::now(),
                        required_skills: vec![],
                    };
                    runtime.broadcast(ctx.tenant_id, presence::Event::EscalationQueued(event));
                    (StatusCode::CREATED, Json(json!(escalation))).into_response()
                }
            }
        }
        Err(routing::RouteError::Duplicate) => {
            let existing = queries::active_escalation_for_conversation_in_tx(
                &mut tx,
                ctx.tenant_id,
                conversation_id,
            )
            .await;
            tx.rollback().await.ok();
            match existing {
                Ok(Some(row)) => {
                    let err = ApiError::conflict("Conversation already has an active escalation")
                        .with_details(vec![json!({
                            "escalationId": row.id,
                        })]);
                    err.with_request_id(&ctx.request_id).into_response()
                }
                _ => ApiError::conflict("Conversation already has an active escalation")
                    .with_request_id(&ctx.request_id)
                    .into_response(),
            }
        }
        Err(e) => {
            tx.rollback().await.ok();
            tracing::error!(%e, "escalate: routing failed");
            ApiError::internal_error("Failed to escalate conversation")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Queue list
// ---------------------------------------------------------------------------

/// Query string for `GET /tenant/escalations/queue`.  `cursor` is the opaque
/// pagination cursor returned by a prior response; `limit` is clamped to
/// 1..=100 by the handler.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueueQueryParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// `GET /tenant/escalations/queue` — list queued escalations.
#[utoipa::path(
    get,
    path = "/tenant/escalations/queue",
    tag = "escalations",
    operation_id = "list_escalation_queue",
    summary = "List queued escalations",
    description = "Return queued escalations for the current tenant, ordered by `escalatedAt` \
                  ascending (oldest first). The response is the doc-only `{data, pagination}` \
                  envelope (`QueueListResponse`); the `pagination.nextCursor` is opaque and should \
                  be passed back verbatim on the next request. \
                  Requires permission: conversations.view",
    params(QueueQueryParams),
    responses(
        (status = 200, description = "Page of queued escalations (data + pagination).", body = QueueListResponse),
        (status = 400, description = "Validation failed (invalid cursor).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_queue(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Query(params): Query<QueueQueryParams>,
) -> Response {
    let limit = params.limit.map(|l| l.clamp(1, 100) as i64).unwrap_or(25);
    let cursor = params.cursor.as_deref();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "list_queue: begin tx failed");
            return ApiError::internal_error("Failed to load queue")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (cursor_ts, cursor_id) = cursor
        .and_then(queries::decode_queue_cursor)
        .unwrap_or((Utc::now(), Uuid::nil()));

    let rows = sqlx::query_as::<_, queries::QueueEntryRow>(
        "SELECT e.id, e.conversation_id, e.reason, e.required_skill_ids, \
                e.required_skill_names, e.status, e.escalated_at, \
                c.channel AS conv_channel, cu.id AS cust_id, cu.display_name AS cust_name, \
                EXTRACT(EPOCH FROM now() - e.escalated_at)::bigint AS waiting_seconds \
         FROM escalations e \
         JOIN conversations c ON c.id = e.conversation_id AND c.tenant_id = e.tenant_id \
         JOIN customers cu ON cu.id = c.customer_id AND cu.tenant_id = c.tenant_id \
         WHERE e.tenant_id = $1 AND e.status = 'queued' \
           AND (e.escalated_at, e.id) > ($2, $3) \
         ORDER BY e.escalated_at ASC, e.id ASC \
         LIMIT $4",
    )
    .bind(ctx.tenant_id)
    .bind(cursor_ts)
    .bind(cursor_id)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_queue: query failed");
            return ApiError::internal_error("Failed to load queue")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    tx.commit().await.ok();

    let has_more = rows.len() > limit as usize;
    let entries: Vec<QueueEntry> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            let req_skills: Vec<RequiredSkillRef> = r
                .required_skill_ids
                .iter()
                .cloned()
                .zip(
                    r.required_skill_names
                        .iter()
                        .cloned()
                        .chain(std::iter::repeat(String::new())),
                )
                .map(|(id, name)| RequiredSkillRef { id: Some(id), name })
                .collect();

            QueueEntry {
                escalation: Escalation {
                    id: r.id,
                    conversation_id: r.conversation_id,
                    reason: r.reason,
                    required_skills: req_skills,
                    status: EscalationStatus::Queued,
                    routing: None,
                    escalated_at: r.escalated_at,
                    closed_at: None,
                },
                conversation: QueueEntryConversationRef {
                    id: r.conversation_id,
                    channel: r.conv_channel,
                    customer: CustomerRef {
                        id: r.cust_id,
                        name: r.cust_name,
                    },
                },
                waiting_seconds: r.waiting_seconds,
            }
        })
        .collect();

    let next_cursor = entries
        .last()
        .map(|e| queries::encode_queue_cursor(&e.escalation.escalated_at, &e.escalation.id));

    Json(json!({
        "data": entries,
        "pagination": {
            "nextCursor": next_cursor,
            "hasMore": has_more,
        }
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Claim
// ---------------------------------------------------------------------------

/// `POST /tenant/escalations/{id}/claim` — claim a queued escalation.
#[utoipa::path(
    post,
    path = "/tenant/escalations/{id}/claim",
    tag = "escalations",
    operation_id = "claim_escalation",
    summary = "Claim a queued escalation",
    description = "Manually claim a queued escalation as the calling member, assigning it to \
                  their membership. Returns 200 with the claimed `Escalation` (status \
                  `assigned`). 409 is returned if the escalation has already been claimed (the \
                  response includes the membership id of the current holder). \
                  Requires permission: conversations.manage",
    params(("id" = Uuid, Path, description = "Escalation identifier")),
    responses(
        (status = 200, description = "Escalation claimed.", body = Escalation),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Escalation or membership not found.", body = ErrorEnvelope),
        (status = 409, description = "Escalation already claimed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn claim(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
    Path(escalation_id): Path<Uuid>,
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
            return ApiError::not_found("Membership not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "claim: resolve membership failed");
            return ApiError::internal_error("Failed to claim escalation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "claim: begin tx failed");
            return ApiError::internal_error("Failed to claim escalation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match routing::claim_in_tx(
        &mut tx,
        ctx.tenant_id,
        escalation_id,
        membership_id,
        principal.user_id,
    )
    .await
    {
        Ok(escalation) => {
            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "claim: commit failed");
                return ApiError::internal_error("Failed to claim escalation")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            let removed_event = EscalationRemovedEvent {
                v: 1,
                escalation_id,
                cause: "claimed".into(),
            };
            runtime.broadcast(
                ctx.tenant_id,
                presence::Event::EscalationRemoved(removed_event),
            );

            Json(json!(escalation)).into_response()
        }
        Err(routing::ClaimError::NotFound) => {
            tx.rollback().await.ok();
            ApiError::not_found("Escalation not found")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(routing::ClaimError::AlreadyClaimed {
            assigned_membership_id,
        }) => {
            tx.rollback().await.ok();
            ApiError::conflict("Escalation already claimed")
                .with_details(vec![json!({
                    "assignedMembershipId": assigned_membership_id,
                })])
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(routing::ClaimError::Internal(msg)) => {
            tx.rollback().await.ok();
            tracing::error!(%msg, "claim: internal error");
            ApiError::internal_error("Failed to claim escalation")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Availability
// ---------------------------------------------------------------------------

/// `GET /tenant/availability/me` — current agent availability.
#[utoipa::path(
    get,
    path = "/tenant/availability/me",
    tag = "escalations",
    operation_id = "get_my_availability",
    summary = "Get my current availability",
    description = "Return the calling member's current availability state. The default for a \
                  member with no row is `away`. `stateChangedAt` is null when no row exists yet. \
                  Requires permission: conversations.manage",
    responses(
        (status = 200, description = "Current availability.", body = Availability),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Membership not found in this tenant.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_my_availability(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
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
            return ApiError::not_found("Membership not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_availability: resolve membership failed");
            return ApiError::internal_error("Failed to get availability")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "get_availability: begin tx failed");
            return ApiError::internal_error("Failed to get availability")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let avail = queries::get_availability_in_tx(&mut tx, ctx.tenant_id, membership_id)
        .await
        .unwrap_or(None);

    tx.commit().await.ok();

    let response = match avail {
        Some(row) => Availability {
            membership_id,
            state: serde_json::from_value(serde_json::Value::String(row.state))
                .unwrap_or(AvailabilityState::Away),
            state_changed_at: Some(row.state_changed_at),
        },
        None => Availability {
            membership_id,
            state: AvailabilityState::Away,
            state_changed_at: None,
        },
    };

    Json(json!(response)).into_response()
}

/// `PUT /tenant/availability/me` — toggle current agent availability.
#[utoipa::path(
    put,
    path = "/tenant/availability/me",
    tag = "escalations",
    operation_id = "set_my_availability",
    summary = "Set my availability",
    description = "Set the calling member's availability to `available` or `away`. When toggling \
                  to `available` the server attempts to drain one queued escalation for the \
                  member (skill match first, then any queue entry) before responding; any \
                  routed escalation is delivered via the SSE stream rather than the response \
                  body. Returns 200 with the persisted `Availability`. \
                  Requires permission: conversations.manage",
    request_body = SetAvailabilityPayload,
    responses(
        (status = 200, description = "Availability set.", body = Availability),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Membership not found in this tenant.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (invalid state value).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_my_availability(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
    ApiJson(payload): ApiJson<SetAvailabilityPayload>,
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
            return ApiError::not_found("Membership not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "set_availability: resolve membership failed");
            return ApiError::internal_error("Failed to set availability")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let new_state = match payload.state {
        AvailabilityState::Available => "available",
        AvailabilityState::Away => "away",
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "set_availability: begin tx failed");
            return ApiError::internal_error("Failed to set availability")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let prev = queries::get_availability_in_tx(&mut tx, ctx.tenant_id, membership_id)
        .await
        .unwrap_or(None);

    let prev_state = prev.as_ref().map(|r| r.state.as_str());
    let prev_state_str = prev_state.unwrap_or("away");

    let availability =
        match queries::upsert_availability_in_tx(&mut tx, ctx.tenant_id, membership_id, new_state)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(%e, "set_availability: upsert failed");
                return ApiError::internal_error("Failed to set availability")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    audit::record_availability_changed(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        membership_id,
        Some(prev_state_str),
        new_state,
        "toggle",
    )
    .await
    .ok();

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "set_availability: commit failed");
        return ApiError::internal_error("Failed to set availability")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let event = crate::model::AvailabilityChangedEvent {
        v: 1,
        membership_id,
        state: payload.state,
        cause: "toggle".into(),
    };
    runtime.broadcast(ctx.tenant_id, presence::Event::AvailabilityChanged(event));

    if new_state == "available" && prev_state_str != "available" {
        let present_ids = runtime.present_membership_ids_async(ctx.tenant_id).await;
        let mut drain_tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(_) => return Json(json!(availability)).into_response(),
        };
        let drained_esc_id = match routing::drain_one_for_membership_in_tx(
            &mut drain_tx,
            ctx.tenant_id,
            membership_id,
            &present_ids,
            principal.user_id,
        )
        .await
        {
            Ok(eid) => eid,
            Err(e) => {
                tracing::error!(%e, "set_availability: drain failed");
                drain_tx.rollback().await.ok();
                return Json(json!(availability)).into_response();
            }
        };
        drain_tx.commit().await.ok();

        if let Some(escalation_id) = drained_esc_id {
            let assigned_event = crate::model::EscalationAssignedEvent {
                v: 1,
                escalation_id,
                conversation_id: Uuid::nil(),
                reason: String::new(),
                routing_reason: crate::model::RoutingReason::QueueAuto,
                matched_skills: Vec::new(),
                assigned_at: Utc::now(),
            };
            runtime.broadcast(
                ctx.tenant_id,
                presence::Event::EscalationAssigned(assigned_event),
            );
            let removed_event = crate::model::EscalationRemovedEvent {
                v: 1,
                escalation_id,
                cause: "assigned".into(),
            };
            runtime.broadcast(
                ctx.tenant_id,
                presence::Event::EscalationRemoved(removed_event),
            );
        }
    }

    Json(json!(availability)).into_response()
}

// ---------------------------------------------------------------------------
// Skills CRUD
// ---------------------------------------------------------------------------

/// `GET /tenant/skills` — list tenant skills.
#[utoipa::path(
    get,
    path = "/tenant/skills",
    tag = "escalations",
    operation_id = "list_skills",
    summary = "List tenant skills",
    description = "Return every skill in the tenant's catalog, ordered by id. Each skill carries \
                  `agentCount` — the number of members currently assigned that skill. The response \
                  is the doc-only `{data: Skill[]}` envelope (`SkillListResponse`). \
                  Requires permission: members.view",
    responses(
        (status = 200, description = "All skills in the catalog.", body = SkillListResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_skills(State(pool): State<PgPool>, ctx: TenantContext) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "list_skills: begin tx failed");
            return ApiError::internal_error("Failed to list skills")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let skills = match queries::list_skills_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(%e, "list_skills: query failed");
            return ApiError::internal_error("Failed to list skills")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    tx.commit().await.ok();
    Json(json!({ "data": skills })).into_response()
}

/// `POST /tenant/skills` — create a skill.
#[utoipa::path(
    post,
    path = "/tenant/skills",
    tag = "escalations",
    operation_id = "create_skill",
    summary = "Create a skill",
    description = "Create a new skill in the tenant catalog. `name` is trimmed and must be \
                  1..=50 characters; uniqueness is case-insensitive within the tenant. Returns \
                  201 with the new `Skill`. 409 is returned if a skill with the same name \
                  already exists. Requires permission: members.view",
    request_body = CreateSkillPayload,
    responses(
        (status = 201, description = "Skill created.", body = Skill),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "A skill with this name already exists.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (name length).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_skill(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<CreateSkillPayload>,
) -> Response {
    let name = payload.name.trim().to_owned();
    if name.is_empty() || name.len() > 50 {
        return ApiError::unprocessable_entity("Skill name must be 1-50 characters")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "create_skill: begin tx failed");
            return ApiError::internal_error("Failed to create skill")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let skill = match queries::create_skill_in_tx(&mut tx, ctx.tenant_id, &name).await {
        Ok(s) => s,
        Err(e) => {
            if let sqlx::Error::Database(ref dbe) = e {
                if dbe.constraint() == Some("skills_tenant_lower_name_uniq") {
                    tx.rollback().await.ok();
                    return ApiError::conflict("A skill with this name already exists")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }
            tracing::error!(%e, "create_skill: insert failed");
            return ApiError::internal_error("Failed to create skill")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    audit::record_skill_created(&mut tx, principal.user_id, ctx.tenant_id, skill.id, &name)
        .await
        .ok();

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "create_skill: commit failed");
        return ApiError::internal_error("Failed to create skill")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    (StatusCode::CREATED, Json(json!(skill))).into_response()
}

/// `PATCH /tenant/skills/{id}` — rename a skill.
#[utoipa::path(
    patch,
    path = "/tenant/skills/{id}",
    tag = "escalations",
    operation_id = "rename_skill",
    summary = "Rename a skill",
    description = "Rename an existing skill in the tenant catalog. `name` is trimmed and must \
                  be 1..=50 characters; uniqueness is case-insensitive within the tenant. \
                  Returns 200 with the renamed `Skill`. 404 is returned if no such skill exists \
                  in this tenant. 409 is returned if another skill in the tenant already has \
                  the requested name. Requires permission: members.manage",
    params(("id" = Uuid, Path, description = "Skill identifier")),
    request_body = RenameSkillPayload,
    responses(
        (status = 200, description = "Skill renamed.", body = Skill),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Skill not found.", body = ErrorEnvelope),
        (status = 409, description = "A skill with this name already exists.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (name length).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn rename_skill(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(skill_id): Path<Uuid>,
    ApiJson(payload): ApiJson<RenameSkillPayload>,
) -> Response {
    let new_name = payload.name.trim().to_owned();
    if new_name.is_empty() || new_name.len() > 50 {
        return ApiError::unprocessable_entity("Skill name must be 1-50 characters")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "rename_skill: begin tx failed");
            return ApiError::internal_error("Failed to rename skill")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let old_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM skills WHERE tenant_id = $1 AND id = $2")
            .bind(ctx.tenant_id)
            .bind(skill_id)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or(None);

    let old_name = match old_name {
        Some(n) => n,
        None => {
            tx.rollback().await.ok();
            return ApiError::not_found("Skill not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let skill = match queries::rename_skill_in_tx(&mut tx, ctx.tenant_id, skill_id, &new_name).await
    {
        Ok(s) => s,
        Err(e) => {
            if let sqlx::Error::Database(ref dbe) = e {
                if dbe.constraint() == Some("skills_tenant_lower_name_uniq") {
                    tx.rollback().await.ok();
                    return ApiError::conflict("A skill with this name already exists")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }
            tracing::error!(%e, "rename_skill: update failed");
            return ApiError::internal_error("Failed to rename skill")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    audit::record_skill_updated(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        skill_id,
        &old_name,
        &new_name,
    )
    .await
    .ok();

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "rename_skill: commit failed");
        return ApiError::internal_error("Failed to rename skill")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!(skill)).into_response()
}

/// `DELETE /tenant/skills/{id}` — delete a skill.
#[utoipa::path(
    delete,
    path = "/tenant/skills/{id}",
    tag = "escalations",
    operation_id = "delete_skill",
    summary = "Delete a skill",
    description = "Hard-delete a skill from the tenant catalog (per R7, skills are not \
                  soft-deleted; cascading `agent_skills` rows are removed in the same \
                  transaction). Returns 204 on success. 404 if no such skill exists in this \
                  tenant. Requires permission: members.manage",
    params(("id" = Uuid, Path, description = "Skill identifier")),
    responses(
        (status = 204, description = "Skill deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Skill not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_skill(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(skill_id): Path<Uuid>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_skill: begin tx failed");
            return ApiError::internal_error("Failed to delete skill")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM skills WHERE tenant_id = $1 AND id = $2")
            .bind(ctx.tenant_id)
            .bind(skill_id)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or(None);

    let name = match name {
        Some(n) => n,
        None => {
            tx.rollback().await.ok();
            return ApiError::not_found("Skill not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = queries::delete_skill_in_tx(&mut tx, ctx.tenant_id, skill_id).await {
        tracing::error!(%e, "delete_skill: delete failed");
        return ApiError::internal_error("Failed to delete skill")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    audit::record_skill_deleted(&mut tx, principal.user_id, ctx.tenant_id, skill_id, &name)
        .await
        .ok();

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_skill: commit failed");
        return ApiError::internal_error("Failed to delete skill")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// `PUT /tenant/members/{membershipId}/skills` — replace a member's skills.
#[utoipa::path(
    put,
    path = "/tenant/members/{membershipId}/skills",
    tag = "escalations",
    operation_id = "set_member_skills",
    summary = "Set a member's skills",
    description = "Replace the skill assignments for a member. `skillIds` is the full set of \
                  skills the member should hold; existing rows are deleted and the supplied \
                  ones inserted. The target membership must be agent-capable \
                  (`owner | admin | manager | agent`). Returns 200 with the doc-only \
                  `{data: Skill[]}` envelope (`MemberSkillsResponse`). \
                  Requires permission: members.manage",
    params(("membershipId" = Uuid, Path, description = "Membership identifier")),
    request_body = SetMemberSkillsPayload,
    responses(
        (status = 200, description = "Member skills updated.", body = MemberSkillsResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Membership not found in this tenant.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (target membership is not agent-capable, or unknown skill id).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_member_skills(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(membership_id): Path<Uuid>,
    ApiJson(payload): ApiJson<SetMemberSkillsPayload>,
) -> Response {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM tenant_memberships \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .bind(membership_id)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    match role.as_deref() {
        Some("owner" | "admin" | "manager" | "agent") => {}
        Some(_) => {
            return ApiError::unprocessable_entity("Target membership is not agent-capable")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        None => {
            return ApiError::not_found("Membership not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "set_member_skills: begin tx failed");
            return ApiError::internal_error("Failed to set member skills")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let skills = match queries::set_member_skills_in_tx(
        &mut tx,
        ctx.tenant_id,
        membership_id,
        &payload.skill_ids,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            if let sqlx::Error::Protocol(msg) = &e {
                return ApiError::unprocessable_entity(msg)
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            tracing::error!(%e, "set_member_skills: failed");
            return ApiError::internal_error("Failed to set member skills")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    audit::record_member_skills_changed(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        membership_id,
        &[],
        &payload.skill_ids,
    )
    .await
    .ok();

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "set_member_skills: commit failed");
        return ApiError::internal_error("Failed to set member skills")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    Json(json!({ "data": skills })).into_response()
}
