-- Migration 0074: Row-Level Security on the remaining tenant-owned tables (MOH-49, step 4c of
-- MOH-31; ADR-004 sections 2 and 7). Same rule and policy as 0072 and 0073. With this migration
-- every table that carries a tenant_id has RLS enabled and forced.
--
-- Three tables need a note:
--
-- * `outbox_events.tenant_id` is TEXT (events without a tenant exist), so its policy compares text
--   with text instead of casting to uuid. A tenant sees only its own events; the workers claim
--   across tenants through the audited `xtenant.outbox_*` functions (0070).
-- * `audit_logs.tenant_id` is nullable: a NULL row is a platform-level event. The strict policy
--   hides those rows from tenant queries and refuses an insert without a tenant. The three
--   tenant-less events the application writes outside a tenant (sign-in events through
--   `xtenant.record_auth_event`, 0068, and the two below) go through functions that accept a
--   fixed list of actions, and the platform reads every row through
--   `xtenant.platform_audit_entries` (0071).
-- * Three trigger functions write these tables on behalf of an UPDATE on `tenants` or `users`: the
--   two soft-delete cascades to `tenant_memberships` and the slug-change audit row. A cascade is
--   cross-tenant by nature (a user's memberships span tenants), and run with the caller's rights
--   the policy would silently skip the rows. They are replaced by SECURITY DEFINER
--   versions in the `xtenant` schema, owned by `app_platform` like the other cross-tenant functions,
--   and the triggers are pointed at them.
-- * `tenant_memberships`, `tenant_invitations`, `widget_*` and `integration_*` are read before a
--   tenant is known (sign-in, invitation links, widget embeds, webhooks). Those reads already go
--   through `xtenant.*` functions (0068), which run as `app_platform` and bypass the policy.

SET LOCAL ROLE app_platform;

-- Events the application records without a tenant: a refused tenant access (the caller named a
-- tenant it is not a member of, so no tenant context exists) and the changes to the global billing
-- plans. Each is an exact (action, resource type) pair; anything else is refused, so the function
-- cannot be used to write other rows with a NULL tenant.
CREATE FUNCTION xtenant.record_platform_event(
  p_action        text,
  p_actor_id      uuid,
  p_resource_type text,
  p_resource_id   text,
  p_details       jsonb
) RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  IF (p_action, p_resource_type) NOT IN (
       ('tenant.access_denied', 'tenant'),
       ('billing_plan.created', 'billing_plan'),
       ('billing_plan.status_changed', 'billing_plan')
     ) THEN
    RAISE EXCEPTION 'record_platform_event does not record % on %', p_action, p_resource_type;
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (p_actor_id, p_action, p_resource_type, p_resource_id, NULL, COALESCE(p_details, '{}'::jsonb));
END
$$;

REVOKE ALL ON FUNCTION xtenant.record_platform_event(text, uuid, text, text, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION xtenant.record_platform_event(text, uuid, text, text, jsonb) TO app_runtime;

-- Trigger functions, same bodies as the public ones they replace (0008, 0015) but run with the
-- rights of app_platform. They are not callable by app_runtime (they return trigger anyway);
-- app_owner needs EXECUTE only to create the triggers below.
CREATE FUNCTION xtenant.cascade_soft_delete_tenant_memberships()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
    UPDATE tenant_memberships
       SET deleted_at = NEW.deleted_at
     WHERE deleted_at IS NULL
       AND tenant_id = NEW.id;
  END IF;
  RETURN NEW;
END
$$;

CREATE FUNCTION xtenant.cascade_soft_delete_user_memberships()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
    UPDATE tenant_memberships
       SET deleted_at = NEW.deleted_at
     WHERE deleted_at IS NULL
       AND user_id = NEW.id;
  END IF;
  RETURN NEW;
END
$$;

CREATE FUNCTION xtenant.audit_tenant_slug_change()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
DECLARE
  actor_text TEXT;
  actor UUID;
BEGIN
  actor_text := current_setting('app.audit_actor_id', true);
  IF actor_text IS NULL OR actor_text = '' THEN
    RAISE EXCEPTION 'tenant.slug_changed requires an audit actor; call set_audit_actor() within the same transaction';
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
    jsonb_build_object('old_slug', OLD.slug::text, 'new_slug', NEW.slug::text)
  );
  RETURN NEW;
END
$$;

GRANT EXECUTE ON FUNCTION
  xtenant.cascade_soft_delete_tenant_memberships(),
  xtenant.cascade_soft_delete_user_memberships(),
  xtenant.audit_tenant_slug_change()
TO app_owner;

RESET ROLE;

DROP TRIGGER tenants_soft_delete_cascade ON tenants;
CREATE TRIGGER tenants_soft_delete_cascade
  AFTER UPDATE ON tenants
  FOR EACH ROW
  WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
  EXECUTE FUNCTION xtenant.cascade_soft_delete_tenant_memberships();

DROP TRIGGER users_soft_delete_cascade ON users;
CREATE TRIGGER users_soft_delete_cascade
  AFTER UPDATE ON users
  FOR EACH ROW
  WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
  EXECUTE FUNCTION xtenant.cascade_soft_delete_user_memberships();

DROP TRIGGER tenants_slug_change_audit ON tenants;
CREATE TRIGGER tenants_slug_change_audit
  AFTER UPDATE ON tenants
  FOR EACH ROW
  WHEN (OLD.deleted_at IS NULL AND NEW.slug IS DISTINCT FROM OLD.slug)
  EXECUTE FUNCTION xtenant.audit_tenant_slug_change();

DROP FUNCTION public.cascade_soft_delete_tenant_memberships();
DROP FUNCTION public.cascade_soft_delete_user_memberships();
DROP FUNCTION public.audit_tenant_slug_change();

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY[
    'skills',
    'agent_skills',
    'agent_availability',
    'tenant_memberships',
    'tenant_invitations',
    'audit_logs',
    'widget_instances',
    'widget_sessions',
    'notifications',
    'integration_connections',
    'integration_secrets',
    'integration_webhook_deliveries',
    'integration_events',
    'tenant_subscriptions',
    'tenant_rate_limit_overrides'
  ] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format(
      'CREATE POLICY tenant_isolation ON %I '
      'USING (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid) '
      'WITH CHECK (tenant_id = NULLIF(current_setting(''app.tenant_id'', true), '''')::uuid)',
      t);
  END LOOP;
END
$$;

ALTER TABLE outbox_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE outbox_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON outbox_events
  USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), ''))
  WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), ''));
