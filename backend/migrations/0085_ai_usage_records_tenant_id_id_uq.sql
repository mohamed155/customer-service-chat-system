-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ai_usage_records_tenant_id_id_uq
    ON ai_usage_records (tenant_id, id);
