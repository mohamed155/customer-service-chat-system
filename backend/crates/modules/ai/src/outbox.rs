//! The events the AI engine publishes for other modules (MOH-195, ADR-001 section 2.3, ADR-017).
//!
//! `ai` says what happened; the modules that care decide what to do about it. `notifications`
//! turns these two into notifications for the right people, with its own wording and dedupe rules,
//! so `ai` no longer calls it. Both are written to the outbox like every other module's events
//! (`conversations::outbox`). `ai.tool_approval_requested` is written in the caller's transaction,
//! with the tool request it describes. `ai.response_failed` is best effort, in its own: a failure to
//! publish is logged and never fails the generation it describes.
//!
//! The payload is JSON with camelCase keys. Consumers read it by name, not by importing this
//! crate: the event type and the keys below are the contract.

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A tool call is waiting for a person to approve it. Aggregate: the tool request.
/// Payload: `tenantId`, `toolRequestId`, `conversationId`.
pub const TOOL_APPROVAL_REQUESTED: &str = "ai.tool_approval_requested";

/// The AI could not answer a conversation. Aggregate: the conversation.
/// Payload: `tenantId`, `conversationId`, `failedAt` (RFC 3339).
pub const RESPONSE_FAILED: &str = "ai.response_failed";

/// Publishes `ai.tool_approval_requested` in the caller's transaction, so the event commits with
/// the tool request it describes. It must: the approval notification depends on the event, and a
/// request that is committed without it would wait for its approver in silence until it expires.
pub async fn emit_tool_approval_requested_in_tx(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    tool_request_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<()> {
    insert_event(
        conn,
        tenant_id,
        "tool_request",
        tool_request_id,
        TOOL_APPROVAL_REQUESTED,
        json!({
            "tenantId": tenant_id,
            "toolRequestId": tool_request_id,
            "conversationId": conversation_id,
        }),
    )
    .await
}

/// Publishes `ai.response_failed` for a conversation the AI failed to answer, at `failed_at`.
///
/// Best effort, in its own transaction: it describes a failure that has already happened and
/// changes nothing that has to stay consistent with it, so a failure to publish is logged and never
/// fails the generation.
pub async fn emit_response_failed(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    failed_at: DateTime<Utc>,
) {
    let result: sqlx::Result<()> = async {
        let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
        insert_event(
            uow.conn(),
            tenant_id,
            "conversation",
            conversation_id,
            RESPONSE_FAILED,
            json!({
                "tenantId": tenant_id,
                "conversationId": conversation_id,
                "failedAt": failed_at.to_rfc3339(),
            }),
        )
        .await?;
        uow.commit().await
    }
    .await;
    if let Err(error) = result {
        tracing::error!(%error, event_type = RESPONSE_FAILED, "failed to publish an AI event");
    }
}

async fn insert_event(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO outbox_events \
         (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(kernel::ids::new_id())
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(tenant_id)
    .bind(event_type)
    .bind(payload)
    .execute(conn)
    .await?;
    Ok(())
}
