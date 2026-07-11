CREATE OR REPLACE FUNCTION set_audit_actor(user_id UUID)
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.audit_actor_id', user_id::text, true);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION clear_audit_actor()
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.audit_actor_id', '', true);
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS tenants_slug_change_audit ON tenants;

CREATE OR REPLACE FUNCTION audit_tenant_slug_change()
RETURNS TRIGGER AS $$
DECLARE
    actor UUID;
BEGIN
    BEGIN
        actor := current_setting('app.audit_actor_id', true)::uuid;
    EXCEPTION WHEN OTHERS THEN
        actor := NULL;
    END;

    INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
    VALUES (
        actor,
        'tenant.slug_changed',
        'tenant',
        NEW.id::text,
        NEW.id,
        jsonb_build_object(
            'old_slug', OLD.slug::text,
            'new_slug', NEW.slug::text
        )
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tenants_slug_change_audit
    AFTER UPDATE ON tenants
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.slug IS DISTINCT FROM OLD.slug)
    EXECUTE FUNCTION audit_tenant_slug_change();
