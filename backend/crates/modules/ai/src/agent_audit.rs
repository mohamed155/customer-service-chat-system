use serde_json::json;

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub async fn record_ai_handling_set(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    mode: &str,
) -> sqlx::Result<()> {
    let details = json!({"mode": mode});
    tenancy::audit::record_in_tx(
        tx,
        "conversation.ai_handling_set",
        actor_user_id,
        Some(tenant_id),
        "conversation",
        Some(&conversation_id.to_string()),
        &details,
    )
    .await
}

pub async fn record_agent_config_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    agent_id: Uuid,
    payload: &serde_json::Value,
) -> sqlx::Result<()> {
    tenancy::audit::record_in_tx(
        tx,
        "agent_config.created",
        actor_user_id,
        Some(tenant_id),
        "agent_configuration",
        Some(&agent_id.to_string()),
        payload,
    )
    .await
}

pub async fn record_agent_config_updated(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    agent_id: Uuid,
    changed_fields: &[&str],
) -> sqlx::Result<()> {
    let details = json!({"changed_fields": changed_fields});
    tenancy::audit::record_in_tx(
        tx,
        "agent_config.updated",
        actor_user_id,
        Some(tenant_id),
        "agent_configuration",
        Some(&agent_id.to_string()),
        &details,
    )
    .await
}

pub async fn record_agent_config_avatar_updated(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    agent_id: Uuid,
    kind: &str,
    detail: &str,
) -> sqlx::Result<()> {
    let details = json!({"kind": kind, "detail": detail});
    tenancy::audit::record_in_tx(
        tx,
        "agent_config.avatar_updated",
        actor_user_id,
        Some(tenant_id),
        "agent_configuration",
        Some(&agent_id.to_string()),
        &details,
    )
    .await
}

pub async fn record_agent_prompt_version_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    prompt_id: Uuid,
    version_number: i32,
    content_len: usize,
    has_change_note: bool,
) -> sqlx::Result<()> {
    let details = json!({
        "version": version_number,
        "content_length": content_len,
        "has_change_note": has_change_note
    });
    tenancy::audit::record_in_tx(
        tx,
        "agent_prompt.version_created",
        actor_user_id,
        Some(tenant_id),
        "agent_prompt",
        Some(&prompt_id.to_string()),
        &details,
    )
    .await
}

pub async fn record_agent_prompt_version_restored(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    prompt_id: Uuid,
    version_number: i32,
    restored_from: i32,
    content_len: usize,
) -> sqlx::Result<()> {
    let details = json!({
        "version": version_number,
        "restored_from": restored_from,
        "content_length": content_len
    });
    tenancy::audit::record_in_tx(
        tx,
        "agent_prompt.version_restored",
        actor_user_id,
        Some(tenant_id),
        "agent_prompt",
        Some(&prompt_id.to_string()),
        &details,
    )
    .await
}
