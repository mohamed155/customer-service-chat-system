use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Json, Response},
    Extension,
};
use chrono::{DateTime, Utc};
use escalations::model::{AvailabilityState, Escalation, Skill};
use identity::Principal;
use kernel::{ApiError, ErrorDetail, Page};
use serde::Serialize;
use sqlx::PgPool;
use tenancy::{members, TenantContext};
use utoipa::ToSchema;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// T062 — list_members_with_skills
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberWithSkills {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,
    pub skills: Vec<Skill>,
    pub availability: AvailabilityState,
}

#[utoipa::path(
    get,
    path = "/tenant/members",
    tag = "members",
    params(members::TeamMemberQuery),
    responses(
        (status = 200, description = "Paginated list of team members with their skills and availability. Requires permission: members.view.", body = Page<TeamMemberWithSkills>),
        (status = 400, description = "Invalid query parameters", body = kernel::ErrorEnvelope),
        (status = 401, description = "Authentication required", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions", body = kernel::ErrorEnvelope),
        (status = 422, description = "Validation failed", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal server error", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_members_with_skills(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Query(params): Query<members::TeamMemberQuery>,
) -> Response {
    let mut details: Vec<ErrorDetail> = Vec::new();

    if let Some(ref q) = params.q {
        if q.len() > 254 {
            details.push(ErrorDetail {
                field: "q".into(),
                code: "too_long".into(),
                message: "Search query must be 254 characters or fewer".into(),
            });
        }
    }

    if let Some(ref status) = params.status {
        if status != "active" && status != "disabled" {
            details.push(ErrorDetail {
                field: "status".into(),
                code: "invalid_value".into(),
                message: "Status must be active or disabled".into(),
            });
        }
    }

    if params.limit == 0 || params.limit > 100 {
        details.push(ErrorDetail {
            field: "limit".into(),
            code: "invalid_range".into(),
            message: "Limit must be between 1 and 100".into(),
        });
    }

    if let Some(ref cursor) = params.cursor {
        let valid_cursor = cursor
            .split_once('_')
            .and_then(|(joined_str, id_str)| {
                chrono::DateTime::parse_from_rfc3339(joined_str)
                    .ok()
                    .and_then(|_| Uuid::parse_str(id_str).ok())
            })
            .is_some();

        if !valid_cursor {
            details.push(ErrorDetail {
                field: "cursor".into(),
                code: "invalid_value".into(),
                message: "Cursor must be a valid roster cursor".into(),
            });
        }
    }

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin transaction");
            return ApiError::internal_error("Failed to fetch team members").into_response();
        }
    };

    let q = params.q.as_deref();
    let status = params.status.as_deref();
    let cursor = params.cursor.as_deref();
    let limit = params.limit;

    let (rows, has_more) =
        match members::list_members_rows_in_tx(&mut tx, ctx.tenant_id, q, status, cursor, limit)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(error = %e, "failed to fetch team members");
                return ApiError::internal_error("Failed to fetch team members").into_response();
            }
        };

    let membership_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    let skills_map = if membership_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        match escalations::queries::skills_and_availability_for_members_in_tx(
            &mut tx,
            ctx.tenant_id,
            &membership_ids,
        )
        .await
        {
            Ok(map) => map,
            Err(e) => {
                tracing::error!(error = %e, "failed to fetch skills and availability");
                return ApiError::internal_error("Failed to fetch skills and availability")
                    .into_response();
            }
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit transaction");
        return ApiError::internal_error("Failed to commit transaction").into_response();
    }

    let items: Vec<TeamMemberWithSkills> = rows
        .into_iter()
        .map(|r| {
            let (skills, availability) = skills_map
                .get(&r.id)
                .cloned()
                .unwrap_or((Vec::new(), AvailabilityState::Away));
            TeamMemberWithSkills {
                id: r.id,
                user_id: r.user_id,
                display_name: r.display_name,
                email: r.email,
                role: r.role,
                status: r.status,
                joined_at: r.joined_at,
                skills,
                availability,
            }
        })
        .collect();

    let next_cursor = items
        .last()
        .map(|last| format!("{}_{}", last.joined_at.format("%+"), last.id));

    Json(Page {
        items,
        next_cursor,
        has_more,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// T072 — get_conversation_with_escalation
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ConversationDetailWithEscalation {
    #[serde(flatten)]
    detail: conversations::model::ConversationDetail,
    escalation: Option<Escalation>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ConversationDetailEnvelope {
    data: ConversationDetailWithEscalation,
}

#[utoipa::path(
    get,
    path = "/tenant/conversations/{id}",
    tag = "conversations",
    params(("id" = Uuid, Path, description = "Conversation identifier")),
    responses(
        (status = 200, description = "Conversation detail merged with the latest escalation context (when present). Requires permission: conversations.view.", body = ConversationDetailEnvelope),
        (status = 401, description = "Authentication required", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions", body = kernel::ErrorEnvelope),
        (status = 404, description = "Conversation not found", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal server error", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_conversation_with_escalation(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Extension(ai_status): Extension<Arc<dyn conversations::AiAgentStatus>>,
    Path(conversation_id): Path<Uuid>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin transaction");
            return ApiError::internal_error("Failed to load conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let detail = match conversations::queries::detail_query_in_tx(
        &mut tx,
        ctx.tenant_id,
        conversation_id,
    )
    .await
    {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, conversation_id = %conversation_id, "detail query failed");
            return ApiError::internal_error("Failed to load conversation")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let escalation = match escalations::queries::latest_escalation_for_conversation_in_tx(
        &mut tx,
        ctx.tenant_id,
        conversation_id,
    )
    .await
    {
        Ok(esc) => esc,
        Err(e) => {
            tracing::error!(error = %e, conversation_id = %conversation_id, "escalation query failed");
            return ApiError::internal_error("Failed to load escalation data")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit transaction");
        return ApiError::internal_error("Failed to load conversation")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let agent_configured = ai_status.agent_configured(ctx.tenant_id).await;
    let platform_available = ai_status.platform_ai_available(ctx.tenant_id).await;

    let has_system_ack =
        conversations::queries::has_system_message(&pool, ctx.tenant_id, conversation_id)
            .await
            .unwrap_or(false);

    let mut detail = detail;
    detail.awaiting_ai_decision = !agent_configured
        && has_system_ack
        && match detail.ai_handling.as_deref() {
            None => true,
            Some("platform_ai") => !platform_available,
            Some("human") => false,
            _ => true,
        };

    let detail_with_esc = ConversationDetailWithEscalation { detail, escalation };

    Json(serde_json::json!({ "data": detail_with_esc })).into_response()
}

// ── T043: Wrapper for decide_tool_request that emits notification.resolve ──

#[derive(serde::Deserialize)]
pub struct DecideRequest {
    pub decision: String,
}

#[utoipa::path(
    post,
    path = "/tenant/tools/requests/{id}/decide",
    tag = "tools",
    operation_id = "decide_tool_request",
    summary = "Decide (approve/deny) a pending tool request",
    description = "Allows an authorized staff member to approve or deny an awaiting-approval tool request. Returns 200 on first decision, 409 with the settled state on a duplicate.",
    params(
        ("id" = Uuid, Path, description = "Tool request ID"),
    ),
    responses(
        (status = 200, description = "Decision applied", body = serde_json::Value),
        (status = 409, description = "Already settled", body = serde_json::Value),
        (status = 422, description = "Invalid decision value", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn decide_tool_request(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    Path(id): Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<DecideRequest>,
) -> Response {
    let approve = match payload.decision.as_str() {
        "approve" => true,
        "deny" => false,
        _ => {
            return ApiError::unprocessable_entity("decision must be 'approve' or 'deny'")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

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
            return ApiError::not_found("Membership not found in this tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "decide: resolve membership failed");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match tools::approval::decide(&pool, ctx.tenant_id, id, membership_id, approve).await {
        Ok(tools::approval::DecideOutcome::Applied(row)) => {
            Json(serde_json::json!(row)).into_response()
        }
        Ok(tools::approval::DecideOutcome::AlreadySettled(row)) => {
            ApiError::conflict("Tool request has already been decided")
                .with_details(vec![serde_json::json!(row)])
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(e) => {
            tracing::error!(%e, "decide_tool_request failed");
            ApiError::internal_error("Failed to decide tool request")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}
