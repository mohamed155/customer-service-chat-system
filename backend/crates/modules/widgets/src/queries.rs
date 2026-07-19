use chrono::{DateTime, Utc};
use rand::Rng;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use crate::model::WidgetInstanceRow;
pub use crate::model::WidgetSessionRow;

pub fn generate_public_id() -> String {
    const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    let id: String = (0..22)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            BASE62[idx] as char
        })
        .collect();
    format!("wgt_{}", id)
}

pub async fn find_instance_by_public_id(
    pool: &PgPool,
    public_id: &str,
) -> sqlx::Result<Option<WidgetInstanceRow>> {
    sqlx::query_as::<_, WidgetInstanceRow>(
        "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE public_id = $1 AND deleted_at IS NULL",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
}

pub async fn insert_session(
    pool: &PgPool,
    tenant_id: Uuid,
    widget_instance_id: Uuid,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> sqlx::Result<WidgetSessionRow> {
    sqlx::query_as::<_, WidgetSessionRow>(
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, tenant_id, widget_instance_id, token_hash, customer_id, expires_at, created_at",
    )
    .bind(tenant_id)
    .bind(widget_instance_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}

pub async fn find_session_by_token_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> sqlx::Result<Option<WidgetSessionRow>> {
    sqlx::query_as::<_, WidgetSessionRow>(
        "SELECT id, tenant_id, widget_instance_id, token_hash, customer_id, expires_at, created_at \
         FROM widget_sessions \
         WHERE token_hash = $1 AND expires_at > now()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn touch_session(
    pool: &PgPool,
    session_id: Uuid,
    new_expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE widget_sessions SET last_seen_at = now(), expires_at = $1 WHERE id = $2")
        .bind(new_expires_at)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_session_customer(
    pool: &PgPool,
    session_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE widget_sessions SET customer_id = $1 WHERE id = $2")
        .bind(customer_id)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn ensure_customer_for_session(
    tx: &mut Transaction<'_, Postgres>,
    pool: &PgPool,
    session: &WidgetSessionRow,
) -> sqlx::Result<Uuid> {
    if let Some(customer_id) = session.customer_id {
        return Ok(customer_id);
    }
    let short_code = session.id.to_string()[..6].to_uppercase();
    let display_name = format!("Visitor {}", short_code);
    let customer_id = customers::create_anonymous_customer_in_tx(
        tx,
        session.tenant_id,
        &display_name,
        "widget",
        &session.id.to_string(),
    )
    .await?;
    set_session_customer(pool, session.id, customer_id).await?;
    Ok(customer_id)
}

pub async fn find_instance_by_id(
    pool: &PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
) -> sqlx::Result<Option<WidgetInstanceRow>> {
    sqlx::query_as::<_, WidgetInstanceRow>(
        "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(instance_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub async fn list_instances(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<WidgetInstanceRow>> {
    sqlx::query_as::<_, WidgetInstanceRow>(
        "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_instance(
    pool: &PgPool,
    tenant_id: Uuid,
    public_id: &str,
    name: &str,
    display_name: &str,
    primary_color: Option<&str>,
    welcome_message: Option<&str>,
    position: Option<&str>,
    theme: Option<&str>,
    allowed_domains: &[String],
) -> sqlx::Result<WidgetInstanceRow> {
    sqlx::query_as::<_, WidgetInstanceRow>(
        "INSERT INTO widget_instances \
         (tenant_id, public_id, name, display_name, primary_color, welcome_message, \
          position, theme, allowed_domains) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id, tenant_id, public_id, name, display_name, primary_color, \
                   welcome_message, position, theme, enabled, allowed_domains, \
                   created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(public_id)
    .bind(name)
    .bind(display_name)
    .bind(primary_color)
    .bind(welcome_message)
    .bind(position)
    .bind(theme)
    .bind(allowed_domains)
    .fetch_one(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn update_instance(
    pool: &PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
    name: &str,
    display_name: &str,
    primary_color: Option<&str>,
    welcome_message: Option<&str>,
    position: Option<&str>,
    theme: Option<&str>,
    enabled: bool,
    allowed_domains: &[String],
) -> sqlx::Result<Option<WidgetInstanceRow>> {
    sqlx::query_as::<_, WidgetInstanceRow>(
        "UPDATE widget_instances SET \
         name = $3, display_name = $4, primary_color = $5, welcome_message = $6, \
         position = $7, theme = $8, enabled = $9, allowed_domains = $10 \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         RETURNING id, tenant_id, public_id, name, display_name, primary_color, \
                   welcome_message, position, theme, enabled, allowed_domains, \
                   created_at, updated_at",
    )
    .bind(instance_id)
    .bind(tenant_id)
    .bind(name)
    .bind(display_name)
    .bind(primary_color)
    .bind(welcome_message)
    .bind(position)
    .bind(theme)
    .bind(enabled)
    .bind(allowed_domains)
    .fetch_optional(pool)
    .await
}

pub async fn soft_delete_instance(
    pool: &PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE widget_instances SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(instance_id)
    .bind(tenant_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_expired_sessions(pool: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM widget_sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
