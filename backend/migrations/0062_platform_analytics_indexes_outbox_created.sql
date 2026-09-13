-- no-transaction
-- Migration 0062: Platform analytics & health — outbox backlog by age (split from 0060).
CREATE INDEX CONCURRENTLY IF NOT EXISTS outbox_pending_created_idx
    ON outbox_events (created_at)
    WHERE processed_at IS NULL;
