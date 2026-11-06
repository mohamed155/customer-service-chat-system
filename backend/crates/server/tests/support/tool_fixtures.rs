//! Fixtures for tool-request tests.
//!
//! `tool_requests.generation_id` references `ai_generations(id)`, which in
//! turn references the triggering customer message. Seed both, in order, so
//! tests get a real parent row instead of a dangling random id.
//!
//! Include with `#[path = "support/tool_fixtures.rs"] mod tool_fixtures;`

#![allow(dead_code)]

use uuid::Uuid;

/// Insert a customer trigger message and an `awaiting_tool_approval`
/// generation for it; returns the generation id.
pub async fn seed_generation(pool: &sqlx::PgPool, tenant_id: Uuid, conversation_id: Uuid) -> Uuid {
    let trigger_message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Please update my contact details') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .expect("seed trigger message");

    sqlx::query_scalar(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, \
         outcome, attempts, latency_ms) \
         VALUES ($1, $2, $3, 'awaiting_tool_approval', 1, 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(trigger_message_id)
    .fetch_one(pool)
    .await
    .expect("seed ai generation")
}
