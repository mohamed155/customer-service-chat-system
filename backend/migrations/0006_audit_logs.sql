CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NULL,
    tenant_id UUID NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    details JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT audit_logs_action_length CHECK (length(action) BETWEEN 1 AND 100)
);

CREATE INDEX audit_logs_tenant_created_idx
    ON audit_logs (tenant_id, created_at DESC);

CREATE INDEX audit_logs_created_idx
    ON audit_logs (created_at DESC);

-- Append-only enforcement: forbid UPDATE and DELETE on audit_logs
CREATE OR REPLACE FUNCTION forbid_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'audit_logs is append-only; UPDATE and DELETE are not permitted';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_logs_append_only
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW
    EXECUTE FUNCTION forbid_mutation();
