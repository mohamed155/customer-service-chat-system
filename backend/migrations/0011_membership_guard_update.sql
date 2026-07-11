DROP TRIGGER IF EXISTS membership_guard_deleted_parent ON tenant_memberships;

CREATE OR REPLACE FUNCTION reject_membership_with_deleted_parent()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM 1 FROM users WHERE id = NEW.user_id FOR UPDATE;
    PERFORM 1 FROM tenants WHERE id = NEW.tenant_id FOR UPDATE;

    IF EXISTS (SELECT 1 FROM users WHERE id = NEW.user_id AND deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'cannot create or modify active membership: user % is soft-deleted', NEW.user_id;
    END IF;

    IF EXISTS (SELECT 1 FROM tenants WHERE id = NEW.tenant_id AND deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'cannot create or modify active membership: tenant % is soft-deleted', NEW.tenant_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER membership_guard_deleted_parent
    BEFORE INSERT OR UPDATE OF user_id, tenant_id, deleted_at ON tenant_memberships
    FOR EACH ROW
    WHEN (NEW.deleted_at IS NULL)
    EXECUTE FUNCTION reject_membership_with_deleted_parent();
