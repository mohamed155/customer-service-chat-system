DROP TRIGGER IF EXISTS tenants_slug_change_audit ON tenants;

CREATE OR REPLACE FUNCTION set_audit_actor(user_id UUID)
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.audit_actor_id', user_id::text, false);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION clear_audit_actor()
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.audit_actor_id', '', false);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION audit_tenant_slug_change()
RETURNS TRIGGER AS $$
DECLARE
    actor_text TEXT;
    actor UUID;
BEGIN
    actor_text := current_setting('app.audit_actor_id', true);
    IF actor_text IS NULL OR actor_text = '' THEN
        RAISE EXCEPTION 'tenant.slug_changed requires an audit actor; call set_audit_actor() before renaming';
    END IF;

    BEGIN
        actor := actor_text::uuid;
    EXCEPTION WHEN OTHERS THEN
        RAISE EXCEPTION 'app.audit_actor_id setting is not a valid UUID: %', actor_text;
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
