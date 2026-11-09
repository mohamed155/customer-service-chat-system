-- Migration 0066: app_owner owns the schema, app_runtime gets DML only (MOH-45).
--
-- ADR-004 section 3, gap T2. Row-Level Security does nothing for a superuser or for the table
-- owner, so the backend has to connect as a role that is neither: app_runtime. Migrations run as
-- app_owner (`server migrate` with MIGRATION_DATABASE_URL). Both roles are created by
-- infra/postgres/init/02-roles.sh, which this migration requires.
--
-- Two starting points:
--   - A fresh database migrated as app_owner from 0001: app_owner already owns everything, so the
--     ownership loops below find nothing to move. Only the grants and default privileges matter.
--   - A database created by the superuser before the roles existed: run 02-roles.sh against it
--     first (it moves ownership as the superuser). Applying this migration as the superuser has
--     the same effect. app_owner cannot take ownership from the superuser itself.
--
-- No Row-Level Security policy is added here: every table stays fully readable, so switching
-- the backend to app_runtime changes nothing it can see.

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'app_owner')
     OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'app_runtime') THEN
    RAISE EXCEPTION 'roles app_owner and app_runtime are missing; run infra/postgres/init/02-roles.sh first';
  END IF;
END
$$;

-- Move every table, view, standalone sequence, function and type in public that someone other
-- than app_owner owns. Extension members stay with their extension; indexes and sequences owned
-- by a table follow the table.
DO $$
DECLARE
  obj record;
BEGIN
  FOR obj IN
    SELECT c.relkind, format('%I.%I', n.nspname, c.relname) AS name
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'public'
      AND c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S')
      AND c.relowner <> 'app_owner'::regrole
      AND NOT EXISTS (SELECT 1 FROM pg_depend d
                      WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid
                        AND d.deptype IN ('e', 'a', 'i'))
  LOOP
    EXECUTE format('ALTER %s %s OWNER TO app_owner',
                   CASE obj.relkind WHEN 'S' THEN 'SEQUENCE'
                                    WHEN 'v' THEN 'VIEW'
                                    WHEN 'm' THEN 'MATERIALIZED VIEW'
                                    WHEN 'f' THEN 'FOREIGN TABLE'
                                    ELSE 'TABLE' END,
                   obj.name);
  END LOOP;

  FOR obj IN
    SELECT p.oid::regprocedure AS sig, p.prokind
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'public'
      AND p.proowner <> 'app_owner'::regrole
      AND NOT EXISTS (SELECT 1 FROM pg_depend d
                      WHERE d.classid = 'pg_proc'::regclass AND d.objid = p.oid
                        AND d.deptype = 'e')
  LOOP
    EXECUTE format('ALTER %s %s OWNER TO app_owner',
                   CASE obj.prokind WHEN 'p' THEN 'PROCEDURE'
                                    WHEN 'a' THEN 'AGGREGATE'
                                    ELSE 'FUNCTION' END,
                   obj.sig);
  END LOOP;

  -- Enums, domains, ranges and standalone composite types. Array types and table row types
  -- follow their owner.
  FOR obj IN
    SELECT t.oid::regtype AS name, t.typtype
    FROM pg_type t
    JOIN pg_namespace n ON n.oid = t.typnamespace
    WHERE n.nspname = 'public'
      AND t.typowner <> 'app_owner'::regrole
      AND t.typtype IN ('e', 'd', 'c', 'r')
      AND (t.typtype <> 'c' OR (SELECT c.relkind FROM pg_class c WHERE c.oid = t.typrelid) = 'c')
      AND NOT EXISTS (SELECT 1 FROM pg_depend d
                      WHERE d.classid = 'pg_type'::regclass AND d.objid = t.oid
                        AND d.deptype IN ('e', 'i'))
  LOOP
    EXECUTE format('ALTER %s %s OWNER TO app_owner',
                   CASE obj.typtype WHEN 'd' THEN 'DOMAIN' ELSE 'TYPE' END,
                   obj.name);
  END LOOP;
END
$$;

-- app_runtime reads and writes rows; it cannot create, alter, drop or truncate anything.
GRANT USAGE ON SCHEMA public TO app_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO app_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO app_runtime;

-- Tables and sequences that later migrations create (as app_owner) get the same grants. Also set
-- by 02-roles.sh; repeated here so a database whose roles were created another way still gets it.
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO app_runtime;
