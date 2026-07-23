use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESOURCE_TYPE: &str = "integration_connection";
const ACTION_CONNECTED: &str = "integration.connected";
const ACTION_CONFIG_UPDATED: &str = "integration.config_updated";
const ACTION_SECRET_ROTATED: &str = "integration.secret_rotated";
const ACTION_DISCONNECTED: &str = "integration.disconnected";

pub async fn record_connected_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    connection_id: Uuid,
    slug: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_CONNECTED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&connection_id.to_string()),
        &json!({
            "connectionId": connection_id,
            "slug": slug,
        }),
    )
    .await
}

pub async fn record_config_updated_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    connection_id: Uuid,
    slug: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_CONFIG_UPDATED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&connection_id.to_string()),
        &json!({
            "connectionId": connection_id,
            "slug": slug,
        }),
    )
    .await
}

pub async fn record_secret_rotated_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    connection_id: Uuid,
    slug: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_SECRET_ROTATED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&connection_id.to_string()),
        &json!({
            "connectionId": connection_id,
            "slug": slug,
        }),
    )
    .await
}

pub async fn record_disconnected_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    connection_id: Uuid,
    slug: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_DISCONNECTED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&connection_id.to_string()),
        &json!({
            "connectionId": connection_id,
            "slug": slug,
        }),
    )
    .await
}
