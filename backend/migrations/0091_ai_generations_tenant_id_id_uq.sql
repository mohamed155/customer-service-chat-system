-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ai_generations_tenant_id_id_uq
    ON ai_generations (tenant_id, id);
