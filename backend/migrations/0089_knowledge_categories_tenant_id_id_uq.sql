-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS knowledge_categories_tenant_id_id_uq
    ON knowledge_categories (tenant_id, id);
