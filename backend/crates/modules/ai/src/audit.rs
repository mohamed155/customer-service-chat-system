use serde_json::json;

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub async fn config_updated(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Option<Uuid>,
    config_id: Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "ai_config.updated",
        Some(actor_user_id),
        tenant_id,
        "ai_configuration",
        Some(&config_id.to_string()),
        details,
    )
    .await
}

pub async fn config_deleted(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Option<Uuid>,
    config_id: Uuid,
    details: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "ai_config.deleted",
        Some(actor_user_id),
        tenant_id,
        "ai_configuration",
        Some(&config_id.to_string()),
        details,
    )
    .await
}

pub async fn capture_content_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    config_id: Uuid,
    old: bool,
    new: bool,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "ai_config.capture_content_changed",
        Some(actor_user_id),
        Some(tenant_id),
        "ai_configuration",
        Some(&config_id.to_string()),
        &json!({"old": old, "new": new}),
    )
    .await
}

pub async fn credential_set(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Option<Uuid>,
    credential_id: Uuid,
    provider: &str,
    key_hint: &str,
    rotated: bool,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "ai_credential.set",
        Some(actor_user_id),
        tenant_id,
        "ai_credential",
        Some(&credential_id.to_string()),
        &json!({"provider": provider, "key_hint": key_hint, "rotated": rotated}),
    )
    .await
}

pub async fn credential_deleted(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Option<Uuid>,
    credential_id: Uuid,
    provider: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "ai_credential.deleted",
        Some(actor_user_id),
        tenant_id,
        "ai_credential",
        Some(&credential_id.to_string()),
        &json!({"provider": provider}),
    )
    .await
}
