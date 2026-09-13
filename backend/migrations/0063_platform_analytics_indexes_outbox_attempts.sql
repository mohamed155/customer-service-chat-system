-- no-transaction
-- Migration 0063: Platform analytics & health — outbox backlog by attempts (split from 0060).
CREATE INDEX CONCURRENTLY IF NOT EXISTS outbox_pending_attempts_idx
    ON outbox_events (attempts)
    WHERE processed_at IS NULL;
