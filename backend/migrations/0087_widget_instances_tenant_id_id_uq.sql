-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS widget_instances_tenant_id_id_uq
    ON widget_instances (tenant_id, id);
