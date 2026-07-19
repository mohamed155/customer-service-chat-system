use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESOURCE_TYPE: &str = "widget_instance";
const ACTION_CREATED: &str = "widget_instance.created";
const ACTION_UPDATED: &str = "widget_instance.updated";
const ACTION_DELETED: &str = "widget_instance.deleted";

pub async fn record_instance_created_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    instance_id: Uuid,
    name: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_CREATED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&instance_id.to_string()),
        &json!({
            "instanceId": instance_id,
            "name": name,
        }),
    )
    .await
}

pub async fn record_instance_updated_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    instance_id: Uuid,
    name: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_UPDATED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&instance_id.to_string()),
        &json!({
            "instanceId": instance_id,
            "name": name,
        }),
    )
    .await
}

pub async fn record_instance_deleted_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    tenant_id: Uuid,
    instance_id: Uuid,
    name: &str,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_DELETED,
        Some(actor_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&instance_id.to_string()),
        &json!({
            "instanceId": instance_id,
            "name": name,
        }),
    )
    .await
}
