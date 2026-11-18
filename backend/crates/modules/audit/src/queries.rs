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
    let ts = DateTime::parse_from_rfc3339(ts_str)
        .ok()?
        .with_timezone(&Utc);
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

/// The platform-wide listing: every tenant's entries plus the platform's own. It reads across
/// tenants, so it goes through the audited function `xtenant.platform_audit_entries`, which records
/// who asked. `requested_by` is required: the platform path never runs without an identity.
#[allow(clippy::too_many_arguments)]
pub async fn list_platform_entries(
    pool: &sqlx::PgPool,
    requested_by: Uuid,
    filter_tenant_id: Option<Uuid>,
    from_ts: Option<DateTime<Utc>>,
    to_ts: Option<DateTime<Utc>>,
    category_prefixes: Option<Vec<String>>,
    actor_id: Option<Uuid>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<AuditRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AuditRow>(
        "SELECT id, action, actor_user_id, resource_type, resource_id, tenant_id, details, \
                created_at, display_name, email, platform_role, deleted_at \
         FROM xtenant.platform_audit_entries($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(requested_by)
    .bind(filter_tenant_id)
    .bind(from_ts)
    .bind(to_ts)
    .bind(category_prefixes)
    .bind(actor_id)
    .bind(cursor.map(|(ts, _)| ts))
    .bind(cursor.map(|(_, id)| id))
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().take(limit as usize).collect())
}

/// One tenant's entries, in that tenant's Unit of Work.
#[allow(clippy::too_many_arguments)]
pub async fn list_entries(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
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
                NULL::text AS display_name, NULL::text AS email, \
                NULL::text AS platform_role, NULL::timestamptz AS deleted_at \
         FROM audit_logs a \
         WHERE a.tenant_id = ",
    );
    builder.push_bind(tenant_id);

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

    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let rows = builder
        .build_query_as::<AuditRow>()
        .fetch_all(uow.conn())
        .await?;

    let _has_more = rows.len() > limit as usize;
    let mut data: Vec<AuditRow> = rows.into_iter().take(limit as usize).collect();

    // Who did it is a person `identity` owns: the actor columns above are empty placeholders and
    // are filled from its directory, for the whole page in one query.
    let actors: Vec<Uuid> = data.iter().filter_map(|row| row.actor_user_id).collect();
    let profiles = identity::directory::profiles(uow.conn(), &actors).await?;
    for row in &mut data {
        if let Some(profile) = row.actor_user_id.and_then(|id| profiles.get(&id)) {
            row.display_name = Some(profile.display_name.clone());
            row.email = Some(profile.email.clone());
            row.platform_role = profile.platform_role.clone();
            row.deleted_at = profile.deleted_at;
        }
    }

    Ok(data)
}
