use chrono::{DateTime, Utc};
use sqlx::{FromRow, Postgres, Transaction};
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
pub struct CatalogRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_available: bool,
    pub config_schema: serde_json::Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct CatalogWithStatusRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_available: bool,
    pub config_schema: serde_json::Value,
    pub connection_id: Option<Uuid>,
    pub is_active: Option<bool>,
    pub outcomes: Option<Vec<String>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ConnectionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub catalog_id: Uuid,
    pub is_active: bool,
    pub config: serde_json::Value,
    pub webhook_token_hash: Vec<u8>,
    pub webhook_token_ciphertext: Vec<u8>,
    pub webhook_token_nonce: Vec<u8>,
    pub connected_at: DateTime<Utc>,
    pub connected_by_membership_id: Option<Uuid>,
    pub disconnected_at: Option<DateTime<Utc>>,
    pub disconnected_by_membership_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SecretRefRow {
    pub field_key: String,
    pub hint: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub id: Uuid,
    pub event_type: String,
    pub outcome: String,
    pub reason: Option<String>,
    pub actor_membership_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub async fn find_catalog_by_slug(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<Option<CatalogRow>, sqlx::Error> {
    sqlx::query_as::<_, CatalogRow>(
        "SELECT id, slug, name, description, category, is_available, config_schema \
         FROM integration_catalog WHERE slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub async fn find_connection(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    catalog_id: Uuid,
) -> Result<Option<ConnectionRow>, sqlx::Error> {
    sqlx::query_as::<_, ConnectionRow>(
        "SELECT id, tenant_id, catalog_id, is_active, config, \
                webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce, \
                connected_at, connected_by_membership_id, \
                disconnected_at, disconnected_by_membership_id \
         FROM integration_connections \
         WHERE tenant_id = $1 AND catalog_id = $2",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_connection_by_id(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
) -> Result<Option<ConnectionRow>, sqlx::Error> {
    sqlx::query_as::<_, ConnectionRow>(
        "SELECT id, tenant_id, catalog_id, is_active, config, \
                webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce, \
                connected_at, connected_by_membership_id, \
                disconnected_at, disconnected_by_membership_id \
         FROM integration_connections \
         WHERE id = $1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_secret_refs(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
) -> Result<Vec<SecretRefRow>, sqlx::Error> {
    sqlx::query_as::<_, SecretRefRow>(
        "SELECT field_key, hint \
         FROM integration_secrets \
         WHERE connection_id = $1 \
         ORDER BY field_key",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await
}

pub async fn list_catalog_with_status(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<Vec<CatalogWithStatusRow>, sqlx::Error> {
    sqlx::query_as::<_, CatalogWithStatusRow>(
        "SELECT cat.id, cat.slug, cat.name, cat.description, cat.category, \
                cat.is_available, cat.config_schema, \
                c.id AS connection_id, c.is_active, ev.outcomes \
         FROM integration_catalog cat \
         LEFT JOIN integration_connections c \
           ON c.catalog_id = cat.id AND c.tenant_id = $1 \
         LEFT JOIN LATERAL ( \
           SELECT array_agg(e.outcome ORDER BY e.created_at DESC) AS outcomes \
           FROM ( \
             SELECT outcome, created_at FROM integration_events \
             WHERE connection_id = c.id \
               AND created_at > now() - interval '24 hours' \
             ORDER BY created_at DESC LIMIT 3 \
           ) e \
         ) ev ON TRUE \
         ORDER BY cat.name",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

pub async fn recent_event_outcomes(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT outcome FROM integration_events \
         WHERE connection_id = $1 \
           AND created_at > now() - interval '24 hours' \
         ORDER BY created_at DESC LIMIT 3",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(o,)| o).collect())
}

pub async fn list_events(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<EventRow>, sqlx::Error> {
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, event_type, outcome, reason, actor_membership_id, created_at \
         FROM integration_events \
         WHERE connection_id = ",
    );
    builder.push_bind(connection_id);

    if let Some((ts, id)) = cursor {
        builder.push(" AND (created_at, id) < (");
        builder.push_bind(ts);
        builder.push(", ");
        builder.push_bind(id);
        builder.push(")");
    }

    builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
    builder.push_bind(limit + 1);

    builder
        .build_query_as::<EventRow>()
        .fetch_all(pool)
        .await
}

pub async fn upsert_connection(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    catalog_id: Uuid,
    webhook_token_hash: &[u8],
    webhook_token_ciphertext: &[u8],
    webhook_token_nonce: &[u8],
    connected_by_membership_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO integration_connections ( \
            tenant_id, catalog_id, is_active, config, \
            webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce, \
            connected_at, connected_by_membership_id \
         ) VALUES ($1, $2, true, '{}'::jsonb, $3, $4, $5, now(), $6) \
         ON CONFLICT (tenant_id, catalog_id) DO UPDATE SET \
            is_active = true, \
            config = EXCLUDED.config, \
            webhook_token_hash = EXCLUDED.webhook_token_hash, \
            webhook_token_ciphertext = EXCLUDED.webhook_token_ciphertext, \
            webhook_token_nonce = EXCLUDED.webhook_token_nonce, \
            connected_at = now(), \
            connected_by_membership_id = EXCLUDED.connected_by_membership_id, \
            disconnected_at = NULL, \
            disconnected_by_membership_id = NULL, \
            updated_at = now() \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(webhook_token_hash)
    .bind(webhook_token_ciphertext)
    .bind(webhook_token_nonce)
    .bind(connected_by_membership_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

pub async fn update_connection_config(
    tx: &mut Transaction<'_, Postgres>,
    connection_id: Uuid,
    config: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE integration_connections \
         SET config = $2, updated_at = now() \
         WHERE id = $1",
    )
    .bind(connection_id)
    .bind(config)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn upsert_secret(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    connection_id: Uuid,
    field_key: &str,
    ciphertext: &[u8],
    nonce: &[u8],
    hint: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO integration_secrets ( \
            tenant_id, connection_id, field_key, ciphertext, nonce, hint \
         ) VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (connection_id, field_key) DO UPDATE SET \
            ciphertext = EXCLUDED.ciphertext, \
            nonce = EXCLUDED.nonce, \
            hint = EXCLUDED.hint, \
            updated_at = now()",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(field_key)
    .bind(ciphertext)
    .bind(nonce)
    .bind(hint)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn delete_secrets_for_connection(
    tx: &mut Transaction<'_, Postgres>,
    connection_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM integration_secrets WHERE connection_id = $1")
        .bind(connection_id)
        .execute(&mut **tx)
        .await?;
    Ok(result.rows_affected())
}

pub async fn deactivate_connection(
    tx: &mut Transaction<'_, Postgres>,
    connection_id: Uuid,
    disconnected_by_membership_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE integration_connections \
         SET is_active = false, \
             disconnected_at = now(), \
             disconnected_by_membership_id = $2, \
             updated_at = now() \
         WHERE id = $1",
    )
    .bind(connection_id)
    .bind(disconnected_by_membership_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    connection_id: Uuid,
    event_type: &str,
    outcome: &str,
    reason: Option<&str>,
    actor_membership_id: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO integration_events ( \
            tenant_id, connection_id, event_type, outcome, reason, actor_membership_id \
         ) VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(event_type)
    .bind(outcome)
    .bind(reason)
    .bind(actor_membership_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

#[derive(Debug, Clone)]
pub struct ConnectionByToken {
    pub connection_id: Uuid,
    pub tenant_id: Uuid,
    pub catalog_slug: String,
    pub is_active: bool,
}

/// Look up a connection by the SHA-256 hash of its intake token. Used by the
/// public `POST /hooks/v1/{token}` handler to resolve `{token}` → connection
/// without leaking which tokens map to which tenants (the call site must
/// treat unknown and inactive cases identically to the outside world).
pub async fn find_connection_by_token_hash(
    pool: &sqlx::PgPool,
    hash: &[u8],
) -> Result<Option<ConnectionByToken>, sqlx::Error> {
    let row: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as(
        "SELECT c.id, c.tenant_id, cat.slug, c.is_active \
         FROM integration_connections c \
         JOIN integration_catalog cat ON cat.id = c.catalog_id \
         WHERE c.webhook_token_hash = $1",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(connection_id, tenant_id, catalog_slug, is_active)| ConnectionByToken {
        connection_id,
        tenant_id,
        catalog_slug,
        is_active,
    }))
}

/// Insert an accepted webhook delivery row. The body has already been parsed
/// as JSON by the caller; we persist it as `jsonb` so retention/sweeping can
/// operate on the structured payload later.
pub async fn insert_delivery(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    connection_id: Uuid,
    payload: &serde_json::Value,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO integration_webhook_deliveries ( \
            tenant_id, connection_id, payload \
         ) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(payload)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

/// Fetch the encrypted bytes (ciphertext, nonce) for a single secret on a
/// connection. The caller decrypts with `crypto::open` using the right AAD.
/// Returns `None` if no row exists for the given `field_key` — the webhook
/// handler treats that as an `invalid_signature` rejection.
pub async fn find_secret_ciphertext(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
    field_key: &str,
) -> Result<Option<(Vec<u8>, Vec<u8>)>, sqlx::Error> {
    let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT ciphertext, nonce FROM integration_secrets \
         WHERE connection_id = $1 AND field_key = $2",
    )
    .bind(connection_id)
    .bind(field_key)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConnectionByTokenRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub catalog_id: Uuid,
    pub is_active: bool,
    pub config: serde_json::Value,
}

pub async fn connection_by_token_and_slug(
    pool: &sqlx::PgPool,
    token_hash: &[u8],
    slug: &str,
) -> Result<Option<ConnectionByTokenRow>, sqlx::Error> {
    sqlx::query_as::<_, ConnectionByTokenRow>(
        "SELECT c.id, c.tenant_id, c.catalog_id, c.is_active, c.config \
         FROM integration_connections c \
         JOIN integration_catalog cat ON cat.id = c.catalog_id \
         WHERE c.webhook_token_hash = $1 AND cat.slug = $2",
    )
    .bind(token_hash)
    .bind(slug)
    .fetch_optional(pool)
    .await
}

pub async fn decrypted_secret(
    pool: &sqlx::PgPool,
    master_key: &crate::crypto::MasterKey,
    connection_id: Uuid,
    field_key: &str,
) -> Result<Option<String>, sqlx::Error> {
    let conn_info = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT c.tenant_id, cat.slug \
         FROM integration_connections c \
         JOIN integration_catalog cat ON cat.id = c.catalog_id \
         WHERE c.id = $1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await?;

    let (tenant_id, slug) = match conn_info {
        Some(row) => row,
        None => return Ok(None),
    };

    let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
        "SELECT ciphertext, nonce FROM integration_secrets \
         WHERE connection_id = $1 AND field_key = $2",
    )
    .bind(connection_id)
    .bind(field_key)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((ciphertext, nonce)) => {
            let scope = crate::crypto::aad(tenant_id, &slug, field_key);
            let plaintext = crate::crypto::open(master_key, &scope, &ciphertext, &nonce)
                .map_err(|e| sqlx::Error::Protocol(format!("decryption failed: {e}")))?;
            Ok(Some(plaintext))
        }
        None => Ok(None),
    }
}

pub async fn active_connection_for_slug(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    slug: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT c.id FROM integration_connections c \
         JOIN integration_catalog cat ON cat.id = c.catalog_id \
         WHERE c.tenant_id = $1 AND cat.slug = $2 AND c.is_active = true AND c.disconnected_at IS NULL",
    )
    .bind(tenant_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
}
