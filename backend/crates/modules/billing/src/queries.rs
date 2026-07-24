use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::model;

pub fn current_period(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let end = if now.month() == 12 {
        NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
    }
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap()
    .and_utc();
    (start, end)
}

pub async fn effective_plan(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<(model::PlanDto, String)> {
    let row = sqlx::query_as::<
        _,
        (Uuid, String, i32, Option<i64>, Option<i32>, bool, String, String),
    >(
        r#"
        SELECT p.id, p.name, p.monthly_price_cents, p.token_allowance,
               p.conversation_allowance, p.is_default, p.status,
               s.status AS subscription_status
        FROM tenant_subscriptions s
        JOIN billing_plans p ON p.id = s.plan_id
        WHERE s.tenant_id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    if let Some((
        id,
        name,
        monthly_price_cents,
        token_allowance,
        conversation_allowance,
        is_default,
        status,
        subscription_status,
    )) = row
    {
        Ok((
            model::PlanDto {
                id,
                name,
                monthly_price_cents,
                token_allowance,
                conversation_allowance,
                is_default,
                status,
            },
            subscription_status,
        ))
    } else {
        let default = sqlx::query_as::<_, (Uuid, String, i32, Option<i64>, Option<i32>, bool, String)>(
            r#"
            SELECT id, name, monthly_price_cents, token_allowance,
                   conversation_allowance, is_default, status
            FROM billing_plans
            WHERE is_default = true
            LIMIT 1
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok((
            model::PlanDto {
                id: default.0,
                name: default.1,
                monthly_price_cents: default.2,
                token_allowance: default.3,
                conversation_allowance: default.4,
                is_default: default.5,
                status: default.6,
            },
            "active".into(),
        ))
    }
}

pub async fn token_usage(
    pool: &PgPool,
    tenant_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(SUM(COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0)), 0)::BIGINT
        FROM ai_usage_records
        WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
        "#,
    )
    .bind(tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
}

pub async fn conversation_usage(
    pool: &PgPool,
    tenant_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT FROM conversations
        WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3 AND deleted_at IS NULL
        "#,
    )
    .bind(tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
}
