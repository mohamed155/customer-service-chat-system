//! What other modules may ask `conversations` (MOH-194, ADR-017).
//!
//! `conversations` owns `conversations`, `messages` and `message_attachments`. The widget, the
//! escalation queue and the WhatsApp sender need a few facts about a conversation: which one a
//! visitor has open, who the customer is, what state it is in, and how busy a member is. They ask
//! here instead of naming those tables in their own SQL.
//!
//! Every function takes the tenant ID and filters on it, and runs on the executor the caller holds,
//! so it stays inside the caller's tenant Unit of Work. A deleted conversation is invisible unless
//! the function says otherwise.

use std::collections::HashMap;

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

/// A visitor's current conversation, as the widget shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenConversation {
    pub id: Uuid,
    pub status: String,
    pub escalated: bool,
    pub assigned: bool,
}

/// The customer's most recently active conversation that is not resolved or closed.
pub async fn open_conversation_of_customer<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<OpenConversation>> {
    let row: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id, status, escalated_at IS NOT NULL, assigned_membership_id IS NOT NULL \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 \
           AND status NOT IN ('resolved', 'closed') \
           AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(executor)
    .await?;
    Ok(
        row.map(|(id, status, escalated, assigned)| OpenConversation {
            id,
            status,
            escalated,
            assigned,
        }),
    )
}

/// The status of a conversation that belongs to this customer, locking the row until the caller's
/// transaction ends. `None` when there is no such live conversation for the customer, which is the
/// same answer for "does not exist" and "is somebody else's".
pub async fn lock_customers_conversation<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT status FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND customer_id = $3 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(customer_id)
    .fetch_optional(executor)
    .await
}

/// The customer a live conversation belongs to.
pub async fn customer_of_live_conversation<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT customer_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(executor)
    .await
}

/// The customer a conversation belongs to, deleted or not: an outbound message still goes to the
/// person it was written to.
pub async fn customer_of_conversation<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("SELECT customer_id FROM conversations WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(executor)
        .await
}

/// A live conversation's status and who it is assigned to.
pub async fn status_and_assignee<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<(String, Option<Uuid>)>> {
    sqlx::query_as(
        "SELECT status, assigned_membership_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(executor)
    .await
}

/// How a conversation is labelled in a queue: its channel and who the customer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCard {
    pub channel: String,
    pub customer_id: Uuid,
    pub customer_name: String,
}

/// The card of each of `conversation_ids` that exists in this tenant, with its customer. IDs of
/// another tenant's conversations and unknown IDs are left out.
pub async fn conversation_cards(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversation_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, ConversationCard>> {
    if conversation_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT id, channel, customer_id FROM conversations \
         WHERE tenant_id = $1 AND id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversation_ids)
    .fetch_all(&mut *conn)
    .await?;
    let customer_ids: Vec<Uuid> = rows
        .iter()
        .map(|(_, _, customer_id)| *customer_id)
        .collect();
    let names = customers::directory::display_names(&mut *conn, tenant_id, &customer_ids).await?;
    // A conversation whose customer is gone has no card, as an inner join would not return it.
    Ok(rows
        .into_iter()
        .filter_map(|(id, channel, customer_id)| {
            let customer_name = names.get(&customer_id)?.clone();
            Some((
                id,
                ConversationCard {
                    channel,
                    customer_id,
                    customer_name,
                },
            ))
        })
        .collect())
}

/// How many open or pending conversations each of `membership_ids` is assigned. Members with none
/// are absent from the map.
pub async fn open_load_by_member(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    membership_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, i64>> {
    if membership_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT assigned_membership_id, COUNT(*) FROM conversations \
         WHERE tenant_id = $1 AND assigned_membership_id = ANY($2) \
           AND status IN ('open', 'pending') AND deleted_at IS NULL \
         GROUP BY assigned_membership_id",
    )
    .bind(tenant_id)
    .bind(membership_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// The text of an outbound message (a reply or an AI answer) that is waiting to be sent.
pub async fn outbound_message_body<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    message_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND id = $2 AND kind IN ('reply', 'ai')",
    )
    .bind(tenant_id)
    .bind(message_id)
    .fetch_optional(executor)
    .await
}
