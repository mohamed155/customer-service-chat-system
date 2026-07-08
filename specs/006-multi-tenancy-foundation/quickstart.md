# Quickstart: Multi-Tenancy Foundation

**Feature**: 006-multi-tenancy-foundation
**Proves**: tenant isolation, platform switching with audit, automatic frontend tenant context — per [contracts/http-api.md](contracts/http-api.md).

## Prerequisites

- Postgres up with feature-005 migrations applied: `docker compose -f infra/docker-compose.yml up -d postgres`, then `cd backend && sqlx migrate run`
- Backend running in development: `cd backend && cargo run -p server` (`APP_ENVIRONMENT=development`)
- Seed two tenants + two users via psql (a platform user with `platform_role='support'`; a tenant user with a membership in tenant A only) — the isolation tests seed their own data; manual seeding is only for the walkthrough below
- Frontend: `cd frontend && pnpm ng serve dashboard`; set your dev principal in the browser: `localStorage.setItem('app.devUserId', '<user uuid>')`

## 1. Isolation matrix — automated (FR-018, SC-001, SC-006)

```bash
cd backend
cargo test -p server --test tenancy
```

**Expected**: all matrix scenarios pass (own tenant 200; foreign/nonexistent tenant 403 with byte-identical bodies; missing/malformed header 400; suspended tenant member 403 vs platform 200; revoked membership 403; dev header ignored outside dev/test → 401; switch + denial audit rows present). Skips with a notice when `DATABASE_URL` is unreachable; runs for real in CI.

## 2. Manual API walkthrough (curl)

```bash
B=http://localhost:8080/api/v1
TU=<tenant-user-uuid>; PU=<platform-user-uuid>; TA=<tenant-a-uuid>; TB=<tenant-b-uuid>

curl -s $B/me -H "X-Dev-User-Id: $TU"                          # 200: memberships list tenant A
curl -s $B/tenant -H "X-Dev-User-Id: $TU" -H "X-Tenant-ID: $TA" # 200: tenant A profile
curl -s $B/tenant -H "X-Dev-User-Id: $TU" -H "X-Tenant-ID: $TB" # 403 unauthorized
curl -s $B/tenant -H "X-Dev-User-Id: $TU" -H "X-Tenant-ID: $(uuidgen)" # 403 — same body as above
curl -s $B/tenant -H "X-Dev-User-Id: $TU"                       # 400 validation_failed (missing header)
curl -s $B/platform/tenants -H "X-Dev-User-Id: $TU"             # 403 (not a platform user)
curl -s $B/platform/tenants -H "X-Dev-User-Id: $PU"             # 200: directory incl. suspended
curl -s -X POST $B/platform/tenants/$TB/switch -H "X-Dev-User-Id: $PU" # 200 + audit row
curl -s $B/tenant -H "X-Dev-User-Id: $PU" -H "X-Tenant-ID: $TB" # 200: platform user in any tenant
```

Verify audit (psql): `SELECT action, actor_user_id, tenant_id, details FROM audit_logs ORDER BY created_at DESC LIMIT 5;` → shows `platform.tenant_switched` and `tenant.access_denied` rows per [data-model.md](data-model.md#audit-vocabulary-rows-in-005s-audit_logs).

## 3. Dev-header hard-disable (FR-019)

Restart the server with `APP_ENVIRONMENT=production` (set required prod env vars) and repeat any request above: **expected 401 `unauthenticated`** — the header is ignored entirely.

## 4. Frontend behavior (US3)

With the dashboard served and `app.devUserId` set:

1. **Platform user**: topbar shows the tenant switcher; picking tenant A fires `POST …/switch`, then subsequent API calls carry `X-Tenant-ID` (inspect DevTools → Network). Reload → tenant A still active (localStorage). Point `app.devUserId` at the tenant user → switcher disappears entirely; their tenant auto-resolves from `/me`.
2. **Forbidden state**: as the platform user, delete the selected tenant in psql (soft-delete), reload → stored selection is discarded, "no tenant selected" prompt shows.
3. **Frontend specs**: `cd frontend && pnpm ng test dashboard` — interceptor, store persistence/discard, switcher visibility, and forbidden-mapping specs pass.

## 5. Quality gates

- Backend (`backend/`): `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (with Postgres up so live tests execute)
- Frontend (`frontend/`): `pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check`
