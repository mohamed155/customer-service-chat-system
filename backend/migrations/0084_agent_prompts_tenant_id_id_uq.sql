-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS agent_prompts_tenant_id_id_uq
    ON agent_prompts (tenant_id, id);
