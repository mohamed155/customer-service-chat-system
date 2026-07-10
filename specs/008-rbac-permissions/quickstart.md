# Quickstart: Validating RBAC & Permissions

**Feature**: 008-rbac-permissions. Validation scenarios proving the feature end-to-end. Contracts: [permissions.md](./contracts/permissions.md), [rest-api.md](./contracts/rest-api.md); entities: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running with migrations applied (`backend/migrations/`), `.env` per `.env.example`.
- Backend: run from `backend/`. Frontend: run from `frontend/` (`pnpm install` done).
- Live-gated integration tests follow the existing pattern in `backend/crates/server/tests/tenancy.rs` (skip unless `DATABASE_URL` reachable).

## 1. Automated verification (the gates)

```powershell
# Backend — unit (matrix invariants, catalog↔contract parity) + integration (role×operation matrix)
cd backend
cargo test

# Frontend — permission service/guard/directive/sidebar specs + full quality gates
cd ../frontend
pnpm ng test dashboard
pnpm ng build dashboard
pnpm lint
pnpm format:check
```

Expected: all pass. The new backend integration suite is `crates/server/tests/rbac.rs`; new frontend specs live beside `core/authz/*` and `layout/sidebar/*`.

## 2. API enforcement matrix (US1 — P1)

Seed one user per role (10 users) plus one user with no role/membership, as in the `tenancy.rs` seed helpers. Then, per user, exercise the declared endpoints (see rest-api.md) and assert:

| Check | Caller | Request | Expect |
|-------|--------|---------|--------|
| Deny mutation to Viewer | tenant `viewer` | any `.manage`-guarded operation | 403 `unauthorized`, no data change |
| Deny platform to tenant user | tenant `owner` | `GET /api/v1/platform/tenants` | 403 `unauthorized` |
| Allow platform per role | platform `finance` | `GET /api/v1/platform/tenants` | 200 |
| Deny by default | user with no roles | every `/api/v1` route except login/logout/me | non-2xx |
| 401 vs 403 distinguishable | unauthenticated | any protected route | 401 `unauthenticated` |

Manual spot-check (dev env uses the `X-Dev-User-Id` header, cookie flow also works):

```powershell
# 403 for a viewer hitting a guarded route
curl -s -H "X-Dev-User-Id: <viewer-uuid>" -H "X-Tenant-ID: <tenant-uuid>" `
  http://localhost:8080/api/v1/tenant   # 200 (overview.view)
# vs. a platform-only route:
curl -s -H "X-Dev-User-Id: <viewer-uuid>" http://localhost:8080/api/v1/platform/tenants   # 403
```

## 3. Navigation & route visibility (US2 — P2)

Start both apps (`cargo run` in `backend/crates/server`; `pnpm ng serve dashboard` in `frontend/`). Sign in as each tenant role and verify against the page→permission table:

1. **Support Agent**: sidebar shows Overview, Conversations, Customers, Knowledge Base only; no Settings/AI Agent/Integrations/Analytics entries.
2. **Viewer**: deep-link to `/tenant/settings` → redirected to first allowed page (Overview); settings content never flashes.
3. **Owner**: all eight tenant pages visible and loadable.
4. **Any role**: `/me` response in devtools contains `permissions` arrays matching the contract matrix; the app makes no other permission source visible.
5. **In-page gating**: elements wrapped in `*appHasPermission` (e.g. manage-level buttons) absent for Viewer.

## 4. Platform staff in tenant (US3 — P3)

With `ENVIRONMENT=development` (non-prod): switch into a tenant as each platform role → full tenant nav and operations succeed.

With `ENVIRONMENT=production` (or the integration-test equivalent config):

1. Super Admin switched into a tenant: every tenant operation succeeds.
2. Support Engineer: Conversations works (view + manage); Settings hidden and `settings.manage` API calls 403.
3. Sales: read-only pages visible (Overview, Analytics, Members, Settings view); any mutation 403.

The backend integration suite covers both environment values by constructing the router with each config; manual verification of prod behavior is optional.

## 5. Immediate role change (FR-011 edge case)

1. Sign in as tenant Admin, open Settings.
2. In the database, downgrade the membership: `UPDATE tenant_memberships SET role='viewer' WHERE user_id='…' AND tenant_id='…';`
3. Perform any settings change in the open session → API returns 403 immediately (next request evaluates the new role).
4. The 403 triggers a `/me` refresh → sidebar drops Settings and the app routes away from the page without re-login.

## Expected outcomes summary

- SC-001: `cargo test` rbac matrix — zero unauthorized 2xx.
- SC-002/SC-003: per-role nav sets match contracts/permissions.md; blocked deep-links redirect with no content flash.
- SC-004: no perceptible latency change (permission check is in-memory; no added queries).
- SC-005: every role appears in at least one allow and one deny assertion across backend + frontend suites.
