CREATE TABLE tenant_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT tenant_memberships_role_check CHECK (
        role IN ('owner', 'admin', 'manager', 'agent', 'viewer')
    )
);

CREATE UNIQUE INDEX tenant_memberships_tenant_user_active_uniq
    ON tenant_memberships (tenant_id, user_id)
    WHERE deleted_at IS NULL;

CREATE INDEX tenant_memberships_user_idx
    ON tenant_memberships (user_id);

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenant_memberships
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- Trigger function: cascade soft-delete to memberships when parent is soft-deleted
CREATE OR REPLACE FUNCTION cascade_soft_delete_memberships()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
        UPDATE tenant_memberships
        SET deleted_at = NEW.deleted_at
        WHERE deleted_at IS NULL
          AND (tenant_id = NEW.id OR user_id = NEW.id);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_soft_delete_cascade
    AFTER UPDATE ON users
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION cascade_soft_delete_memberships();

CREATE TRIGGER tenants_soft_delete_cascade
    AFTER UPDATE ON tenants
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION cascade_soft_delete_memberships();
