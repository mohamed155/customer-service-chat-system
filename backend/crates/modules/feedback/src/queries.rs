use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::model::ConversationFeedbackRow;

pub async fn find_conversation_for_session(
    pool: &PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<(String, String, Option<Uuid>)>> {
    sqlx::query_as::<_, (String, String, Option<Uuid>)>(
        "SELECT status, channel, assigned_membership_id \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND id = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}

pub async fn insert_feedback(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    widget_session_id: Option<Uuid>,
    channel: &str,
    agent_configuration_id: Option<Uuid>,
    assigned_membership_id: Option<Uuid>,
    rating: i16,
    comment: Option<&str>,
) -> sqlx::Result<Option<ConversationFeedbackRow>> {
    sqlx::query_as::<_, ConversationFeedbackRow>(
        "INSERT INTO conversation_feedback \
         (tenant_id, conversation_id, widget_session_id, channel, \
          agent_configuration_id, assigned_membership_id, rating, comment) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (tenant_id, conversation_id) DO NOTHING \
         RETURNING id, tenant_id, conversation_id, widget_session_id, channel, \
                   agent_configuration_id, assigned_membership_id, rating, comment, \
                   submitted_at, created_at",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(widget_session_id)
    .bind(channel)
    .bind(agent_configuration_id)
    .bind(assigned_membership_id)
    .bind(rating)
    .bind(comment)
    .fetch_optional(pool)
    .await
}

pub async fn find_feedback_by_conversation(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<ConversationFeedbackRow>> {
    sqlx::query_as::<_, ConversationFeedbackRow>(
        "SELECT id, tenant_id, conversation_id, widget_session_id, channel, \
                agent_configuration_id, assigned_membership_id, rating, comment, \
                submitted_at, created_at \
         FROM conversation_feedback \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}

pub async fn find_pending_feedback(
    pool: &PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<(Uuid, DateTime<Utc>)>> {
    sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        "SELECT c.id, c.last_activity_at \
         FROM conversations c \
         LEFT JOIN conversation_feedback f \
           ON f.conversation_id = c.id AND f.tenant_id = c.tenant_id \
         WHERE c.tenant_id = $1 \
           AND c.customer_id = $2 \
           AND c.status IN ('resolved', 'closed') \
           AND c.deleted_at IS NULL \
           AND f.id IS NULL \
         ORDER BY c.last_activity_at DESC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(pool)
    .await
}

pub async fn feedback_summary(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<(Option<f64>, i64)> {
    sqlx::query_as::<_, (Option<f64>, i64)>(
        "SELECT AVG(rating)::float8, COUNT(*)::bigint \
         FROM conversation_feedback \
         WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

pub async fn resolve_ai_agent_configuration(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT ac.id FROM agent_configurations ac \
         WHERE ac.tenant_id = $1 \
           AND ac.status = 'live' \
           AND ac.deleted_at IS NULL \
           AND EXISTS (SELECT 1 FROM ai_generations g \
                       WHERE g.tenant_id = $1 AND g.conversation_id = $2) \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}
