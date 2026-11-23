-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS integration_connections_tenant_id_id_uq
    ON integration_connections (tenant_id, id);
