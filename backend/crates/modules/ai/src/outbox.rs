//! The events the AI engine publishes for other modules (MOH-195, ADR-001 section 2.3, ADR-017).
//!
//! `ai` says what happened; the modules that care decide what to do about it. `notifications`
//! turns these two into notifications for the right people, with its own wording and dedupe rules,
//! so `ai` no longer calls it. Both are written to the outbox like every other module's events
//! (`conversations::outbox`), in the tenant's own Unit of Work. Publishing is best effort: a
//! failure is logged and never fails the generation it describes.
//!
//! The payload is JSON with camelCase keys. Consumers read it by name, not by importing this
//! crate: the event type and the keys below are the contract.

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// A tool call is waiting for a person to approve it. Aggregate: the tool request.
/// Payload: `tenantId`, `toolRequestId`, `conversationId`.
pub const TOOL_APPROVAL_REQUESTED: &str = "ai.tool_approval_requested";

/// The AI could not answer a conversation. Aggregate: the conversation.
/// Payload: `tenantId`, `conversationId`, `failedAt` (RFC 3339).
pub const RESPONSE_FAILED: &str = "ai.response_failed";

/// Publishes `ai.tool_approval_requested` for a tool request that is awaiting approval.
pub async fn emit_tool_approval_requested(
    pool: &PgPool,
    tenant_id: Uuid,
    tool_request_id: Uuid,
    conversation_id: Uuid,
) {
    publish(
        pool,
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
    .await;
}

/// Publishes `ai.response_failed` for a conversation the AI failed to answer, at `failed_at`.
pub async fn emit_response_failed(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    failed_at: DateTime<Utc>,
) {
    publish(
        pool,
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
    .await;
}

async fn publish(
    pool: &PgPool,
    tenant_id: Uuid,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) {
    let result: sqlx::Result<()> = async {
        let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
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
        .execute(uow.conn())
        .await?;
        uow.commit().await
    }
    .await;
    if let Err(error) = result {
        tracing::error!(%error, event_type, "failed to publish an AI event");
    }
}
