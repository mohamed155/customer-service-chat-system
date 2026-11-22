//! What other modules read and write on conversations and messages.
//!
//! The use cases that carry rules (adding a message, assigning, opening a conversation, the detail
//! and the timeline) run the application services on the caller's transaction. The rest are
//! single statements the AI responder, the WhatsApp sender, the widget and escalations call; they
//! run in the Unit of Work the caller holds or open one for the tenant.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use crate::application::records::ConversationRecord as ConversationRow;
pub use crate::application::ConversationActor;
pub use crate::infrastructure::messages::{
    conversation_ai_state, customer_display_name, customer_id_for_conversation, has_ai_reply_since,
    has_customer_message_after, has_system_message, insert_ai_reply_in_tx, insert_auto_ack_in_tx,
    insert_citations_in_tx, insert_fallback_in_tx, last_customer_message_at,
    load_attachments_for_messages, load_citations_for_messages, message_body, recent_history,
    set_ai_handling_in_tx, set_escalated_in_tx, summary_history,
};

use crate::application::model::{ConversationDetail, ConversationStatusRef, Message};
use crate::application::ports::ConversationStore;
use crate::application::{service, ConversationError};
use crate::domain::MessageKind;
use crate::infrastructure::{PgEnrichment, PgUnit};

/// What a use case failed with, in the store's own words: a database error stays the database
/// error, and the rules' refusals keep the messages callers have always seen.
fn into_sqlx(error: ConversationError) -> sqlx::Error {
    match error {
        ConversationError::NotFound => sqlx::Error::RowNotFound,
        ConversationError::Store(error) => error
            .downcast::<sqlx::Error>()
            .unwrap_or_else(|other| sqlx::Error::Protocol(other.to_string())),
        refusal => sqlx::Error::Protocol(refusal.to_string()),
    }
}

/// A live conversation of the tenant, or `None` when it is missing or deleted.
pub async fn conversation_row_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<ConversationRow>> {
    PgUnit::new(tx)
        .find_live(tenant_id, id)
        .await
        .map_err(|error| into_sqlx(error.into()))
}

/// A conversation's full detail including customer, assignee, last
/// message preview, and participants, scoped to `(tenant_id, id)`.
/// Returns `None` when the conversation is missing or soft-deleted.
pub async fn detail_query_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<ConversationDetail>> {
    service::detail(&mut PgUnit::new(tx), tenant_id, id)
        .await
        .map_err(into_sqlx)
}

/// A keyset-paginated page of messages for a conversation, newest first, with citations,
/// attachments and delivery state. Returns the page, whether more follow, and the cursor to ask
/// for them.
pub async fn timeline_query_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    cursor: Option<String>,
    limit: i64,
) -> sqlx::Result<(Vec<Message>, bool, Option<String>)> {
    let page = service::timeline(
        &mut PgUnit::new(tx),
        &PgEnrichment::new(pool),
        tenant_id,
        conversation_id,
        cursor.as_deref(),
        limit,
    )
    .await
    .map_err(into_sqlx)?;
    Ok((page.messages, page.has_more, page.next_cursor))
}

/// Inserts a message into the conversation, marks it active and reopens it when the kind and the
/// status call for it. Returns the message and the conversation's refreshed status.
#[allow(clippy::too_many_arguments)]
pub async fn add_message_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    kind: &str,
    body: &str,
    sender_membership_id: Option<Uuid>,
    logged_by_membership_id: Option<Uuid>,
    actor: ConversationActor,
) -> sqlx::Result<(Message, ConversationStatusRef)> {
    let kind = MessageKind::parse(kind)
        .ok_or_else(|| sqlx::Error::Protocol(format!("Unknown message kind '{kind}'")))?;
    let added = service::add_message(
        &mut PgUnit::new(tx),
        tenant_id,
        conversation_id,
        kind,
        body,
        sender_membership_id,
        logged_by_membership_id,
        actor,
    )
    .await
    .map_err(into_sqlx)?;
    Ok((added.message, added.conversation))
}

/// Assigns (or with `None` unassigns) a conversation's member. Validates the target is an active
/// member, writes the audit record and the outbox events. Does not lock the row: the caller holds
/// `SELECT … FOR UPDATE` if it needs the assignment to be exclusive. Used by the escalation
/// routing (origin `escalations`) and the tools.
pub async fn assign_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    assigned_membership_id: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    origin: &str,
) -> sqlx::Result<()> {
    service::assign(
        &mut PgUnit::new(tx),
        tenant_id,
        conversation_id,
        assigned_membership_id,
        actor_user_id,
        origin,
    )
    .await
    .map_err(into_sqlx)
}

/// Opens a conversation for a customer of the tenant, with a first reply when `body` is not
/// empty, and returns its detail. Fails when the customer is not in the tenant.
pub async fn create_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    body: &str,
    actor: ConversationActor,
    widget_instance_id: Option<Uuid>,
) -> sqlx::Result<ConversationDetail> {
    service::create(
        &mut PgUnit::new(tx),
        tenant_id,
        customer_id,
        channel,
        body,
        actor,
        widget_instance_id,
    )
    .await
    .map_err(into_sqlx)
}
