use crate::model::{NotificationRow, NotificationState, SubjectType};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ── Cursor helpers (hex-encoded "(ts|id)" tuple) ────────────────────

pub fn encode_cursor(ts: DateTime<Utc>, id: Uuid) -> String {
    let payload = format!("{}|{}", ts.to_rfc3339(), id);
    hex::encode(payload.as_bytes())
}

pub fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = hex::decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (ts_str, id_str) = decoded.split_once('|')?;
    let ts = DateTime::parse_from_rfc3339(ts_str)
        .ok()?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id_str).ok()?;
    Some((ts, id))
}

// ── Write paths ─────────────────────────────────────────────────────

pub async fn fan_out(
    pool: &PgPool,
    req: &super::emit::NotificationRequest,
    recipients: &[Uuid],
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO notifications \
         (tenant_id, recipient_membership_id, kind, state, title, body, subject_type, subject_id, dedupe_key, actor_membership_id) \
         SELECT $1, UNNEST($2::uuid[]), $3, 'unread', $4, $5, $6, $7, $8, $9 \
         ON CONFLICT (recipient_membership_id, dedupe_key) DO NOTHING",
    )
    .bind(req.tenant_id)
    .bind(recipients)
    .bind(req.kind.as_str())
    .bind(&req.title)
    .bind(&req.body)
    .bind(req.subject_type.as_str())
    .bind(req.subject_id)
    .bind(&req.dedupe_key)
    .bind(req.actor_membership_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn resolve_subject(
    pool: &PgPool,
    tenant_id: Uuid,
    subject_type: &SubjectType,
    subject_id: Uuid,
    resolved_by: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE notifications SET state = 'resolved', updated_at = now() \
         WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3 \
           AND state = 'unread' \
           AND recipient_membership_id IS DISTINCT FROM $4 \
         RETURNING recipient_membership_id",
    )
    .bind(tenant_id)
    .bind(subject_type.as_str())
    .bind(subject_id)
    .bind(resolved_by)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.get("recipient_membership_id")).collect())
}

// ── Read paths ──────────────────────────────────────────────────────

pub async fn list(
    pool: &PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
    state: Option<&NotificationState>,
    cursor: Option<&str>,
    limit: i64,
) -> Result<(Vec<NotificationRow>, Option<String>), sqlx::Error> {
    let limit = limit.clamp(1, 50);
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, tenant_id, recipient_membership_id, kind, state, title, body, \
                subject_type, subject_id, dedupe_key, actor_membership_id, \
                created_at, updated_at, read_at \
         FROM notifications \
         WHERE tenant_id = ",
    );
    builder.push_bind(tenant_id);
    builder.push(" AND recipient_membership_id = ");
    builder.push_bind(membership_id);

    if let Some(nd_state) = state {
        builder.push(" AND state = ");
        builder.push_bind(nd_state.as_str());
    }

    if let Some(cursor_str) = cursor {
        if let Some((ts, id)) = decode_cursor(cursor_str) {
            builder.push(" AND (created_at, id) < (");
            builder.push_bind(ts);
            builder.push(", ");
            builder.push_bind(id);
            builder.push(")");
        }
    }

    builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
    builder.push_bind(limit + 1);

    let rows = builder.build_query_as::<NotificationRow>().fetch_all(pool).await?;

    let has_more = rows.len() > limit as usize;
    let data: Vec<NotificationRow> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = data.last().map(|last| encode_cursor(last.created_at, last.id));

    Ok((data, if has_more { next_cursor } else { None }))
}

pub async fn unread_count(
    pool: &PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(*) as count FROM notifications \
         WHERE tenant_id = $1 AND recipient_membership_id = $2 AND state = 'unread'",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_one(pool)
    .await?;

    Ok(row.get("count"))
}

pub async fn mark_read(
    pool: &PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
    notification_id: Uuid,
) -> Result<Option<NotificationRow>, sqlx::Error> {
    let result = sqlx::query_as::<_, NotificationRow>(
        "UPDATE notifications SET state = 'read', read_at = now(), updated_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND recipient_membership_id = $3 \
         RETURNING *",
    )
    .bind(notification_id)
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

pub async fn mark_all_read(
    pool: &PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE notifications SET state = 'read', read_at = now(), updated_at = now() \
         WHERE tenant_id = $1 AND recipient_membership_id = $2 AND state = 'unread'",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
