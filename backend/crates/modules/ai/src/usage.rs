use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Write model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UsageWrite {
    pub tenant_id: Uuid,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub status: &'static str,
    pub error_category: Option<&'static str>,
    pub streamed: bool,
    pub latency_ms: i32,
    pub request_id: Option<String>,
    pub request_content: Option<serde_json::Value>,
    pub response_content: Option<String>,
}

// ---------------------------------------------------------------------------
// Read models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UsageListItem {
    pub id: Uuid,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub status: String,
    pub error_category: Option<String>,
    pub streamed: bool,
    pub latency_ms: i32,
    pub request_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UsageSummary {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub unreported_calls: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UsageDetailRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub status: String,
    pub error_category: Option<String>,
    pub streamed: bool,
    pub latency_ms: i32,
    pub request_id: Option<String>,
    pub request_content: Option<serde_json::Value>,
    pub response_content: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Response envelope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pagination {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}

// ---------------------------------------------------------------------------
// Cursor helpers (keyset pagination on (created_at DESC, id DESC))
// ---------------------------------------------------------------------------

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    let payload = format!("{}|{}", created_at.to_rfc3339(), id);
    hex::encode(payload.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = hex::decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (ts, id) = decoded.split_once('|')?;
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let id = Uuid::parse_str(id).ok()?;
    Some((ts, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_cursor_roundtrip() {
        let ts = Utc::now();
        let id = Uuid::new_v4();
        let cursor = encode_cursor(ts, id);
        let (decoded_ts, decoded_id) = decode_cursor(&cursor).unwrap();
        assert_eq!(decoded_ts.timestamp(), ts.timestamp());
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn test_cursor_invalid() {
        assert!(decode_cursor("zzzz").is_none());
        assert!(decode_cursor("").is_none());
    }
}

// ---------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------

pub async fn insert(pool: &PgPool, w: UsageWrite) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, \
          error_category, streamed, latency_ms, request_id, \
          request_content, response_content) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         RETURNING id",
    )
    .bind(w.tenant_id)
    .bind(&w.provider)
    .bind(&w.model)
    .bind(w.input_tokens)
    .bind(w.output_tokens)
    .bind(w.status)
    .bind(w.error_category)
    .bind(w.streamed)
    .bind(w.latency_ms)
    .bind(&w.request_id)
    .bind(&w.request_content)
    .bind(&w.response_content)
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// List (cursor-paginated, metadata only — no content columns)
// ---------------------------------------------------------------------------

pub async fn list(
    pool: &PgPool,
    tenant_id: Uuid,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    cursor: Option<String>,
    limit: i64,
) -> Result<PaginatedResponse<UsageListItem>, sqlx::Error> {
    let decoded = cursor.as_ref().and_then(|c| decode_cursor(c));

    let mut sql = String::from(
        "SELECT id, provider, model, input_tokens, output_tokens, status, \
                error_category, streamed, latency_ms, request_id, created_at \
         FROM ai_usage_records \
         WHERE tenant_id = $1",
    );

    let mut next_bind = 2u16;

    if from.is_some() {
        sql.push_str(&format!(" AND created_at >= ${next_bind}::timestamptz"));
        next_bind += 1;
    }
    if to.is_some() {
        sql.push_str(&format!(" AND created_at < ${next_bind}::timestamptz"));
        next_bind += 1;
    }

    if decoded.is_some() {
        sql.push_str(&format!(
            " AND (created_at, id) < (${a}::timestamptz, ${b}::uuid)",
            a = next_bind,
            b = next_bind + 1
        ));
        next_bind += 2;
    }

    sql.push_str(&format!(
        " ORDER BY created_at DESC, id DESC LIMIT ${next_bind}"
    ));

    let mut query = sqlx::query_as::<_, UsageListItem>(&sql).bind(tenant_id);

    if let Some(f) = from {
        query = query.bind(f);
    }
    if let Some(t) = to {
        query = query.bind(t);
    }
    if let Some((ts, id_)) = decoded {
        query = query.bind(ts).bind(id_);
    }

    query = query.bind(limit + 1);

    let rows = query.fetch_all(pool).await?;
    let has_more = rows.len() > limit as usize;
    let items: Vec<UsageListItem> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = has_more.then(|| {
        let last = items.last().expect("page with more rows has a last item");
        encode_cursor(last.created_at, last.id)
    });

    Ok(PaginatedResponse {
        data: items,
        pagination: Pagination {
            next_cursor,
            has_more,
        },
    })
}

// ---------------------------------------------------------------------------
// Summary (aggregate)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
struct UsageSummaryRow {
    calls: i64,
    input_tokens: i64,
    output_tokens: i64,
    unreported_calls: i64,
}

#[allow(unused_assignments)]
pub async fn summary(
    pool: &PgPool,
    tenant_id: Uuid,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<UsageSummary, sqlx::Error> {
    let mut sql = String::from(
        "SELECT COUNT(*)::bigint AS calls, \
                COALESCE(SUM(input_tokens), 0)::bigint AS input_tokens, \
                COALESCE(SUM(output_tokens), 0)::bigint AS output_tokens, \
                COUNT(*) FILTER (WHERE input_tokens IS NULL OR output_tokens IS NULL)::bigint AS unreported_calls \
         FROM ai_usage_records \
         WHERE tenant_id = $1",
    );

    let mut next_bind = 2u16;

    if from.is_some() {
        sql.push_str(&format!(" AND created_at >= ${next_bind}::timestamptz"));
        next_bind += 1;
    }
    if to.is_some() {
        sql.push_str(&format!(" AND created_at < ${next_bind}::timestamptz"));
        next_bind += 1;
    }

    let mut query = sqlx::query_as::<_, UsageSummaryRow>(&sql).bind(tenant_id);

    if let Some(f) = from {
        query = query.bind(f);
    }
    if let Some(t) = to {
        query = query.bind(t);
    }

    let row = query.fetch_one(pool).await?;

    Ok(UsageSummary {
        from,
        to,
        calls: row.calls,
        input_tokens: row.input_tokens,
        output_tokens: row.output_tokens,
        unreported_calls: row.unreported_calls,
    })
}

// ---------------------------------------------------------------------------
// Detail (single row, full content)
// ---------------------------------------------------------------------------

pub async fn detail(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Option<UsageDetailRow> {
    sqlx::query_as::<_, UsageDetailRow>(
        "SELECT id, tenant_id, provider, model, input_tokens, output_tokens, \
                status, error_category, streamed, latency_ms, request_id, \
                request_content, response_content, created_at \
         FROM ai_usage_records \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()?
}
