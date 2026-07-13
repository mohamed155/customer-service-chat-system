use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::error;

pub async fn record(
    pool: &PgPool,
    action: &str,
    actor_user_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(tenant_id)
    .bind(details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!(
            action = %action,
            resource_type = %resource_type,
            error = %e,
            "failed to record audit log entry"
        );
    }
}

/// Transactional variant: writes the audit row inside the caller's open
/// transaction and surfaces the sqlx error to the caller. Used by handlers
/// that require the audit row and the data mutation to commit atomically
/// (T042 / FR-009 / Constitution III).
pub async fn record_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    action: &str,
    actor_user_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(tenant_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn record_member_invited(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    invitation_id: uuid::Uuid,
    email: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.invited",
        Some(actor_user_id),
        Some(tenant_id),
        "invitation",
        Some(&invitation_id.to_string()),
        &json!({"email": email, "role": role}),
    )
    .await
}

pub async fn record_member_invitation_revoked(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    invitation_id: uuid::Uuid,
    email: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.invitation_revoked",
        Some(actor_user_id),
        Some(tenant_id),
        "invitation",
        Some(&invitation_id.to_string()),
        &json!({"email": email, "role": role}),
    )
    .await
}

pub async fn record_member_invitation_accepted(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    invitation_id: uuid::Uuid,
    email: &str,
    role: &str,
    user_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.invitation_accepted",
        Some(actor_user_id),
        Some(tenant_id),
        "invitation",
        Some(&invitation_id.to_string()),
        &json!({"email": email, "role": role, "user_id": user_id}),
    )
    .await
}

pub async fn record_member_role_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    membership_id: uuid::Uuid,
    previous_role: &str,
    new_role: &str,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.role_changed",
        Some(actor_user_id),
        Some(tenant_id),
        "membership",
        Some(&membership_id.to_string()),
        &json!({"previous_role": previous_role, "new_role": new_role}),
    )
    .await
}

pub async fn record_member_disabled(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    membership_id: uuid::Uuid,
    role: &str,
    previous_status: &str,
    new_status: &str,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.disabled",
        Some(actor_user_id),
        Some(tenant_id),
        "membership",
        Some(&membership_id.to_string()),
        &json!({"role": role, "previous_status": previous_status, "new_status": new_status}),
    )
    .await
}

pub async fn record_member_enabled(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    membership_id: uuid::Uuid,
    role: &str,
    previous_status: &str,
    new_status: &str,
) -> Result<(), sqlx::Error> {
    record_in_tx(
        tx,
        "member.enabled",
        Some(actor_user_id),
        Some(tenant_id),
        "membership",
        Some(&membership_id.to_string()),
        &json!({"role": role, "previous_status": previous_status, "new_status": new_status}),
    )
    .await
}

pub async fn access_denied(
    pool: &PgPool,
    actor_user_id: Option<uuid::Uuid>,
    requested_tenant_id: &str,
    reason: &str,
) {
    record(
        pool,
        "tenant.access_denied",
        actor_user_id,
        None,
        "tenant",
        Some(requested_tenant_id),
        &json!({"requested_tenant_id": requested_tenant_id, "reason": reason}),
    )
    .await;
}
