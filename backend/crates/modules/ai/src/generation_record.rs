use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationOutcome {
    Success,
    Superseded,
    CancelledEscalation,
    Failed,
    Fallback,
    AwaitingToolApproval,
}

impl GenerationOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Superseded => "superseded",
            Self::CancelledEscalation => "cancelled_escalation",
            Self::Failed => "failed",
            Self::Fallback => "fallback",
            Self::AwaitingToolApproval => "awaiting_tool_approval",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerationRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub trigger_message_id: Uuid,
    pub response_message_id: Option<Uuid>,
    pub usage_record_id: Option<Uuid>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub outcome: GenerationOutcome,
    pub error_category: Option<String>,
    pub attempts: i16,
    pub continuation_used: bool,
    pub retrieval_chunk_count: i16,
    pub retrieval_top_similarity: Option<f32>,
    pub retrieval_degraded: bool,
    pub confidence_score: Option<f32>,
    pub latency_ms: i32,
    pub request_id: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Write a provisional generation row before tool requests reference it
/// (`tool_requests.generation_id` is a foreign key). The final `insert` for
/// the same id completes it. No-op if the row already exists.
pub async fn ensure_provisional(
    pool: &PgPool,
    id: Uuid,
    tenant_id: Uuid,
    conversation_id: Uuid,
    trigger_message_id: Uuid,
) -> sqlx::Result<()> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    sqlx::query(
        "INSERT INTO ai_generations \
         (id, tenant_id, conversation_id, trigger_message_id, outcome, attempts, latency_ms) \
         VALUES ($1, $2, $3, $4, 'awaiting_tool_approval', 1, 0) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(trigger_message_id)
    .execute(uow.conn())
    .await?;
    Ok(())
}

/// Insert the generation record, or complete the provisional row written by
/// [`ensure_provisional`] for the same id.
pub async fn insert(pool: &PgPool, rec: &GenerationRecord) -> sqlx::Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO ai_generations
           (id, tenant_id, conversation_id, trigger_message_id, response_message_id,
            usage_record_id, provider, model, outcome, error_category, attempts,
            continuation_used, retrieval_chunk_count, retrieval_top_similarity,
            retrieval_degraded, confidence_score, latency_ms, request_id, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
           ON CONFLICT (id) DO UPDATE SET
             response_message_id = EXCLUDED.response_message_id,
             usage_record_id = EXCLUDED.usage_record_id,
             provider = EXCLUDED.provider,
             model = EXCLUDED.model,
             outcome = EXCLUDED.outcome,
             error_category = EXCLUDED.error_category,
             attempts = EXCLUDED.attempts,
             continuation_used = EXCLUDED.continuation_used,
             retrieval_chunk_count = EXCLUDED.retrieval_chunk_count,
             retrieval_top_similarity = EXCLUDED.retrieval_top_similarity,
             retrieval_degraded = EXCLUDED.retrieval_degraded,
             confidence_score = EXCLUDED.confidence_score,
             latency_ms = EXCLUDED.latency_ms,
             request_id = EXCLUDED.request_id
           RETURNING id"#,
    )
    .bind(rec.id)
    .bind(rec.tenant_id)
    .bind(rec.conversation_id)
    .bind(rec.trigger_message_id)
    .bind(rec.response_message_id)
    .bind(rec.usage_record_id)
    .bind(&rec.provider)
    .bind(&rec.model)
    .bind(rec.outcome.as_str())
    .bind(&rec.error_category)
    .bind(rec.attempts)
    .bind(rec.continuation_used)
    .bind(rec.retrieval_chunk_count)
    .bind(rec.retrieval_top_similarity)
    .bind(rec.retrieval_degraded)
    .bind(rec.confidence_score)
    .bind(rec.latency_ms)
    .bind(&rec.request_id)
    .bind(rec.created_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_as_str_matches_check_values() {
        assert_eq!(GenerationOutcome::Success.as_str(), "success");
        assert_eq!(GenerationOutcome::Superseded.as_str(), "superseded");
        assert_eq!(
            GenerationOutcome::CancelledEscalation.as_str(),
            "cancelled_escalation"
        );
        assert_eq!(GenerationOutcome::Failed.as_str(), "failed");
        assert_eq!(GenerationOutcome::Fallback.as_str(), "fallback");
        assert_eq!(
            GenerationOutcome::AwaitingToolApproval.as_str(),
            "awaiting_tool_approval"
        );
    }
}
