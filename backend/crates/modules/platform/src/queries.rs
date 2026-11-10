use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub async fn tenant_counts(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<(i64, i64, i64, i64)> {
    sqlx::query_as(
        "SELECT COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE status = 'active')::bigint,
                COUNT(*) FILTER (WHERE status = 'suspended')::bigint,
                COUNT(*) FILTER (WHERE created_at >= $1 AND created_at < $2)::bigint
         FROM tenants WHERE deleted_at IS NULL",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_one(pool)
    .await
}

// The tenant counts above read `tenants`, which has no `tenant_id` and is never under Row-Level
// Security. Everything below reads tenant-owned tables across tenants, so each goes through an
// audited function in the `xtenant` schema.

pub async fn active_conversation_count(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT xtenant.platform_active_conversation_count()")
        .fetch_one(pool)
        .await
}

pub async fn ai_usage_totals(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<(i64, i64, i64, i64, i64)> {
    sqlx::query_as(
        "SELECT total, successes, failures, input_tokens, output_tokens
         FROM xtenant.platform_ai_usage_totals($1, $2)",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_one(pool)
    .await
}

pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, Option<DateTime<Utc>>, i64)> {
    sqlx::query_as("SELECT pending, oldest, stuck FROM xtenant.platform_queue_health()")
        .fetch_one(pool)
        .await
}

#[derive(Debug, sqlx::FromRow)]
pub struct ProviderMetricsRow {
    pub provider: String,
    pub total: i64,
    pub successes: i64,
    pub failures: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
    pub authentication: i64,
    pub rate_limited: i64,
    pub unavailable: i64,
    pub timeout: i64,
    pub invalid_request: i64,
}

pub async fn provider_metrics(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<Vec<ProviderMetricsRow>> {
    sqlx::query_as(
        "SELECT provider, total, successes, failures, input_tokens, output_tokens,
                p50_latency_ms, p95_latency_ms, authentication, rate_limited, unavailable,
                timeout, invalid_request
         FROM xtenant.platform_provider_metrics($1, $2)",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_all(pool)
    .await
}

pub async fn recent_provider_failures(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<Vec<(String, String, Option<String>, i32, DateTime<Utc>)>> {
    sqlx::query_as(
        "SELECT provider, model, error_category, latency_ms, created_at
         FROM xtenant.platform_recent_provider_failures($1, $2)",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_all(pool)
    .await
}
