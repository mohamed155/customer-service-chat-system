//! How a channel hands a customer's message to `conversations` (MOH-194, ADR-017).
//!
//! A channel (WhatsApp today) has resolved who the customer is. It asks `conversations` to open or
//! reuse that customer's conversation on the channel and to append the message, instead of
//! inserting into `conversations` and `messages` itself. Both run in the caller's transaction, so
//! the message, the channel's own metadata and the outbox event commit together.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// The customer's open conversation on `channel`, or a new one when they have none.
pub async fn open_or_create_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
) -> sqlx::Result<Uuid> {
    if let Some(id) =
        super::messages::find_open_conversation_in_tx(tx, tenant_id, customer_id, channel).await?
    {
        return Ok(id);
    }
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, $3, 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .fetch_one(&mut **tx)
    .await
}

/// Appends the customer's message to the conversation and marks the conversation active now.
/// Returns the new message's ID.
pub async fn append_customer_message_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
) -> sqlx::Result<Uuid> {
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE conversations SET last_activity_at = now() WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;
    Ok(message_id)
}
