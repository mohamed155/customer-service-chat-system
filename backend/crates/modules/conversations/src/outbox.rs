//! Events other modules ask `conversations` to put in the outbox, in the transaction they hold.
//!
//! The events themselves (names and payloads) are defined by the module's `ConversationEvent`;
//! these functions are the facade over writing them.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::application::ConversationEvent;
use crate::infrastructure::outbox::publish;

/// A customer wrote on `channel`: the AI responder and the notifications pick it up.
pub async fn emit_customer_message_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    channel: &str,
) -> sqlx::Result<()> {
    publish(
        tx,
        &ConversationEvent::CustomerMessage {
            tenant_id,
            conversation_id,
            message_id,
            channel,
        },
    )
    .await
}

/// A tool request was approved, denied, cancelled or expired.
pub async fn emit_tool_decision_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    tool_request_id: Uuid,
    outcome: &str,
) -> sqlx::Result<()> {
    publish(
        tx,
        &ConversationEvent::ToolDecision {
            tenant_id,
            conversation_id,
            tool_request_id,
            outcome,
        },
    )
    .await
}

/// A message is waiting to be sent to the customer on WhatsApp.
pub async fn emit_whatsapp_outbound_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
) -> sqlx::Result<()> {
    publish(
        tx,
        &ConversationEvent::WhatsappOutbound {
            tenant_id,
            conversation_id,
            message_id,
        },
    )
    .await
}
