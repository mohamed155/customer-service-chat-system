use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use chrono::Utc;
use kernel::ApiError;
use sqlx::PgPool;

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
        let conv_used = queries::conversation_usage(&pool, ctx.tenant_id, period.0, period.1).await?;
        Ok::<_, sqlx::Error>((plan, subscription_status, token_used, conv_used))
    }.await {
        Ok((plan, subscription_status, token_used, conv_used)) => {
            let tokens = model::metric_usage(token_used, plan.token_allowance);
            let conversations = model::metric_usage(conv_used, plan.conversation_allowance.map(|v| v as i64));
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
            (StatusCode::OK, Json(model::BillingOverviewDto {
                plan,
                subscription_status,
                period: model::PeriodDto { start: period.0, end: period.1 },
                usage: model::UsageDto { tokens, conversations },
                ai_paused,
                ai_paused_reason,
            })).into_response()
        }
        Err(e) => {
            tracing::error!(?e, "get_billing_overview");
            ApiError::internal_error("internal error").into_response()
        }
    }
}
