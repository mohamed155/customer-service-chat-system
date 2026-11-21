use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};

pub async fn record_member_invited(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    invitation_id: uuid::Uuid,
    email: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    audit_log::record_in_tx(
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
    audit_log::record_in_tx(
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
    audit_log::record_in_tx(
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
    audit_log::record_in_tx(
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
    audit_log::record_in_tx(
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
    audit_log::record_in_tx(
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

pub async fn record_tenant_signup_completed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    tenant_name: &str,
    slug: &str,
    email: &str,
) -> Result<(), sqlx::Error> {
    audit_log::record_in_tx(
        tx,
        "tenant.signup_completed",
        Some(actor_user_id),
        Some(tenant_id),
        "tenant",
        Some(&tenant_id.to_string()),
        &json!({"name": tenant_name, "slug": slug, "email": email}),
    )
    .await
}

pub async fn access_denied(
    pool: &PgPool,
    actor_user_id: Option<uuid::Uuid>,
    requested_tenant_id: &str,
    reason: &str,
) {
    audit_log::record(
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
