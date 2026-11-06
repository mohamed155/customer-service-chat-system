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

pub async fn active_conversation_count(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM conversations
         WHERE status IN ('open','pending') AND deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await
}

pub async fn ai_usage_totals(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<(i64, i64, i64, i64, i64)> {
    sqlx::query_as(
        "SELECT COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE status = 'success')::bigint,
                COUNT(*) FILTER (WHERE status = 'failure')::bigint,
                COALESCE(SUM(input_tokens), 0)::bigint,
                COALESCE(SUM(output_tokens), 0)::bigint
         FROM ai_usage_records WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_one(pool)
    .await
}

pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, Option<DateTime<Utc>>, i64)> {
    sqlx::query_as(
        "SELECT COUNT(*)::bigint,
                MIN(created_at),
                COUNT(*) FILTER (WHERE attempts > 5)::bigint
         FROM outbox_events WHERE processed_at IS NULL",
    )
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
        "SELECT provider,
                COUNT(*)::bigint AS total,
                COUNT(*) FILTER (WHERE status = 'success')::bigint AS successes,
                COUNT(*) FILTER (WHERE status = 'failure')::bigint AS failures,
                COALESCE(SUM(input_tokens), 0)::bigint AS input_tokens,
                COALESCE(SUM(output_tokens), 0)::bigint AS output_tokens,
                (percentile_cont(0.5)  WITHIN GROUP (ORDER BY latency_ms::float8))::bigint AS p50_latency_ms,
                (percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms::float8))::bigint AS p95_latency_ms,
                COUNT(*) FILTER (WHERE error_category = 'authentication')::bigint AS authentication,
                COUNT(*) FILTER (WHERE error_category = 'rate_limited')::bigint AS rate_limited,
                COUNT(*) FILTER (WHERE error_category = 'unavailable')::bigint AS unavailable,
                COUNT(*) FILTER (WHERE error_category = 'timeout')::bigint AS timeout,
                COUNT(*) FILTER (WHERE error_category = 'invalid_request')::bigint AS invalid_request
         FROM ai_usage_records
         WHERE created_at >= $1 AND created_at < $2
         GROUP BY provider",
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
         FROM ai_usage_records
         WHERE status = 'failure' AND created_at >= $1 AND created_at < $2
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(from_ts)
    .bind(to_ts)
    .fetch_all(pool)
    .await
}
