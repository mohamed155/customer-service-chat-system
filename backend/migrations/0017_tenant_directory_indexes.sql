-- Indexes to accelerate the tenant directory query pattern used by
-- list_tenants (ILIKE on name/slug, equality on status, cursor pagination on
-- id, filtered by deleted_at IS NULL).

-- Enable pg_trgm for GIN trigram index on name/slug (ILIKE support).
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Composite B-tree index for the common filtered-query case:
--   WHERE deleted_at IS NULL AND status = $1 AND id > $2
CREATE INDEX IF NOT EXISTS idx_tenants_directory_filter
    ON tenants (deleted_at, status, id)
    WHERE deleted_at IS NULL;

-- GIN trigram index for ILIKE search on name and slug:
--   WHERE (name ILIKE $1 OR slug ILIKE $2)
CREATE INDEX IF NOT EXISTS idx_tenants_directory_search
    ON tenants USING gin (name gin_trgm_ops, slug gin_trgm_ops)
    WHERE deleted_at IS NULL;
