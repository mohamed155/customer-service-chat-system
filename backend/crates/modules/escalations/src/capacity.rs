//! Tenant routing capacity API and queue drain after increases.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
    Extension,
};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorDetail, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{audit, presence, queries, routing};
use tenancy::TenantContext;

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetDefaultCapacityPayload {
    pub default_capacity: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetAgentCapacityPayload {
    pub capacity: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentCapacity {
    pub membership_id: Uuid,
    pub display_name: String,
    #[serde(rename = "override")]
    pub override_capacity: Option<i32>,
    pub effective: i32,
    pub open_load: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoutingCapacityResponse {
    pub default_capacity: i32,
    pub agents: Vec<AgentCapacity>,
}

fn invalid_capacity(field: &'static str, message: &'static str, request_id: &str) -> Response {
    ApiError::unprocessable_entity("Validation failed")
        .with_details(vec![ErrorDetail {
            field: field.to_owned(),
            code: "invalid_range".to_owned(),
            message: message.to_owned(),
        }])
        .with_request_id(request_id)
        .into_response()
}

async fn response_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<RoutingCapacityResponse> {
    let default_capacity = queries::default_capacity_in_tx(tx, tenant_id).await?;
    let membership_ids = tenancy::directory::active_member_ids(
        &mut **tx,
        tenant_id,
        tenancy::directory::ActiveMembers {
            roles: Some(&queries::ASSIGNABLE_ROLES),
            ..Default::default()
        },
    )
    .await?;
    let profiles = tenancy::directory::member_profiles(tx, tenant_id, &membership_ids).await?;
    let overrides = queries::capacity_overrides_in_tx(tx, tenant_id, &membership_ids).await?;
    let loads =
        conversations::directory::open_load_by_member(tx, tenant_id, &membership_ids).await?;

    let agents = membership_ids
        .into_iter()
        .filter_map(|membership_id| {
            let profile = profiles.get(&membership_id)?;
            if !profile.active {
                return None;
            }
            let override_capacity = overrides.get(&membership_id).copied().flatten();
            Some(AgentCapacity {
                membership_id,
                display_name: profile.display_name.clone(),
                override_capacity,
                effective: override_capacity.unwrap_or(default_capacity),
                open_load: loads.get(&membership_id).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(RoutingCapacityResponse {
        default_capacity,
        agents,
    })
}

async fn drain_to_capacity_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    present_ids: &[Uuid],
    actor_user_id: Uuid,
) -> sqlx::Result<Vec<crate::model::EscalationAssignedEvent>> {
    let mut events = Vec::new();
    while let Some(escalation_id) =
        routing::drain_any_in_tx(tx, tenant_id, present_ids, actor_user_id).await?
    {
        let row = queries::escalation_row_in_tx(tx, tenant_id, escalation_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        events.push(crate::model::EscalationAssignedEvent {
            v: 1,
            escalation_id,
            conversation_id: row.conversation_id,
            reason: row.reason,
            routing_reason: crate::model::RoutingReason::QueueAuto,
            matched_skills: row.matched_skill_names,
            assigned_at: row.assigned_at.unwrap_or(row.escalated_at),
        });
    }
    Ok(events)
}

fn broadcast_drained(
    runtime: &presence::Runtime,
    tenant_id: Uuid,
    events: Vec<crate::model::EscalationAssignedEvent>,
) {
    for event in events {
        let escalation_id = event.escalation_id;
        runtime.broadcast(tenant_id, presence::Event::EscalationAssigned(event));
        runtime.broadcast(
            tenant_id,
            presence::Event::EscalationRemoved(crate::model::EscalationRemovedEvent {
                v: 1,
                escalation_id,
                cause: "assigned".into(),
            }),
        );
    }
}

/// Read the current tenant default and the effective capacity/load of each active assignable member.
#[utoipa::path(
    get,
    path = "/tenant/routing/capacity",
    tag = "escalations",
    operation_id = "get_routing_capacity",
    summary = "Get routing capacity settings",
    description = "Return the tenant default and per-agent overrides, effective capacities, and open/pending conversation loads. Requires permission: members.view.",
    responses(
        (status = 200, description = "Routing capacity settings.", body = RoutingCapacityResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get(State(pool): State<sqlx::PgPool>, ctx: TenantContext) -> Response {
    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "get routing capacity: begin transaction failed");
            return ApiError::internal_error("Failed to fetch routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let capacity = match response_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(capacity) => capacity,
        Err(error) => {
            tracing::error!(%error, "get routing capacity failed");
            return ApiError::internal_error("Failed to fetch routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "get routing capacity: commit failed");
        return ApiError::internal_error("Failed to fetch routing capacity")
            .with_request_id(&ctx.request_id)
            .into_response();
    }
    Json(capacity).into_response()
}

/// Change the tenant default. An increase drains queued escalations up to available capacity.
#[utoipa::path(
    put,
    path = "/tenant/routing/capacity/default",
    tag = "escalations",
    operation_id = "set_default_routing_capacity",
    summary = "Set the tenant routing capacity default",
    description = "Set the inherited per-agent capacity from 1 through 200. Increasing it drains the queue while agents have room. Requires permission: members.manage.",
    request_body = SetDefaultCapacityPayload,
    responses(
        (status = 200, description = "Updated routing capacity settings.", body = RoutingCapacityResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Default capacity must be between 1 and 200.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_default(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
    ApiJson(payload): ApiJson<SetDefaultCapacityPayload>,
) -> Response {
    if !(1..=200).contains(&payload.default_capacity) {
        return invalid_capacity(
            "default_capacity",
            "Default capacity must be between 1 and 200",
            &ctx.request_id,
        );
    }
    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "set default routing capacity: begin transaction failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(error) = queries::take_tenant_routing_lock_in_tx(&mut tx, ctx.tenant_id).await {
        tracing::error!(%error, "set default routing capacity: acquire routing lock failed");
        return ApiError::internal_error("Failed to update routing capacity")
            .with_request_id(&ctx.request_id)
            .into_response();
    }
    let previous_capacity = match queries::default_capacity_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(capacity) => capacity,
        Err(error) => {
            tracing::error!(%error, "set default routing capacity: read previous value failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let mut drained = Vec::new();
    if previous_capacity != payload.default_capacity {
        if let Err(error) =
            queries::set_default_capacity_in_tx(&mut tx, ctx.tenant_id, payload.default_capacity)
                .await
        {
            tracing::error!(%error, "set default routing capacity: write failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if let Err(error) = audit::record_tenant_routing_capacity_changed(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            previous_capacity,
            payload.default_capacity,
        )
        .await
        {
            tracing::error!(%error, "set default routing capacity: audit failed");
            return ApiError::internal_error("Failed to record audit entry")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if payload.default_capacity > previous_capacity {
            let present_ids = runtime.present_membership_ids_async(ctx.tenant_id).await;
            drained = match drain_to_capacity_in_tx(
                &mut tx,
                ctx.tenant_id,
                &present_ids,
                principal.user_id,
            )
            .await
            {
                Ok(events) => events,
                Err(error) => {
                    tracing::error!(%error, "set default routing capacity: queue drain failed");
                    return ApiError::internal_error("Failed to update routing capacity")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };
        }
    }
    let response = match response_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "set default routing capacity: build response failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "set default routing capacity: commit failed");
        return ApiError::internal_error("Failed to update routing capacity")
            .with_request_id(&ctx.request_id)
            .into_response();
    }
    broadcast_drained(&runtime, ctx.tenant_id, drained);
    Json(response).into_response()
}

/// Set an agent override to an integer capacity or null to inherit the tenant default.
#[utoipa::path(
    put,
    path = "/tenant/routing/capacity/agents/{membership_id}",
    tag = "escalations",
    operation_id = "set_agent_routing_capacity",
    summary = "Set an agent routing capacity override",
    description = "Set an agent override from 0 through 200, or null to inherit the tenant default. Zero prevents automatic assignment. Increasing effective capacity drains the queue. Requires permission: members.manage.",
    params(("membership_id" = Uuid, Path, description = "Tenant membership identifier")),
    request_body = SetAgentCapacityPayload,
    responses(
        (status = 200, description = "Updated routing capacity settings.", body = RoutingCapacityResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Membership not found in this tenant.", body = ErrorEnvelope),
        (status = 422, description = "Capacity override must be null or between 0 and 200.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_agent(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(runtime): Extension<Arc<presence::Runtime>>,
    Path(membership_id): Path<Uuid>,
    ApiJson(payload): ApiJson<SetAgentCapacityPayload>,
) -> Response {
    if payload
        .capacity
        .is_some_and(|capacity| !(0..=200).contains(&capacity))
    {
        return invalid_capacity(
            "capacity",
            "Agent capacity must be null or between 0 and 200",
            &ctx.request_id,
        );
    }
    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "set agent routing capacity: begin transaction failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(error) = queries::take_tenant_routing_lock_in_tx(&mut tx, ctx.tenant_id).await {
        tracing::error!(%error, "set agent routing capacity: acquire routing lock failed");
        return ApiError::internal_error("Failed to update routing capacity")
            .with_request_id(&ctx.request_id)
            .into_response();
    }
    match tenancy::directory::lock_active_member(
        &mut **tx,
        ctx.tenant_id,
        membership_id,
        &queries::ASSIGNABLE_ROLES,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            return ApiError::not_found("Membership not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "set agent routing capacity: validate membership failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }
    let default_capacity = match queries::default_capacity_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(capacity) => capacity,
        Err(error) => {
            tracing::error!(%error, "set agent routing capacity: read default failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let previous_override = match queries::capacity_override_in_tx(
        &mut tx,
        ctx.tenant_id,
        membership_id,
    )
    .await
    {
        Ok(capacity) => capacity,
        Err(error) => {
            tracing::error!(%error, "set agent routing capacity: read previous override failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let previous_effective = previous_override.unwrap_or(default_capacity);
    let new_effective = payload.capacity.unwrap_or(default_capacity);
    let mut drained = Vec::new();
    if previous_override != payload.capacity {
        if let Err(error) = queries::set_capacity_override_in_tx(
            &mut tx,
            ctx.tenant_id,
            membership_id,
            payload.capacity,
        )
        .await
        {
            tracing::error!(%error, "set agent routing capacity: write override failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if let Err(error) = audit::record_agent_routing_capacity_changed(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            membership_id,
            previous_override,
            payload.capacity,
            previous_effective,
            new_effective,
        )
        .await
        {
            tracing::error!(%error, "set agent routing capacity: audit failed");
            return ApiError::internal_error("Failed to record audit entry")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if new_effective > previous_effective {
            let present_ids = runtime.present_membership_ids_async(ctx.tenant_id).await;
            drained = match drain_to_capacity_in_tx(
                &mut tx,
                ctx.tenant_id,
                &present_ids,
                principal.user_id,
            )
            .await
            {
                Ok(events) => events,
                Err(error) => {
                    tracing::error!(%error, "set agent routing capacity: queue drain failed");
                    return ApiError::internal_error("Failed to update routing capacity")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };
        }
    }
    let response = match response_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "set agent routing capacity: build response failed");
            return ApiError::internal_error("Failed to update routing capacity")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "set agent routing capacity: commit failed");
        return ApiError::internal_error("Failed to update routing capacity")
            .with_request_id(&ctx.request_id)
            .into_response();
    }
    broadcast_drained(&runtime, ctx.tenant_id, drained);
    Json(response).into_response()
}
