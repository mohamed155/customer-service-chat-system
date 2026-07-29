-- Migration 0061: Tenant-level rate-limit overrides.
CREATE TABLE tenant_rate_limit_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    scope TEXT NOT NULL,
    max_requests INTEGER NOT NULL CHECK (max_requests > 0),
    window_seconds INTEGER NOT NULL CHECK (window_seconds > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT tenant_rate_limit_overrides_scope_check
        CHECK (scope IN ('login', 'widget_session', 'widget_message', 'webhook', 'tenant_global'))
);

CREATE UNIQUE INDEX tenant_rate_limit_overrides_active_idx
    ON tenant_rate_limit_overrides (tenant_id, scope)
    WHERE deleted_at IS NULL;

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenant_rate_limit_overrides
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
