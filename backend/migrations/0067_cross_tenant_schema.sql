-- Migration 0067: schema and plumbing for the audited cross-tenant path (MOH-46, part 1: MOH-53).
--
-- ADR-004 section 2: work that has no tenant yet, or spans tenants, must not turn Row-Level
-- Security off for the application. Each such job gets one narrow SECURITY DEFINER function in
-- the `xtenant` schema. The application role (app_runtime) may execute them and nothing else:
-- it still cannot read the underlying tables without a tenant.
--
-- The functions are owned by app_platform (NOLOGIN, BYPASSRLS; created by
-- infra/postgres/init/02-roles.sh), not by app_owner. FORCE ROW LEVEL SECURITY applies policies
-- to the table owner too and app_owner is NOBYPASSRLS, so a function owned by app_owner would
-- return no rows as soon as a table's policy is on. Later migrations create their functions the
-- same way as below: SET LOCAL ROLE app_platform, CREATE FUNCTION, RESET ROLE.
--
-- Rules every function follows (a catalog test checks the mechanical ones):
--   - SECURITY DEFINER with SET search_path = pg_catalog, public, pg_temp (pg_temp last);
--   - EXECUTE revoked from PUBLIC and granted to app_runtime;
--   - takes only the arguments it needs and returns only the columns its caller uses;
--   - writes an audit row (xtenant._audit) when it returns or changes something. An empty result
--     (an unknown token, an idle poll) writes nothing: unauthenticated callers can reach the
--     lookups, and an audit row per miss would let anyone flood audit_logs.
--
-- No Row-Level Security policy is added here, so nothing the application can see changes.

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'app_platform') THEN
    RAISE EXCEPTION 'role app_platform is missing; run infra/postgres/init/02-roles.sh first';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'app_platform'
                   AND rolbypassrls AND NOT rolsuper AND NOT rolcanlogin) THEN
    RAISE EXCEPTION 'role app_platform must be NOLOGIN BYPASSRLS and not a superuser; run infra/postgres/init/02-roles.sh';
  END IF;
  IF NOT pg_has_role(current_user, 'app_platform', 'SET') THEN
    RAISE EXCEPTION '% cannot SET ROLE app_platform; run infra/postgres/init/02-roles.sh', current_user;
  END IF;
END
$$;

CREATE SCHEMA IF NOT EXISTS xtenant;
REVOKE ALL ON SCHEMA xtenant FROM PUBLIC;
GRANT USAGE ON SCHEMA xtenant TO app_runtime;
GRANT USAGE, CREATE ON SCHEMA xtenant TO app_platform;

-- What the functions can touch: DML on every table, including tables later migrations create.
-- The role cannot log in, so only functions it owns ever use these privileges.
GRANT USAGE ON SCHEMA public TO app_platform;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO app_platform;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO app_platform;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_platform;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO app_platform;

SET LOCAL ROLE app_platform;

-- Functions app_platform creates are not executable by PUBLIC (Postgres grants that by default;
-- only a database-wide default privilege removes it, a per-schema one does not). Run as
-- app_platform itself: app_owner may not change another role's default privileges.
ALTER DEFAULT PRIVILEGES REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;

-- Internal helper: the one place a cross-tenant function writes its audit row. Not granted to
-- app_runtime, so the application cannot forge these rows; only the other functions in this
-- schema (same owner) call it.
CREATE FUNCTION xtenant._audit(
  p_function    text,
  p_tenant_id   uuid,
  p_actor_id    uuid,
  p_resource_id text,
  p_details     jsonb DEFAULT '{}'::jsonb
) RETURNS void
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog, public, pg_temp
AS $$
  INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details)
  VALUES (p_actor_id, 'xtenant.' || p_function, 'cross_tenant_access', p_resource_id, p_tenant_id,
          COALESCE(p_details, '{}'::jsonb));
$$;

RESET ROLE;
