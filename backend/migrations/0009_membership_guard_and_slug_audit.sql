CREATE OR REPLACE FUNCTION reject_membership_with_deleted_parent()
RETURNS TRIGGER AS $$
DECLARE
    parent_deleted_at TIMESTAMPTZ;
BEGIN
    SELECT deleted_at INTO parent_deleted_at FROM users WHERE id = NEW.user_id;
    IF parent_deleted_at IS NOT NULL THEN
        RAISE EXCEPTION 'cannot create active membership: user % is soft-deleted', NEW.user_id;
    END IF;

    SELECT deleted_at INTO parent_deleted_at FROM tenants WHERE id = NEW.tenant_id;
    IF parent_deleted_at IS NOT NULL THEN
        RAISE EXCEPTION 'cannot create active membership: tenant % is soft-deleted', NEW.tenant_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER membership_guard_deleted_parent
    BEFORE INSERT ON tenant_memberships
    FOR EACH ROW
    EXECUTE FUNCTION reject_membership_with_deleted_parent();

CREATE OR REPLACE FUNCTION audit_tenant_slug_change()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.slug <> OLD.slug THEN
        INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
        VALUES (
            NULL,
            'tenant.slug_changed',
            'tenant',
            NEW.id::text,
            NEW.id,
            jsonb_build_object(
                'old_slug', OLD.slug::text,
                'new_slug', NEW.slug::text
            )
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tenants_slug_change_audit
    AFTER UPDATE ON tenants
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.slug IS DISTINCT FROM OLD.slug)
    EXECUTE FUNCTION audit_tenant_slug_change();
