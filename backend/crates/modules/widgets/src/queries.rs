use chrono::{DateTime, Utc};
use rand::RngExt;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use crate::model::WidgetInstanceRow;
pub use crate::model::WidgetSessionRow;

pub fn generate_public_id() -> String {
    const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::rng();
    let id: String = (0..22)
        .map(|_| {
            let idx = rng.random_range(0..62);
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
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<WidgetSessionRow> = async {
    sqlx::query_as::<_, WidgetSessionRow>(
        "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, tenant_id, widget_instance_id, token_hash, customer_id, expires_at, created_at",
    )
    .bind(tenant_id)
    .bind(widget_instance_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(uow.conn())
    .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
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
    tenant_id: uuid::Uuid,
    session_id: Uuid,
    new_expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<()> = async {
        let updated = sqlx::query(
            "UPDATE widget_sessions SET last_seen_at = now(), expires_at = $1 \
         WHERE id = $2 AND tenant_id = $3",
        )
        .bind(new_expires_at)
        .bind(session_id)
        .bind(tenant_id)
        .execute(uow.conn())
        .await?;
        // No row means the session is gone (or belongs to another tenant): it was not refreshed.
        if updated.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

/// Link the session to its customer. Runs in the caller's transaction: the
/// customer row may have just been created there and isn't visible to other
/// connections until commit (the FK check would fail on a separate connection).
pub async fn set_session_customer(
    tx: &mut Transaction<'_, Postgres>,
    session_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE widget_sessions SET customer_id = $1 WHERE id = $2")
        .bind(customer_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn ensure_customer_for_session(
    tx: &mut Transaction<'_, Postgres>,
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
    set_session_customer(tx, session.id, customer_id).await?;
    Ok(customer_id)
}

pub async fn find_instance_by_id(
    pool: &PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
) -> sqlx::Result<Option<WidgetInstanceRow>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Option<WidgetInstanceRow>> = async {
        sqlx::query_as::<_, WidgetInstanceRow>(
            "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(instance_id)
        .bind(tenant_id)
        .fetch_optional(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn list_instances(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<WidgetInstanceRow>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<WidgetInstanceRow>> = async {
        sqlx::query_as::<_, WidgetInstanceRow>(
            "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

/// Runs in the caller's transaction so the widget row and its audit entry commit (or roll back)
/// together; the handler commits after the audit insert succeeds.
#[allow(clippy::too_many_arguments)]
pub async fn insert_instance(
    tx: &mut Transaction<'_, Postgres>,
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
    .fetch_one(&mut **tx)
    .await
}

/// Runs in the caller's transaction so the widget change and its audit entry commit (or roll back)
/// together; the handler commits after the audit insert succeeds.
#[allow(clippy::too_many_arguments)]
pub async fn update_instance(
    tx: &mut Transaction<'_, Postgres>,
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
    .fetch_optional(&mut **tx)
    .await
}

/// Runs in the caller's transaction: the delete handler holds the row with
/// `SELECT … FOR UPDATE`, so updating through a separate pool connection
/// blocked on that lock forever (DELETE never returned).
pub async fn soft_delete_instance(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    instance_id: Uuid,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE widget_instances SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(instance_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_expired_sessions(pool: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM widget_sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
