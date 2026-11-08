#!/bin/bash
# Runs once, on first start of an empty data volume (docker-entrypoint-initdb.d).
# Installs the extensions the migrations expect, in POSTGRES_DB and in template1 so databases
# created later (for example by `sqlx::test`) inherit them.
set -euo pipefail

for db in template1 "${POSTGRES_DB}"; do
  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${db}" <<-'SQL'
	CREATE EXTENSION IF NOT EXISTS vector;
	CREATE EXTENSION IF NOT EXISTS pg_trgm;
	CREATE EXTENSION IF NOT EXISTS citext;
SQL
done
