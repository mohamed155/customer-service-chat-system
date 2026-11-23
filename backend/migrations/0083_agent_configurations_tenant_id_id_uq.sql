-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS agent_configurations_tenant_id_id_uq
    ON agent_configurations (tenant_id, id);
