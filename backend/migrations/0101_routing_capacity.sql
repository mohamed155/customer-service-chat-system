-- MOH-104: tenant default and per-member routing capacity, plus fair assignment history.
CREATE TABLE routing_settings (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE RESTRICT,
    default_capacity INTEGER NOT NULL DEFAULT 10
        CONSTRAINT routing_settings_default_capacity_check
        CHECK (default_capacity BETWEEN 1 AND 200),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER set_routing_settings_updated_at
    BEFORE UPDATE ON routing_settings
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

ALTER TABLE agent_availability
    ADD COLUMN capacity_override INTEGER NULL,
    ADD COLUMN last_assigned_at TIMESTAMPTZ NULL,
    ADD CONSTRAINT agent_availability_capacity_override_check
        CHECK (capacity_override IS NULL OR capacity_override BETWEEN 0 AND 200);

ALTER TABLE routing_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE routing_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON routing_settings
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
