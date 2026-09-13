-- no-transaction
-- Migration 0064: Platform analytics & health — active conversations by status (split from 0060).
CREATE INDEX CONCURRENTLY IF NOT EXISTS conversations_status_active_idx
    ON conversations (status)
    WHERE deleted_at IS NULL;
