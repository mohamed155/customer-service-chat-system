-- Migration 0068: cross-tenant functions for the lookups that happen before a tenant is known
-- (MOH-46, part 1: MOH-53). Conventions are in 0067.
--
-- Each one resolves a credential (a user id from a session, an invitation token, a widget public
-- id or session token, a webhook token) to the rows it entitles the caller to, and returns only
-- the columns its caller uses. A hit writes an audit row (action `xtenant.<function>`, resource
-- `cross_tenant_access`, tenant and resource id of what was resolved) at most once an hour per
-- resource, so repeating a call cannot grow audit_logs; a miss writes nothing.

SET LOCAL ROLE app_platform;

-- GET /me: the memberships of one user across tenants.
CREATE FUNCTION xtenant.user_memberships(p_user_id uuid)
RETURNS TABLE (tenant_id uuid, name text, slug text, role text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
BEGIN
  RETURN QUERY
    SELECT tm.tenant_id, t.name, t.slug::text, tm.role
    FROM tenant_memberships tm
    JOIN tenants t ON t.id = tm.tenant_id
    WHERE tm.user_id = p_user_id
      AND tm.deleted_at IS NULL
      AND t.deleted_at IS NULL;
  IF FOUND THEN
    PERFORM xtenant._audit('user_memberships', NULL, p_user_id, p_user_id::text, '{}'::jsonb, interval '1 hour');
  END IF;
END
$$;

-- Sign-in and sign-out events have no tenant. The function is the audit write itself, limited to
-- `auth.*` actions so it cannot be used to write other audit rows without a tenant.
CREATE FUNCTION xtenant.record_auth_event(
  p_action      text,
  p_actor_id    uuid,
  p_resource_id text,
  p_details     jsonb
) RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
BEGIN
  IF p_action IS NULL OR p_action NOT LIKE 'auth.%' THEN
    RAISE EXCEPTION 'record_auth_event only records auth.* actions';
  END IF;
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (p_actor_id, p_action, 'user', p_resource_id, NULL, COALESCE(p_details, '{}'::jsonb));
END
$$;

-- Invitation preview and accept: the token identifies the invitation, which identifies the tenant.
-- The caller continues in that tenant's Unit of Work. A soft-deleted tenant is a miss.
CREATE FUNCTION xtenant.invitation_by_token(p_token_hash text)
RETURNS TABLE (
  id            uuid,
  tenant_id     uuid,
  email         text,
  role          text,
  status        text,
  expires_at    timestamptz,
  accepted_at   timestamptz,
  tenant_name   text,
  tenant_status text
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT ti.id, ti.tenant_id, ti.email::text AS email, ti.role, ti.status, ti.expires_at,
         ti.accepted_at, t.name AS tenant_name, t.status AS tenant_status
    INTO r
    FROM tenant_invitations ti
    JOIN tenants t ON t.id = ti.tenant_id AND t.deleted_at IS NULL
   WHERE ti.token_hash = p_token_hash;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('invitation_by_token', r.tenant_id, NULL, r.id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.id, r.tenant_id, r.email, r.role, r.status, r.expires_at, r.accepted_at,
                      r.tenant_name, r.tenant_status;
END
$$;

-- Public widget: instance by its public id (the embed snippet carries it).
CREATE FUNCTION xtenant.widget_instance_by_public_id(p_public_id text)
RETURNS TABLE (
  id               uuid,
  tenant_id        uuid,
  public_id        text,
  name             text,
  display_name     text,
  primary_color    text,
  welcome_message  text,
  "position"       text,
  theme            text,
  enabled          boolean,
  allowed_domains  text[],
  created_at       timestamptz,
  updated_at       timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT wi.id, wi.tenant_id, wi.public_id, wi.name, wi.display_name, wi.primary_color,
         wi.welcome_message, wi."position", wi.theme, wi.enabled, wi.allowed_domains,
         wi.created_at, wi.updated_at
    INTO r
    FROM widget_instances wi
   WHERE wi.public_id = p_public_id AND wi.deleted_at IS NULL;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('widget_instance_by_public_id', r.tenant_id, NULL, r.id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.id, r.tenant_id, r.public_id, r.name, r.display_name, r.primary_color,
                      r.welcome_message, r."position", r.theme, r.enabled, r.allowed_domains,
                      r.created_at, r.updated_at;
END
$$;

-- Public widget: a live session by the hash of its bearer token.
CREATE FUNCTION xtenant.widget_session_by_token(p_token_hash bytea)
RETURNS TABLE (
  id                 uuid,
  tenant_id          uuid,
  widget_instance_id uuid,
  token_hash         bytea,
  customer_id        uuid,
  expires_at         timestamptz,
  created_at         timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT ws.id, ws.tenant_id, ws.widget_instance_id, ws.token_hash, ws.customer_id,
         ws.expires_at, ws.created_at
    INTO r
    FROM widget_sessions ws
   WHERE ws.token_hash = p_token_hash AND ws.expires_at > now();
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('widget_session_by_token', r.tenant_id, NULL, r.id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.id, r.tenant_id, r.widget_instance_id, r.token_hash, r.customer_id,
                      r.expires_at, r.created_at;
END
$$;

-- Inbound webhooks (integrations, WhatsApp): the URL token identifies the connection, which
-- identifies the tenant. Unknown and inactive stay distinguishable to the caller, which maps both
-- to the same response.
CREATE FUNCTION xtenant.webhook_connection_by_token(p_token_hash bytea)
RETURNS TABLE (connection_id uuid, tenant_id uuid, catalog_slug text, is_active boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT c.id AS connection_id, c.tenant_id, cat.slug::text AS catalog_slug, c.is_active
    INTO r
    FROM integration_connections c
    JOIN integration_catalog cat ON cat.id = c.catalog_id
   WHERE c.webhook_token_hash = p_token_hash;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('webhook_connection_by_token', r.tenant_id, NULL, r.connection_id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.connection_id, r.tenant_id, r.catalog_slug, r.is_active;
END
$$;

CREATE FUNCTION xtenant.webhook_connection_by_token_and_slug(p_token_hash bytea, p_slug text)
RETURNS TABLE (id uuid, tenant_id uuid, catalog_id uuid, is_active boolean, config jsonb)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
#variable_conflict use_column
DECLARE
  r record;
BEGIN
  SELECT c.id, c.tenant_id, c.catalog_id, c.is_active, c.config
    INTO r
    FROM integration_connections c
    JOIN integration_catalog cat ON cat.id = c.catalog_id
   WHERE c.webhook_token_hash = p_token_hash AND cat.slug::text = p_slug;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  PERFORM xtenant._audit('webhook_connection_by_token_and_slug', r.tenant_id, NULL, r.id::text,
                         '{}'::jsonb, interval '1 hour');
  RETURN QUERY SELECT r.id, r.tenant_id, r.catalog_id, r.is_active, r.config;
END
$$;

-- Only app_runtime may call them. Granted as the owner (app_platform): app_owner has no grant
-- option on these functions, so the same statements run as app_owner would silently do nothing.
REVOKE ALL ON FUNCTION
  xtenant.user_memberships(uuid),
  xtenant.record_auth_event(text, uuid, text, jsonb),
  xtenant.invitation_by_token(text),
  xtenant.widget_instance_by_public_id(text),
  xtenant.widget_session_by_token(bytea),
  xtenant.webhook_connection_by_token(bytea),
  xtenant.webhook_connection_by_token_and_slug(bytea, text)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  xtenant.user_memberships(uuid),
  xtenant.record_auth_event(text, uuid, text, jsonb),
  xtenant.invitation_by_token(text),
  xtenant.widget_instance_by_public_id(text),
  xtenant.widget_session_by_token(bytea),
  xtenant.webhook_connection_by_token(bytea),
  xtenant.webhook_connection_by_token_and_slug(bytea, text)
TO app_runtime;

RESET ROLE;
