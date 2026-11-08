#!/bin/bash
# Runs once, on first start of an empty data volume (docker-entrypoint-initdb.d), after
# 01-extensions.sh. Creates the two application roles that make Row-Level Security meaningful
# (ADR-004 section 3): RLS does nothing if the app connects as a superuser or as the table owner.
#
#   app_owner    owns the database and the public schema; runs migrations. Not a superuser.
#   app_runtime  what the running application connects as: DML only, cannot create or alter
#                anything, owns nothing, NOBYPASSRLS. Tables that app_owner creates later are
#                readable and writable by it through the default privileges set below.
#
# The bootstrap superuser (POSTGRES_USER) stays for administration and for the current backend,
# which still connects as it until MOH-31 switches it over.
#
# Passwords come from the environment (.env, see .env.example). There are deliberately no
# defaults: if either is unset the roles are NOT created, so a stack started without them (for
# example staging) never gets an account with a well-known password.
set -euo pipefail

if [ -z "${APP_OWNER_PASSWORD:-}" ] || [ -z "${APP_RUNTIME_PASSWORD:-}" ]; then
  echo "02-roles.sh: APP_OWNER_PASSWORD / APP_RUNTIME_PASSWORD are not set; skipping role creation." >&2
  echo "02-roles.sh: copy .env.example to .env and start the stack with --env-file .env," >&2
  echo "02-roles.sh: then recreate the volume (docker compose down -v) to create the roles." >&2
  exit 0
fi

# Variables are passed to psql with -v and quoted by psql (:'name' literals, :"name" identifiers),
# so passwords and database names containing quotes cannot break out of the statement.
psql -v ON_ERROR_STOP=1 \
     -v owner_password="${APP_OWNER_PASSWORD}" \
     -v runtime_password="${APP_RUNTIME_PASSWORD}" \
     -v dbname="${POSTGRES_DB}" \
     --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" <<-'SQL'
	CREATE ROLE app_owner LOGIN PASSWORD :'owner_password'
	  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
	CREATE ROLE app_runtime LOGIN PASSWORD :'runtime_password'
	  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;

	-- app_owner owns the database and the schema, so migrations need no superuser rights.
	ALTER DATABASE :"dbname" OWNER TO app_owner;
	ALTER SCHEMA public OWNER TO app_owner;

	-- Nobody gets anything by default; grant explicitly.
	REVOKE ALL ON DATABASE :"dbname" FROM PUBLIC;
	REVOKE ALL ON SCHEMA public FROM PUBLIC;
	GRANT CONNECT ON DATABASE :"dbname" TO app_runtime;
	GRANT USAGE ON SCHEMA public TO app_runtime;

	-- Objects created by migrations (run as app_owner) are DML-accessible to the runtime role.
	ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
	  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO app_runtime;
	ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public
	  GRANT USAGE, SELECT ON SEQUENCES TO app_runtime;
SQL

echo "02-roles.sh: created app_owner and app_runtime."
