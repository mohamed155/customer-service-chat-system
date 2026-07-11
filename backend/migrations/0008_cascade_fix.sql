DROP TRIGGER IF EXISTS users_soft_delete_cascade ON users;
DROP TRIGGER IF EXISTS tenants_soft_delete_cascade ON tenants;
DROP FUNCTION IF EXISTS cascade_soft_delete_memberships();

CREATE OR REPLACE FUNCTION cascade_soft_delete_user_memberships()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
        UPDATE tenant_memberships
        SET deleted_at = NEW.deleted_at
        WHERE deleted_at IS NULL
          AND user_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION cascade_soft_delete_tenant_memberships()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
        UPDATE tenant_memberships
        SET deleted_at = NEW.deleted_at
        WHERE deleted_at IS NULL
          AND tenant_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_soft_delete_cascade
    AFTER UPDATE ON users
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION cascade_soft_delete_user_memberships();

CREATE TRIGGER tenants_soft_delete_cascade
    AFTER UPDATE ON tenants
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION cascade_soft_delete_tenant_memberships();
