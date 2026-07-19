//! Every query in this file must filter `tenant_id = $1` and must exclude
//! `deleted_at IS NOT NULL` conversations.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn conversation_counts(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: Option<&str>,
) -> sqlx::Result<(i64, i64, i64, i64)> {
    sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT
            COUNT(*)::bigint,
            COUNT(*) FILTER (WHERE c.status IN ('resolved','closed'))::bigint,
            COUNT(*) FILTER (WHERE c.status IN ('resolved','closed') AND NOT esc.escalated)::bigint,
            COUNT(*) FILTER (WHERE esc.escalated)::bigint
        FROM conversations c
        CROSS JOIN LATERAL (
            SELECT EXISTS (
                SELECT 1 FROM escalations es
                WHERE es.tenant_id = c.tenant_id AND es.conversation_id = c.id
            ) AS escalated
        ) esc
        WHERE c.tenant_id = $1
          AND c.deleted_at IS NULL
          AND c.created_at >= $2
          AND c.created_at < $3
          AND ($4::text IS NULL OR c.channel = $4)
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn avg_first_response_seconds(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: Option<&str>,
) -> sqlx::Result<Option<f64>> {
    sqlx::query_scalar::<_, Option<f64>>(
        r#"
        WITH cohort AS (
            SELECT c.id FROM conversations c
            WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
              AND c.created_at >= $2 AND c.created_at < $3
              AND ($4::text IS NULL OR c.channel = $4)
        ),
        first_customer AS (
            SELECT m.conversation_id, MIN(m.created_at) AS asked_at
            FROM messages m JOIN cohort ON cohort.id = m.conversation_id
            WHERE m.tenant_id = $1 AND m.kind = 'customer'
            GROUP BY m.conversation_id
        ),
        first_reply AS (
            SELECT fc.conversation_id, MIN(m.created_at) AS replied_at
            FROM first_customer fc
            JOIN messages m ON m.conversation_id = fc.conversation_id AND m.tenant_id = $1
            WHERE m.kind IN ('reply','ai') AND m.created_at > fc.asked_at
            GROUP BY fc.conversation_id
        )
        SELECT AVG(EXTRACT(EPOCH FROM (fr.replied_at - fc.asked_at)))::float8
        FROM first_customer fc
        JOIN first_reply fr ON fr.conversation_id = fc.conversation_id
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn avg_response_seconds(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: Option<&str>,
) -> sqlx::Result<Option<f64>> {
    sqlx::query_scalar::<_, Option<f64>>(
        r#"
        WITH cohort AS (
            SELECT c.id FROM conversations c
            WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
              AND c.created_at >= $2 AND c.created_at < $3
              AND ($4::text IS NULL OR c.channel = $4)
        )
        SELECT AVG(EXTRACT(EPOCH FROM (r.replied_at - m.created_at)))::float8
        FROM messages m
        JOIN cohort ON cohort.id = m.conversation_id
        CROSS JOIN LATERAL (
            SELECT MIN(m2.created_at) AS replied_at
            FROM messages m2
            WHERE m2.tenant_id = m.tenant_id
              AND m2.conversation_id = m.conversation_id
              AND m2.kind IN ('reply','ai')
              AND m2.created_at > m.created_at
        ) r
        WHERE m.tenant_id = $1 AND m.kind = 'customer' AND r.replied_at IS NOT NULL
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn satisfaction(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: Option<&str>,
) -> sqlx::Result<(Option<f64>, i64)> {
    sqlx::query_as::<_, (Option<f64>, i64)>(
        r#"
        SELECT AVG(f.rating)::float8, COUNT(*)::bigint
        FROM conversation_feedback f
        WHERE f.tenant_id = $1
          AND f.submitted_at >= $2
          AND f.submitted_at < $3
          AND ($4::text IS NULL OR f.channel = $4)
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn token_totals(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint,
            COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0))
                     FILTER (WHERE g.id IS NULL), 0)::bigint
        FROM ai_usage_records u
        LEFT JOIN LATERAL (
            SELECT gg.id FROM ai_generations gg
            WHERE gg.tenant_id = u.tenant_id AND gg.usage_record_id = u.id
            LIMIT 1
        ) g ON TRUE
        WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .fetch_one(pool)
    .await
}

pub async fn channel_breakdown(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> sqlx::Result<Vec<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT c.channel, COUNT(*)::bigint
        FROM conversations c
        WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
          AND c.created_at >= $2 AND c.created_at < $3
        GROUP BY c.channel
        ORDER BY COUNT(*) DESC, c.channel ASC
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .fetch_all(pool)
    .await
}

pub async fn token_totals_for_channel(
    pool: &PgPool,
    tenant_id: Uuid,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: &str,
) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint
        FROM ai_usage_records u
        WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
          AND EXISTS (
              SELECT 1 FROM ai_generations g
              JOIN conversations c ON c.tenant_id = g.tenant_id AND c.id = g.conversation_id
              WHERE g.tenant_id = u.tenant_id AND g.usage_record_id = u.id
                AND c.channel = $4 AND c.deleted_at IS NULL
          )
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .fetch_one(pool)
    .await
}

pub async fn daily_series(
    pool: &PgPool,
    tenant_id: Uuid,
    from_date: NaiveDate,
    to_date: NaiveDate,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
    channel: Option<&str>,
) -> sqlx::Result<Vec<(NaiveDate, i64, i64, i64, Option<f64>, i64, i64)>> {
    sqlx::query_as::<_, (NaiveDate, i64, i64, i64, Option<f64>, i64, i64)>(
        r#"
        WITH days AS (
            SELECT generate_series($5::date, $6::date, interval '1 day')::date AS day
        ),
        conv AS (
            SELECT (c.created_at AT TIME ZONE 'UTC')::date AS day,
                   COUNT(*)::bigint AS volume,
                   COUNT(*) FILTER (WHERE c.status IN ('resolved','closed') AND NOT esc.escalated)::bigint AS ai_resolved,
                   COUNT(*) FILTER (WHERE esc.escalated)::bigint AS handed_off
            FROM conversations c
            CROSS JOIN LATERAL (
                SELECT EXISTS (
                    SELECT 1 FROM escalations es
                    WHERE es.tenant_id = c.tenant_id AND es.conversation_id = c.id
                ) AS escalated
            ) esc
            WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
              AND c.created_at >= $2 AND c.created_at < $3
              AND ($4::text IS NULL OR c.channel = $4)
            GROUP BY 1
        ),
        fb AS (
            SELECT (f.submitted_at AT TIME ZONE 'UTC')::date AS day,
                   AVG(f.rating)::float8 AS avg_rating,
                   COUNT(*)::bigint AS rating_count
            FROM conversation_feedback f
            WHERE f.tenant_id = $1 AND f.submitted_at >= $2 AND f.submitted_at < $3
              AND ($4::text IS NULL OR f.channel = $4)
            GROUP BY 1
        ),
        tok AS (
            SELECT (u.created_at AT TIME ZONE 'UTC')::date AS day,
                   COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint AS tokens
            FROM ai_usage_records u
            WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
            GROUP BY 1
        )
        SELECT d.day,
               COALESCE(conv.volume, 0)::bigint,
               COALESCE(conv.ai_resolved, 0)::bigint,
               COALESCE(conv.handed_off, 0)::bigint,
               fb.avg_rating,
               COALESCE(fb.rating_count, 0)::bigint,
               COALESCE(tok.tokens, 0)::bigint
        FROM days d
        LEFT JOIN conv ON conv.day = d.day
        LEFT JOIN fb ON fb.day = d.day
        LEFT JOIN tok ON tok.day = d.day
        ORDER BY d.day
        "#,
    )
    .bind(tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(channel)
    .bind(from_date)
    .bind(to_date)
    .fetch_all(pool)
    .await
}
