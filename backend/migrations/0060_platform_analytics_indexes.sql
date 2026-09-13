-- no-transaction
-- Migration 0060: Platform analytics & health — cross-tenant, time-ranged read paths.
-- Split into one CREATE INDEX CONCURRENTLY per file (0060, 0062-0064): Postgres
-- implicitly wraps a multi-statement message in a transaction even without an
-- explicit BEGIN, and CONCURRENTLY can never run inside one.
CREATE INDEX CONCURRENTLY IF NOT EXISTS ai_usage_records_created_idx
    ON ai_usage_records (created_at DESC);
