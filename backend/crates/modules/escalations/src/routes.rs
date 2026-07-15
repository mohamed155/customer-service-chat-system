use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use chrono::Utc;
use identity::Principal;
use kernel::{ApiError, ApiJson};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tenancy::TenantContext;

use crate::audit;
use crate::model::{
    Availability, AvailabilityState, CreateSkillPayload, CustomerRef, EscalatePayload, Escalation,
    EscalationAssignedEvent, EscalationQueuedEvent, EscalationRemovedEvent, EscalationStatus,
    QueueEntry, QueueEntryConversationRef, RenameSkillPayload, RequiredSkillRef,
    SetAvailabilityPayload, SetMemberSkillsPayload,
};
use crate::presence;
use crate::queries;
use crate::routing;

// ---------------------------------------------------------------------------
// Escalate
// ---------------------------------------------------------------------------

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

#[derive(Debug, Deserialize)]
pub struct QueueQueryParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

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
