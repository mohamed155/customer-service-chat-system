-- Migration 0060: Platform analytics & health — cross-tenant, time-ranged read paths.
CREATE INDEX IF NOT EXISTS ai_usage_records_created_idx
    ON ai_usage_records (created_at DESC);

CREATE INDEX IF NOT EXISTS outbox_pending_created_idx
    ON outbox_events (created_at)
    WHERE processed_at IS NULL;

CREATE INDEX IF NOT EXISTS outbox_pending_attempts_idx
    ON outbox_events (attempts)
    WHERE processed_at IS NULL;

CREATE INDEX IF NOT EXISTS conversations_status_active_idx
    ON conversations (status)
    WHERE deleted_at IS NULL;
