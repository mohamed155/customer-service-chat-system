use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::Utc;
use kernel::ApiError;
use sqlx::PgPool;
use uuid::Uuid;

use crate::audit;
use crate::model;
use crate::queries;

#[utoipa::path(
    get,
    path = "/tenant/billing/overview",
    tag = "billing",
    operation_id = "get_billing_overview",
    summary = "Tenant billing overview",
    responses(
        (status = 200, description = "Billing overview for the current period.", body = model::BillingOverviewDto),
        (status = 403, description = "Forbidden — missing billing.admin permission."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn get_billing_overview(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
) -> Response {
    let period = queries::current_period(Utc::now());
    match async {
        let (plan, subscription_status) = queries::effective_plan(&pool, ctx.tenant_id).await?;
        let token_used = queries::token_usage(&pool, ctx.tenant_id, period.0, period.1).await?;
        let conv_used =
            queries::conversation_usage(&pool, ctx.tenant_id, period.0, period.1).await?;
        Ok::<_, sqlx::Error>((plan, subscription_status, token_used, conv_used))
    }
    .await
    {
        Ok((plan, subscription_status, token_used, conv_used)) => {
            let tokens = model::metric_usage(token_used, plan.token_allowance);
            let conversations =
                model::metric_usage(conv_used, plan.conversation_allowance.map(|v| v as i64));
            let (ai_paused, ai_paused_reason) = match subscription_status.as_str() {
                "past_due" | "canceled" => (true, Some("subscription_status".into())),
                _ => {
                    if tokens.state == "exceeded" {
                        (true, Some("token_limit".into()))
                    } else if conversations.state == "exceeded" {
                        (true, Some("conversation_limit".into()))
                    } else {
                        (false, None)
                    }
                }
            };
            (
                StatusCode::OK,
                Json(model::BillingOverviewDto {
                    plan,
                    subscription_status,
                    period: model::PeriodDto {
                        start: period.0,
                        end: period.1,
                    },
                    usage: model::UsageDto {
                        tokens,
                        conversations,
                    },
                    ai_paused,
                    ai_paused_reason,
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(?e, "get_billing_overview");
            ApiError::internal_error("internal error").into_response()
        }
    }
}

// ── T042: Platform plan-catalog handlers ──────────────────────────────────

#[utoipa::path(
    get,
    path = "/platform/billing/plans",
    tag = "billing",
    operation_id = "list_billing_plans",
    summary = "List all billing plans",
    responses(
        (status = 200, description = "Plan list.", body = model::PlanListResponse),
        (status = 403, description = "Forbidden — missing platform.billing.view permission."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn list_plans(
    State(pool): State<PgPool>,
    Extension(_principal): Extension<identity::Principal>,
) -> Response {
    match queries::list_plans(&pool).await {
        Ok(plans) => (StatusCode::OK, Json(model::PlanListResponse { plans })).into_response(),
        Err(e) => {
            tracing::error!(?e, "list_plans");
            ApiError::internal_error("internal error").into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/platform/billing/plans",
    tag = "billing",
    operation_id = "create_billing_plan",
    summary = "Create a billing plan",
    request_body = model::CreatePlanRequest,
    responses(
        (status = 201, description = "Plan created.", body = model::PlanDto),
        (status = 403, description = "Forbidden — missing platform.billing.manage permission."),
        (status = 409, description = "Duplicate plan name."),
        (status = 422, description = "Validation failed."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn create_plan(
    State(pool): State<PgPool>,
    Extension(principal): Extension<identity::Principal>,
    kernel::ApiJson(payload): kernel::ApiJson<model::CreatePlanRequest>,
) -> Response {
    use kernel::ErrorDetail;

    let mut details: Vec<ErrorDetail> = Vec::new();

    if payload.name.trim().is_empty() {
        details.push(ErrorDetail {
            field: "name".into(),
            code: "invalid_value".into(),
            message: "Name must not be empty".into(),
        });
    }
    if payload.monthly_price_cents < 0 {
        details.push(ErrorDetail {
            field: "monthlyPriceCents".into(),
            code: "invalid_value".into(),
            message: "Monthly price must be non-negative".into(),
        });
    }
    if payload.token_allowance.is_some_and(|v| v < 0) {
        details.push(ErrorDetail {
            field: "tokenAllowance".into(),
            code: "invalid_value".into(),
            message: "Token allowance must be non-negative".into(),
        });
    }
    if payload.conversation_allowance.is_some_and(|v| v < 0) {
        details.push(ErrorDetail {
            field: "conversationAllowance".into(),
            code: "invalid_value".into(),
            message: "Conversation allowance must be non-negative".into(),
        });
    }
    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let plan = match queries::create_plan(&mut tx, &payload).await {
        Ok(plan) => plan,
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            return ApiError::conflict("A plan with this name already exists").into_response();
        }
        Err(e) => {
            return ApiError::internal_error(format!("Database insert failed: {e}"))
                .into_response();
        }
    };

    if let Err(e) = audit::plan_created(&mut tx, principal.user_id, &plan).await {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    (StatusCode::CREATED, Json(plan)).into_response()
}

#[utoipa::path(
    patch,
    path = "/platform/billing/plans/{plan_id}",
    tag = "billing",
    operation_id = "update_billing_plan",
    summary = "Update a billing plan's status",
    params(
        ("plan_id" = Uuid, Path, description = "Plan ID"),
    ),
    request_body = model::UpdatePlanRequest,
    responses(
        (status = 200, description = "Plan updated.", body = model::PlanDto),
        (status = 403, description = "Forbidden — missing platform.billing.manage permission."),
        (status = 404, description = "Plan not found."),
        (status = 409, description = "Cannot retire the default plan."),
        (status = 422, description = "Invalid status."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn update_plan(
    State(pool): State<PgPool>,
    Extension(principal): Extension<identity::Principal>,
    axum::extract::Path(plan_id): axum::extract::Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<model::UpdatePlanRequest>,
) -> Response {
    let valid_statuses = ["active", "retired"];
    if !valid_statuses.contains(&payload.status.as_str()) {
        return ApiError::unprocessable_entity(format!(
            "Invalid status. Must be one of: {}",
            valid_statuses.join(", ")
        ))
        .into_response();
    }

    let before = match queries::plan_by_id(&pool, plan_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return ApiError::not_found("Plan not found").into_response(),
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };

    if before.is_default && payload.status == "retired" {
        return ApiError::conflict("Cannot retire the default plan").into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let after = match queries::set_plan_status(&mut tx, plan_id, &payload.status).await {
        Ok(p) => p,
        Err(e) => {
            return ApiError::internal_error(format!("Database update failed: {e}"))
                .into_response();
        }
    };

    if let Err(e) = audit::plan_status_changed(
        &mut tx,
        principal.user_id,
        plan_id,
        &before.status,
        &payload.status,
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    (StatusCode::OK, Json(after)).into_response()
}

#[utoipa::path(
    get,
    path = "/platform/billing/subscriptions",
    tag = "billing",
    operation_id = "list_tenant_subscriptions",
    summary = "List all tenant subscriptions",
    responses(
        (status = 200, description = "Subscription list.", body = model::SubscriptionListResponse),
        (status = 403, description = "Forbidden — missing platform.billing.view permission."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn list_subscriptions(
    State(pool): State<PgPool>,
    Extension(_principal): Extension<identity::Principal>,
) -> Response {
    match queries::list_subscriptions(&pool).await {
        Ok(subscriptions) => (
            StatusCode::OK,
            Json(model::SubscriptionListResponse { subscriptions }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(?e, "list_subscriptions");
            ApiError::internal_error("internal error").into_response()
        }
    }
}

#[utoipa::path(
    put,
    path = "/platform/billing/tenants/{tenant_id}/subscription",
    tag = "billing",
    operation_id = "put_tenant_subscription",
    summary = "Set a tenant's subscription plan and status",
    params(
        ("tenant_id" = Uuid, Path, description = "Tenant ID"),
    ),
    request_body = model::PutSubscriptionRequest,
    responses(
        (status = 200, description = "Subscription updated.", body = model::SubscriptionDto),
        (status = 403, description = "Forbidden — missing platform.billing.manage permission."),
        (status = 404, description = "Tenant or plan not found."),
        (status = 409, description = "Plan is retired."),
        (status = 422, description = "Invalid status."),
        (status = 500, description = "Internal server error."),
    ),
)]
pub async fn put_subscription(
    State(pool): State<PgPool>,
    Extension(principal): Extension<identity::Principal>,
    axum::extract::Path(tenant_id): axum::extract::Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<model::PutSubscriptionRequest>,
) -> Response {
    let valid_statuses = ["active", "past_due", "canceled"];
    if !valid_statuses.contains(&payload.status.as_str()) {
        return ApiError::unprocessable_entity(format!(
            "Invalid status. Must be one of: {}",
            valid_statuses.join(", ")
        ))
        .into_response();
    }

    let tenant_exists = match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    {
        Ok(exists) => exists,
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };
    if !tenant_exists {
        return ApiError::not_found("Tenant not found").into_response();
    }

    let target_plan = match queries::plan_by_id(&pool, payload.plan_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return ApiError::not_found("Plan not found").into_response(),
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };

    // Read before-state for audit
    let before = match queries::subscription_for_tenant(&pool, tenant_id).await {
        Ok(sub) => sub,
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };

    // Assigning retired plan → 409 unless tenant is already on it
    if target_plan.status == "retired" {
        if let Some((_, current_plan_id, _)) = before {
            if current_plan_id != payload.plan_id {
                return ApiError::conflict("Cannot assign a retired plan to a tenant")
                    .into_response();
            }
        } else {
            return ApiError::conflict("Cannot assign a retired plan to a tenant").into_response();
        }
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let (subscription_id, after) = match queries::upsert_subscription(
        &mut tx,
        tenant_id,
        payload.plan_id,
        &payload.status,
        principal.user_id,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return ApiError::internal_error(format!("Database upsert failed: {e}"))
                .into_response();
        }
    };

    let before_json = serde_json::json!({
        "plan_id": before.as_ref().map(|(_, pid, _)| pid),
        "status": before.as_ref().map(|(_, _, s)| s),
    });
    let after_json = serde_json::json!({
        "plan_id": after.plan_id,
        "status": after.status,
    });

    if let Err(e) = audit::subscription_changed(
        &mut tx,
        principal.user_id,
        tenant_id,
        subscription_id,
        &before_json,
        &after_json,
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    (StatusCode::OK, Json(after)).into_response()
}
