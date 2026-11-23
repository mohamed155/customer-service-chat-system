-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS tenant_tools_tenant_id_id_uq
    ON tenant_tools (tenant_id, id);
