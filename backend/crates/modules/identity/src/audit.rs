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
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(RESOURCE_TYPE_USER)
    .bind(resource_id)
    .bind(Option::<Uuid>::None)
    .bind(details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!(
            action = %action,
            resource_type = %RESOURCE_TYPE_USER,
            error = %e,
            "failed to record authentication audit log entry"
        );
    }
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
