-- Migration 0048: AI Conversation Engine — generation trace records and
-- confidence scoring for AI responses.
-- See specs/021-ai-conversation-engine/ for the full design.

-- Table: ai_generations — one row per engine run (a claimed customer-message
-- trigger), regardless of how many provider attempts it made. This is the
-- inspectable Generation Record of FR-015 / SC-008.
CREATE TABLE ai_generations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE RESTRICT,
    trigger_message_id UUID NOT NULL REFERENCES messages(id) ON DELETE RESTRICT,
    response_message_id UUID NULL REFERENCES messages(id) ON DELETE SET NULL,
    usage_record_id UUID NULL REFERENCES ai_usage_records(id) ON DELETE SET NULL,
    provider TEXT NULL,
    model TEXT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('success','superseded','cancelled_escalation','failed','fallback')),
    error_category TEXT NULL,
    attempts SMALLINT NOT NULL DEFAULT 0,
    continuation_used BOOLEAN NOT NULL DEFAULT false,
    retrieval_chunk_count SMALLINT NOT NULL DEFAULT 0,
    retrieval_top_similarity REAL NULL,
    retrieval_degraded BOOLEAN NOT NULL DEFAULT false,
    confidence_score REAL NULL CHECK (confidence_score >= 0 AND confidence_score <= 1),
    latency_ms INTEGER NOT NULL,
    request_id TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- B-tree index for conversation-scoped generation inspection.
CREATE INDEX ai_generations_conversation_idx
    ON ai_generations (tenant_id, conversation_id, created_at DESC);

-- B-tree index for tenant-wide operational queries.
CREATE INDEX ai_generations_tenant_idx
    ON ai_generations (tenant_id, created_at DESC);

-- Column: messages.ai_confidence_score — stores the deterministic heuristic
-- confidence score for AI-generated messages. NULL for non-AI messages and
-- pre-021 AI messages.
ALTER TABLE messages
    ADD COLUMN ai_confidence_score REAL NULL CHECK (ai_confidence_score >= 0 AND ai_confidence_score <= 1);
