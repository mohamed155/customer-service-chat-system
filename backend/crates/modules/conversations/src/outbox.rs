use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub async fn emit_status_changed_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    old_status: &str,
    new_status: &str,
    actor_membership_id: Option<Uuid>,
    origin: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.status_changed', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(json!({
        "tenantId": tenant_id,
        "conversationId": conversation_id,
        "oldStatus": old_status,
        "newStatus": new_status,
        "actorMembershipId": actor_membership_id,
        "origin": origin,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn emit_assignment_changed_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    old_membership_id: Option<Uuid>,
    new_membership_id: Option<Uuid>,
    actor_membership_id: Option<Uuid>,
    origin: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.assignment_changed', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(json!({
        "tenantId": tenant_id,
        "conversationId": conversation_id,
        "oldMembershipId": old_membership_id,
        "newMembershipId": new_membership_id,
        "actorMembershipId": actor_membership_id,
        "origin": origin,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}
