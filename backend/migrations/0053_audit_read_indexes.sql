-- Supports the actor_id filter on audit list endpoints (spec 026).
-- Existing indexes audit_logs_tenant_created_idx and audit_logs_created_idx
-- already cover the tenant and platform listing paths.
CREATE INDEX audit_logs_actor_created_idx
    ON audit_logs (actor_user_id, created_at DESC);
