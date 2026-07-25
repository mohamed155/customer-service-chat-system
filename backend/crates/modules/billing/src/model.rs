use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct PlanDto {
    pub id: Uuid,
    pub name: String,
    pub monthly_price_cents: i32,
    pub token_allowance: Option<i64>,
    pub conversation_allowance: Option<i32>,
    pub is_default: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PeriodDto {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricUsageDto {
    pub used: i64,
    pub allowance: Option<i64>,
    pub percent: Option<i32>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UsageDto {
    pub tokens: MetricUsageDto,
    pub conversations: MetricUsageDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BillingOverviewDto {
    pub plan: PlanDto,
    pub subscription_status: String,
    pub period: PeriodDto,
    pub usage: UsageDto,
    pub ai_paused: bool,
    pub ai_paused_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlanListResponse {
    pub plans: Vec<PlanDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePlanRequest {
    pub name: String,
    pub monthly_price_cents: i32,
    pub token_allowance: Option<i64>,
    pub conversation_allowance: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePlanRequest {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionRowDto {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub plan_id: Uuid,
    pub plan_name: String,
    pub plan_is_default: bool,
    pub status: String,
    pub explicit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionListResponse {
    pub subscriptions: Vec<SubscriptionRowDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PutSubscriptionRequest {
    pub plan_id: Uuid,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionDto {
    pub tenant_id: Uuid,
    pub plan_id: Uuid,
    pub status: String,
}

/// `normal` below 80%, `warning` at 80..100%, `exceeded` at or above 100%.
/// An unlimited allowance (None) is always `normal` with no percent.
/// A zero allowance (`Some(0)`) is `exceeded` from the start — a deliberate
/// "AI disabled for this metric" plan, not a division-by-zero guard.
pub fn metric_usage(used: i64, allowance: Option<i64>) -> MetricUsageDto {
    match allowance {
        None => MetricUsageDto {
            used,
            allowance: None,
            percent: None,
            state: "normal".into(),
        },
        Some(limit) if limit <= 0 => MetricUsageDto {
            used,
            allowance: Some(limit),
            percent: Some(100),
            state: "exceeded".into(),
        },
        Some(limit) => {
            let percent = ((used as f64 / limit as f64) * 100.0).floor() as i32;
            let state = if used >= limit {
                "exceeded"
            } else if percent >= 80 {
                "warning"
            } else {
                "normal"
            };
            MetricUsageDto {
                used,
                allowance: Some(limit),
                percent: Some(percent),
                state: state.into(),
            }
        }
    }
}
