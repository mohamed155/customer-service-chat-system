use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

pub fn encode_cursor(ts: DateTime<Utc>, id: Uuid) -> String {
    let payload = format!("{}|{}", ts.to_rfc3339(), id);
    hex::encode(payload.as_bytes())
}

pub fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = hex::decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (ts_str, id_str) = decoded.split_once('|')?;
    let ts = DateTime::parse_from_rfc3339(ts_str).ok()?.with_timezone(&Utc);
    let id = Uuid::parse_str(id_str).ok()?;
    Some((ts, id))
}

#[derive(Debug, Clone, FromRow)]
pub struct AuditRow {
    pub id: Uuid,
    pub action: String,
    pub actor_user_id: Option<Uuid>,
    pub resource_type: String,
    pub resource_id: String,
    pub tenant_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub platform_role: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[allow(clippy::too_many_arguments)]
pub async fn list_entries(
    pool: &sqlx::PgPool,
    tenant_scope: Option<Uuid>,
    filter_tenant_id: Option<Uuid>,
    from_ts: Option<DateTime<Utc>>,
    to_ts: Option<DateTime<Utc>>,
    category_prefixes: Option<Vec<String>>,
    actor_id: Option<Uuid>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<AuditRow>, sqlx::Error> {
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT a.id, a.action, a.actor_user_id, a.resource_type, a.resource_id, \
                a.tenant_id, a.details, a.created_at, \
                u.display_name, u.email, u.platform_role, u.deleted_at \
         FROM audit_logs a \
         LEFT JOIN users u ON u.id = a.actor_user_id \
         WHERE 1=1",
    );

    if let Some(ts) = tenant_scope {
        builder.push(" AND a.tenant_id = ");
        builder.push_bind(ts);
    }

    if let Some(tid) = filter_tenant_id {
        builder.push(" AND a.tenant_id = ");
        builder.push_bind(tid);
    }

    if let Some(from) = from_ts {
        builder.push(" AND a.created_at >= ");
        builder.push_bind(from);
    }

    if let Some(to) = to_ts {
        builder.push(" AND a.created_at <= ");
        builder.push_bind(to);
    }

    if let Some(ref prefixes) = category_prefixes {
        builder.push(" AND a.action LIKE ANY(");
        builder.push_bind(prefixes);
        builder.push(")");
    }

    if let Some(aid) = actor_id {
        builder.push(" AND a.actor_user_id = ");
        builder.push_bind(aid);
    }

    if let Some((ts, id)) = cursor {
        builder.push(" AND (a.created_at, a.id) < (");
        builder.push_bind(ts);
        builder.push(", ");
        builder.push_bind(id);
        builder.push(")");
    }

    builder.push(" ORDER BY a.created_at DESC, a.id DESC LIMIT ");
    builder.push_bind(limit + 1);

    let rows = builder.build_query_as::<AuditRow>().fetch_all(pool).await?;

    let _has_more = rows.len() > limit as usize;
    let data: Vec<AuditRow> = rows.into_iter().take(limit as usize).collect();

    Ok(data)
}
