#!/bin/bash
# Compose health check for the dev Postgres. Healthy means:
#   1. the server accepts TCP connections (so first-run init scripts have finished),
#   2. POSTGRES_USER / POSTGRES_PASSWORD / POSTGRES_DB authenticate against the stored database
#      (`pg_isready` does not check any of that, so settings that drifted from an old volume would
#      otherwise look healthy until the first migration fails). The connection uses the
#      container IP, not 127.0.0.1: the image trusts loopback, so only a non-loopback connection
#      actually verifies the password, and
#   3. vector, pg_trgm and citext exist in that database and in template1 (an interrupted first
#      start leaves a data directory that skips the init script on every later start).
# If this stays unhealthy after you changed POSTGRES_* or interrupted the first start, see the
# README ("Resetting local data"): the fix is to recreate the volume, which deletes its data.
set -euo pipefail

host="$(hostname -i | cut -d' ' -f1)"
export PGPASSWORD="${POSTGRES_PASSWORD}"
query="select count(*) from pg_extension where extname in ('vector', 'pg_trgm', 'citext')"

for db in "${POSTGRES_DB}" template1; do
  [ "$(psql -h "${host}" -U "${POSTGRES_USER}" -d "${db}" -tA -c "${query}")" = "3" ]
done
