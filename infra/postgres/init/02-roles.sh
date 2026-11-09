#!/bin/bash
# Creates the two application roles that make Row-Level Security meaningful (ADR-004 section 3):
# RLS does nothing if the app connects as a superuser or as the table owner.
#
#   app_owner    owns the database, the public schema and every table; runs migrations
#                (`server migrate` with MIGRATION_DATABASE_URL). Not a superuser.
#   app_runtime  what the running application connects as (DATABASE_URL): DML only, cannot create
#                or alter anything, owns nothing, NOBYPASSRLS.
#
# The bootstrap superuser (POSTGRES_USER) stays for administration only.
#
# Where it runs (it is idempotent, so running it again is safe):
#   - docker-entrypoint-initdb.d, once, on first start of an empty volume (after 01-extensions.sh);
#   - CI, against the plain Postgres service container (PGHOST/PGPASSWORD set, ROLES_REQUIRED=1);
#   - an existing database, e.g. a local volume created before the roles existed or staging:
#       docker compose ... exec -T -e ROLES_REQUIRED=1 postgres bash -s < infra/postgres/init/02-roles.sh
#     On an existing database it also hands every table, sequence, view, function and type in
#     `public` that the superuser created over to app_owner, so migrations can run as app_owner.
#
# Passwords come from the environment (.env, see .env.example). There are deliberately no
# defaults: if either is unset the roles are NOT created, so a stack started without them never
# gets an account with a well-known password. With ROLES_REQUIRED=1 that is an error instead.
set -euo pipefail

if [ -z "${APP_OWNER_PASSWORD:-}" ] || [ -z "${APP_RUNTIME_PASSWORD:-}" ]; then
  if [ "${ROLES_REQUIRED:-0}" = "1" ]; then
    echo "02-roles.sh: APP_OWNER_PASSWORD and APP_RUNTIME_PASSWORD must be set." >&2
    exit 1
  fi
  echo "02-roles.sh: APP_OWNER_PASSWORD / APP_RUNTIME_PASSWORD are not set; skipping role creation." >&2
  echo "02-roles.sh: copy .env.example to .env and start the stack with --env-file .env," >&2
  echo "02-roles.sh: then recreate the volume (docker compose down -v) to create the roles." >&2
  exit 0
fi

# Variables are passed to psql with -v and quoted by psql (:'name' literals, :"name" identifiers),
# so passwords and database names containing quotes cannot break out of the statement. psql does
# not interpolate inside DO blocks, so the passwords are handed to them through set_config.
psql -v ON_ERROR_STOP=1 \
     -v owner_password="${APP_OWNER_PASSWORD}" \
     -v runtime_password="${APP_RUNTIME_PASSWORD}" \
     -v dbname="${POSTGRES_DB}" \
     --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" <<-'SQL'
	SELECT set_config('roles.owner_password', :'owner_password', false),
	       set_config('roles.runtime_password', :'runtime_password', false) \g /dev/null

	DO $$
	DECLARE
	  r text;
	BEGIN
	  FOREACH r IN ARRAY ARRAY['app_owner', 'app_runtime'] LOOP
	    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = r) THEN
	      EXECUTE format('CREATE ROLE %I LOGIN', r);
	    END IF;
	    -- Re-applied on every run, so an existing role can never keep elevated attributes.
	    EXECUTE format(
	      'ALTER ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS',
	      r,
	      current_setting(CASE r WHEN 'app_owner' THEN 'roles.owner_password'
	                             ELSE 'roles.runtime_password' END));
	  END LOOP;
	END
	$$;

	SELECT set_config('roles.owner_password', '', false),
	       set_config('roles.runtime_password', '', false) \g /dev/null

	-- app_owner owns the database and the schema, so migrations need no superuser rights.
	ALTER DATABASE :"dbname" OWNER TO app_owner;
	ALTER SCHEMA public OWNER TO app_owner;

	-- Nobody gets anything by default; grant explicitly.
	REVOKE ALL ON DATABASE :"dbname" FROM PUBLIC;
	REVOKE ALL ON SCHEMA public FROM PUBLIC;
	GRANT CONNECT ON DATABASE :"dbname" TO app_runtime;
	GRANT USAGE ON SCHEMA public TO app_runtime;

	-- An existing database (created before the roles, by the superuser): hand its objects to
	-- app_owner. Extension members (pgvector, pg_trgm, citext) stay with the extension. Indexes
	-- and sequences owned by a table follow the table. A fresh database has nothing to move.
	DO $$
	DECLARE
	  obj record;
	BEGIN
	  FOR obj IN
	    SELECT c.oid, c.relkind,
	           format('%I.%I', n.nspname, c.relname) AS name
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

	  -- Enums, domains, ranges and standalone composite types. Array types and table row types follow their owner.
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

	-- Tables that already exist are DML-accessible to the runtime role (migration 0066 does the
	-- same, for databases whose roles were created elsewhere) ...
	GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO app_runtime;
	GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO app_runtime;

	-- ... and so is everything migrations (run as app_owner) create later.
	ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
	  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_runtime;
	ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
	  GRANT USAGE, SELECT ON SEQUENCES TO app_runtime;
SQL

echo "02-roles.sh: app_owner and app_runtime are in place."
