-- no-transaction
-- The throttle in xtenant._audit (0067) looks up the latest audit row for one function and
-- resource. Partial, so it only covers the cross-tenant rows and costs the other audit writes nothing.
--
-- One CREATE INDEX CONCURRENTLY per file, as in 0060 and 0062-0064: audit_logs is populated and
-- append-only, and CONCURRENTLY neither blocks its writers nor can run in a transaction with
-- any other statement.
CREATE INDEX CONCURRENTLY IF NOT EXISTS audit_logs_cross_tenant_idx
  ON audit_logs (action, resource_id, created_at DESC)
  WHERE resource_type = 'cross_tenant_access';
