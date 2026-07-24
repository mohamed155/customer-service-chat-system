use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::{PgPool, Postgres, Transaction};
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

// ── T040: Platform plan catalog / subscription queries ─────────────────────

pub async fn list_plans(pool: &PgPool) -> sqlx::Result<Vec<model::PlanDto>> {
    let rows = sqlx::query_as::<_, (Uuid, String, i32, Option<i64>, Option<i32>, bool, String)>(
        "SELECT id, name, monthly_price_cents, token_allowance, conversation_allowance, \
         is_default, status FROM billing_plans ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, monthly_price_cents, token_allowance, conversation_allowance,
              is_default, status)| model::PlanDto {
                id,
                name,
                monthly_price_cents,
                token_allowance,
                conversation_allowance,
                is_default,
                status,
            },
        )
        .collect())
}

pub async fn create_plan(
    tx: &mut Transaction<'_, Postgres>,
    req: &model::CreatePlanRequest,
) -> sqlx::Result<model::PlanDto> {
    let row = sqlx::query_as::<_, (Uuid, String, i32, Option<i64>, Option<i32>, bool, String)>(
        "INSERT INTO billing_plans (name, monthly_price_cents, token_allowance, \
         conversation_allowance, is_default, status) \
         VALUES ($1, $2, $3, $4, false, 'active') \
         RETURNING id, name, monthly_price_cents, token_allowance, \
         conversation_allowance, is_default, status",
    )
    .bind(&req.name)
    .bind(req.monthly_price_cents)
    .bind(req.token_allowance)
    .bind(req.conversation_allowance)
    .fetch_one(&mut **tx)
    .await?;
    Ok(model::PlanDto {
        id: row.0,
        name: row.1,
        monthly_price_cents: row.2,
        token_allowance: row.3,
        conversation_allowance: row.4,
        is_default: row.5,
        status: row.6,
    })
}

pub async fn plan_by_id(pool: &PgPool, plan_id: Uuid) -> sqlx::Result<Option<model::PlanDto>> {
    let row = sqlx::query_as::<_, (Uuid, String, i32, Option<i64>, Option<i32>, bool, String)>(
        "SELECT id, name, monthly_price_cents, token_allowance, conversation_allowance, \
         is_default, status FROM billing_plans WHERE id = $1",
    )
    .bind(plan_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(id, name, monthly_price_cents, token_allowance, conversation_allowance,
          is_default, status)| model::PlanDto {
            id,
            name,
            monthly_price_cents,
            token_allowance,
            conversation_allowance,
            is_default,
            status,
        },
    ))
}

pub async fn set_plan_status(
    tx: &mut Transaction<'_, Postgres>,
    plan_id: Uuid,
    status: &str,
) -> sqlx::Result<model::PlanDto> {
    let row = sqlx::query_as::<_, (Uuid, String, i32, Option<i64>, Option<i32>, bool, String)>(
        "UPDATE billing_plans SET status = $1 WHERE id = $2 \
         RETURNING id, name, monthly_price_cents, token_allowance, \
         conversation_allowance, is_default, status",
    )
    .bind(status)
    .bind(plan_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(model::PlanDto {
        id: row.0,
        name: row.1,
        monthly_price_cents: row.2,
        token_allowance: row.3,
        conversation_allowance: row.4,
        is_default: row.5,
        status: row.6,
    })
}

pub async fn list_subscriptions(pool: &PgPool) -> sqlx::Result<Vec<model::SubscriptionRowDto>> {
    let rows = sqlx::query_as::<_, (Uuid, String, Uuid, String, bool, String, bool)>(
        "SELECT t.id, t.name, \
         COALESCE(p.id, d.id), \
         COALESCE(p.name, d.name), \
         COALESCE(p.is_default, d.is_default), \
         COALESCE(s.status, 'active'), \
         (s.id IS NOT NULL) \
         FROM tenants t \
         LEFT JOIN tenant_subscriptions s ON s.tenant_id = t.id \
         LEFT JOIN billing_plans p ON p.id = s.plan_id \
         CROSS JOIN (SELECT id, name, is_default FROM billing_plans WHERE is_default) d \
         WHERE t.deleted_at IS NULL \
         ORDER BY t.name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(tenant_id, tenant_name, plan_id, plan_name, plan_is_default, status, explicit)| {
                model::SubscriptionRowDto {
                    tenant_id,
                    tenant_name,
                    plan_id,
                    plan_name,
                    plan_is_default,
                    status,
                    explicit,
                }
            },
        )
        .collect())
}

/// Returns (subscription_id, plan_id, status) for a tenant, if explicit.
pub async fn subscription_for_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Option<(Uuid, Uuid, String)>> {
    sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT id, plan_id, status FROM tenant_subscriptions WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_subscription(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    plan_id: Uuid,
    status: &str,
    assigned_by: Uuid,
) -> sqlx::Result<(Uuid, model::SubscriptionDto)> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, String)>(
        "INSERT INTO tenant_subscriptions (tenant_id, plan_id, status, assigned_by) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (tenant_id) DO UPDATE SET \
         plan_id = EXCLUDED.plan_id, status = EXCLUDED.status, \
         assigned_by = EXCLUDED.assigned_by \
         RETURNING id, tenant_id, plan_id, status",
    )
    .bind(tenant_id)
    .bind(plan_id)
    .bind(status)
    .bind(assigned_by)
    .fetch_one(&mut **tx)
    .await?;
    Ok((
        row.0,
        model::SubscriptionDto {
            tenant_id: row.1,
            plan_id: row.2,
            status: row.3,
        },
    ))
}
