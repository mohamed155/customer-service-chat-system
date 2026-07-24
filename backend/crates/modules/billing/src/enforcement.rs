use sqlx::PgPool;
use uuid::Uuid;

use crate::queries;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    SubscriptionStatus,
    TokenLimit,
    ConversationLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnforcementDecision {
    pub allowed: bool,
    pub reason: Option<PauseReason>,
}

pub async fn check(pool: &PgPool, tenant_id: Uuid) -> Result<EnforcementDecision, sqlx::Error> {
    let (start, end) = queries::current_period(chrono::Utc::now());
    let (plan, subscription_status) = queries::effective_plan(pool, tenant_id).await?;

    match subscription_status.as_str() {
        "past_due" | "canceled" => {
            return Ok(EnforcementDecision {
                allowed: false,
                reason: Some(PauseReason::SubscriptionStatus),
            });
        }
        _ => {}
    }

    if let Some(limit) = plan.token_allowance {
        let tokens = queries::token_usage(pool, tenant_id, start, end).await?;
        if tokens >= limit {
            return Ok(EnforcementDecision {
                allowed: false,
                reason: Some(PauseReason::TokenLimit),
            });
        }
    }

    if let Some(limit) = plan.conversation_allowance {
        let conversations = queries::conversation_usage(pool, tenant_id, start, end).await?;
        if conversations >= limit as i64 {
            return Ok(EnforcementDecision {
                allowed: false,
                reason: Some(PauseReason::ConversationLimit),
            });
        }
    }

    Ok(EnforcementDecision {
        allowed: true,
        reason: None,
    })
}
