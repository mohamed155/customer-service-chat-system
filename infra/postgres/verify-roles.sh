#!/bin/bash
# Proves the role split works: the runtime role cannot get around a Row-Level Security policy.
#
# Runs inside the postgres container (it needs the POSTGRES_* and APP_*_PASSWORD variables):
#
#   docker compose -f infra/docker-compose.yml exec -T postgres bash -s < infra/postgres/verify-roles.sh
#
# It creates a throwaway table with FORCE ROW LEVEL SECURITY as app_owner, tries to read, write
# and bypass it as app_runtime, and drops it again. Exits non-zero on the first failed check.
set -uo pipefail

host="$(hostname -i | cut -d' ' -f1)"   # non-loopback, so password authentication applies
db="${POSTGRES_DB}"
probe="rls_probe_$$"
tenant_a="00000000-0000-0000-0000-00000000000a"
tenant_b="00000000-0000-0000-0000-00000000000b"
failures=0

# run <role> <password> <sql>: prints the result, returns psql's exit status.
run() { PGPASSWORD="$2" psql -X -h "${host}" -U "$1" -d "${db}" -tA -v ON_ERROR_STOP=1 -c "$3" 2>&1; }
as_owner()   { run app_owner   "${APP_OWNER_PASSWORD}"   "$1"; }
as_runtime() { run app_runtime "${APP_RUNTIME_PASSWORD}" "$1"; }
as_super()   { run "${POSTGRES_USER}" "${POSTGRES_PASSWORD}" "$1"; }

pass() { echo "PASS  $1"; }
fail() { echo "FAIL  $1"; [ -n "${2:-}" ] && echo "      got: $2"; failures=$((failures + 1)); }

# expect_eq <description> <expected> <actual>
expect_eq() { if [ "$2" = "$3" ]; then pass "$1"; else fail "$1" "$3 (expected $2)"; fi; }
# expect_denied <description> <command...>: the command must fail
expect_denied() {
  local description="$1"; shift
  local out; out="$("$@")" && fail "${description} (it succeeded)" "${out}" || pass "${description}"
}

cleanup() { as_owner "DROP TABLE IF EXISTS ${probe}" >/dev/null 2>&1; }
trap cleanup EXIT

echo "== role attributes"
for role in app_owner app_runtime; do
  attrs="$(as_super "select concat(rolsuper::int, rolbypassrls::int, rolcreatedb::int, rolcreaterole::int, rolreplication::int) from pg_roles where rolname = '${role}'")"
  expect_eq "${role}: not superuser, no BYPASSRLS, no CREATEDB/CREATEROLE/REPLICATION" "00000" "${attrs}"
done

# app_platform owns the audited cross-tenant functions: it cannot log in, but it bypasses RLS so
# those functions keep working once policies are forced.
attrs="$(as_super "select concat(rolsuper::int, rolbypassrls::int, rolcreatedb::int, rolcreaterole::int, rolreplication::int, rolcanlogin::int) from pg_roles where rolname = 'app_platform'")"
expect_eq "app_platform: BYPASSRLS, not superuser, no CREATEDB/CREATEROLE/REPLICATION, cannot log in" "010000" "${attrs}"
expect_eq "app_owner can SET ROLE app_platform but does not inherit it" "t f" \
  "$(as_super "select pg_has_role('app_owner', 'app_platform', 'SET'), pg_has_role('app_owner', 'app_platform', 'USAGE')" | tr '|' ' ')"
expect_eq "app_runtime cannot become app_platform" "f" "$(as_super "select pg_has_role('app_runtime', 'app_platform', 'SET')")"

