-- Migration 0052: indexes for analytics aggregation queries (spec 025).
-- No new tables: analytics reads existing conversation/message/feedback/usage data.

-- Drives every conversation-cohort scan (volume, rates, breakdown). Existing
-- conversation indexes lead on status/customer/last_activity_at, none on created_at.
CREATE INDEX conversations_tenant_created_idx
    ON conversations (tenant_id, created_at)
    WHERE deleted_at IS NULL;

-- Reverse join for attributing ai_usage_records to a conversation channel.
-- Existing ai_generations index leads on conversation_id, not usage_record_id.
CREATE INDEX ai_generations_tenant_usage_record_idx
    ON ai_generations (tenant_id, usage_record_id)
    WHERE usage_record_id IS NOT NULL;
