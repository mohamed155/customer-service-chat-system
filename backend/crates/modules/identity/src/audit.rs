use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

const RESOURCE_TYPE_USER: &str = "user";

pub async fn record(
    pool: &PgPool,
    action: &str,
    actor_user_id: Option<Uuid>,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) {
    // Authentication events have no tenant, so the row is written by the audited function
    // `xtenant.record_auth_event`, which only accepts `auth.*` actions.
    let result = record_in(pool, action, actor_user_id, resource_id, details).await;

    if let Err(e) = result {
        error!(
            action = %action,
            resource_type = %RESOURCE_TYPE_USER,
            error = %e,
            "failed to record authentication audit log entry"
        );
    }
}

pub(crate) async fn record_in<'c>(
    db: impl sqlx::PgExecutor<'c>,
    action: &str,
    actor_user_id: Option<Uuid>,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query("SELECT xtenant.record_auth_event($1, $2, $3, $4)")
        .bind(action)
        .bind(actor_user_id)
        .bind(resource_id)
        .bind(details)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn login_succeeded(pool: &PgPool, user_id: Uuid) {
    let resource_id = user_id.to_string();

    record(
        pool,
        "auth.login_succeeded",
        Some(user_id),
        Some(resource_id.as_str()),
        &json!({}),
    )
    .await;
}

pub async fn login_failed(pool: &PgPool, email: &str, reason: &str) {
    // `resource_id` is NOT NULL-constrained (audit_logs_resource_required);
    // use the email as the resource identifier so the row can be correlated
    // with the user record even when the lookup didn't resolve to one.
    record(
        pool,
        "auth.login_failed",
        None,
        Some(email),
        &json!({
            "email": email,
            "reason": reason,
        }),
    )
    .await;
}

pub async fn account_locked(
    pool: &PgPool,
    user_id: Uuid,
    failed_attempts: i32,
    locked_until: DateTime<Utc>,
    lock_seconds: i32,
) {
    let resource_id = user_id.to_string();
    record(
        pool,
        "auth.account_locked",
        None,
        Some(&resource_id),
        &json!({
            "failed_attempts": failed_attempts,
            "locked_until": locked_until,
            "lock_seconds": lock_seconds,
        }),
    )
    .await;
}

pub async fn account_unlocked(pool: &PgPool, user_id: Uuid, reason: &str) {
    let resource_id = user_id.to_string();
    record(
        pool,
        "auth.account_unlocked",
        None,
        Some(&resource_id),
        &json!({ "reason": reason }),
    )
    .await;
}

pub async fn logged_out(pool: &PgPool, user_id: Uuid, jti: Uuid) {
    let resource_id = user_id.to_string();

    record(
        pool,
        "auth.logged_out",
        Some(user_id),
        Some(resource_id.as_str()),
        &json!({ "jti": jti }),
    )
    .await;
}