echo "== sample RLS policy, set up as app_owner"
setup() { local out; out="$("$@")" || { echo "FAIL  setup step failed: $*"; echo "      got: ${out}"; exit 1; }; }
setup as_owner "create table ${probe} (id serial primary key, tenant_id uuid not null, note text not null)"
setup as_owner "alter table ${probe} enable row level security"
setup as_owner "alter table ${probe} force row level security"
setup as_owner "create policy tenant_isolation on ${probe} using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)"
# Seed as the bootstrap superuser: FORCE ROW LEVEL SECURITY applies to the owner too, so
# app_owner itself could not insert rows without first setting a tenant.
setup as_super "insert into ${probe} (tenant_id, note) values ('${tenant_a}', 'a1'), ('${tenant_a}', 'a2'), ('${tenant_b}', 'b1')"
expect_eq "app_runtime owns no relations" "0" "$(as_super "select count(*) from pg_class c join pg_roles r on r.oid = c.relowner where r.rolname = 'app_runtime'")"

echo "== app_runtime cannot see other tenants' rows"
expect_eq "no tenant set returns no rows" "0" "$(as_runtime "select count(*) from ${probe}")"
expect_eq "empty tenant returns no rows" "0" "$(as_runtime "select set_config('app.tenant_id', '', false); select count(*) from ${probe}" | tail -n1)"
expect_eq "tenant A sees only its 2 rows" "2" "$(as_runtime "select set_config('app.tenant_id', '${tenant_a}', false); select count(*) from ${probe}" | tail -n1)"
expect_eq "tenant A sees no tenant B rows" "0" "$(as_runtime "select set_config('app.tenant_id', '${tenant_a}', false); select count(*) from ${probe} where tenant_id = '${tenant_b}'" | tail -n1)"

echo "== app_runtime has the DML it needs on its own tenant (so the denials below come from the policy, not from missing grants)"
expect_eq "tenant A can insert its own row (uses the serial sequence)" "3" "$(as_runtime "select set_config('app.tenant_id', '${tenant_a}', false); insert into ${probe} (tenant_id, note) values ('${tenant_a}', 'a3'); select count(*) from ${probe}" | tail -n1)"
expect_eq "tenant A can update and delete its own rows" "1" "$(as_runtime "select set_config('app.tenant_id', '${tenant_a}', false); update ${probe} set note = 'a1-edited' where note = 'a1'; delete from ${probe} where note = 'a3'; select count(*) from ${probe} where note = 'a1-edited'" | tail -n1)"
expect_eq "tenant A cannot update tenant B rows (0 rows affected)" "b1" "$(as_super "select note from ${probe} where tenant_id = '${tenant_b}'")"

echo "== app_runtime cannot bypass or change the policy"
expect_denied "cannot insert a row for another tenant (WITH CHECK)" \
  as_runtime "select set_config('app.tenant_id', '${tenant_a}', false); insert into ${probe} (tenant_id, note) values ('${tenant_b}', 'leak')"
expect_denied "cannot switch row security off (query errors instead of returning rows)" \
  as_runtime "set row_security = off; select count(*) from ${probe}"
expect_denied "cannot disable RLS on the table (not the owner)" as_runtime "alter table ${probe} disable row level security"
expect_denied "cannot drop the policy" as_runtime "drop policy tenant_isolation on ${probe}"
expect_denied "cannot create tables (no DDL)" as_runtime "create table ${probe}_ddl (id int)"
expect_denied "cannot become another role" as_runtime "set role app_owner"
expect_denied "cannot become app_platform" as_runtime "set role app_platform"
expect_denied "cannot grant itself BYPASSRLS" as_runtime "alter role app_runtime bypassrls"

echo "== app_owner is a member of app_platform but still subject to the policy (FORCE applies to the owner)"
expect_eq "app_owner reads no rows through the forced policy" "0" "$(as_owner "select count(*) from ${probe}")"

echo "== for contrast: the bootstrap superuser ignores the policy, which is why the app must not use it"
expect_eq "superuser sees all 3 rows" "3" "$(as_super "select count(*) from ${probe}")"

echo
if [ "${failures}" -eq 0 ]; then echo "All role checks passed."; else echo "${failures} role check(s) failed."; exit 1; fi
