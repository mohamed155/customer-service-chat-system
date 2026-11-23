-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS widget_sessions_tenant_id_id_uq
    ON widget_sessions (tenant_id, id);
