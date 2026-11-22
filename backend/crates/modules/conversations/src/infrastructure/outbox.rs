//! Writes conversation events to the transactional outbox.

use sqlx::PgConnection;

use crate::application::ConversationEvent;

/// Stores `event` in the outbox, in the transaction `conn` belongs to.
pub async fn publish(conn: &mut PgConnection, event: &ConversationEvent<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(kernel::ids::new_id())
    .bind(event.aggregate_type())
    .bind(event.conversation_id())
    .bind(event.tenant_id())
    .bind(event.event_type())
    .bind(event.payload())
    .execute(conn)
    .await?;
    Ok(())
}
