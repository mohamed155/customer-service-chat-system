# Implementation Plan: Platform Tenant Management

**Branch**: `010-platform-tenant-management` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/010-platform-tenant-management/spec.md`

## Summary

Give platform staff full lifecycle management of customer organizations: extend the `tenants` table with business metadata (plan/tier + primary contact) via migration, add a `platform.tenants.manage` permission (Super Admin + Support Engineer) to the central authz catalog, implement create/detail/update endpoints alongside the existing directory listing (which gains a status filter), and build the platform-area frontend pages — tenant list with search/filter/pagination, detail page, and a shared create/edit form — reachable from the 009 platform-nav control, permission-gated, and audited end to end.

Technical approach: backend work lives in the existing `tenancy` module (where `list_tenants`/`switch_tenant` already live) plus one-line catalog/matrix additions in `authz`; routes register through the fail-closed `.guarded()` builder from 008. The PATCH handler runs in a database transaction that calls the existing `set_audit_actor()` function — mandatory, because the live `tenants_slug_change_audit` trigger rejects slug updates without a transaction-local actor. Frontend adds a `features/platform/tenants/` area (Observable-based data service + SignalStore, per constitution v1.2.0's RxJS-first rule) composing existing shared components (data-table, status-badge, toolbar, search-input, page-header/container). The platform area's route gate is rebalanced: the area opens to any platform role holding `platform.tenants.list`; the overview placeholder keeps `platform.admin`.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum, SQLx (PostgreSQL), existing `authz`/`tenancy`/`identity` module crates; Angular Router, Reactive Forms, NgRx SignalStore (feature-local list/detail state), existing `core/authz` + shared components; RxJS operators for all new async flows (constitution v1.2.0)

**Storage**: PostgreSQL — migration `0016` adds `plan TEXT NOT NULL DEFAULT 'trial'` (CHECK: trial/starter/professional/enterprise), `contact_name TEXT NULL` (length ≤200 CHECK), `contact_email TEXT NULL` to `tenants`. Existing constraints (name length, slug format, live-slug partial unique index, status CHECK) unchanged. The existing `tenants_slug_change_audit` trigger (migration 0015) is a hard constraint: slug updates MUST run in a transaction that has called `set_audit_actor(actor_id)`.

**Testing**: `cargo test` — extend `backend/crates/server/tests/rbac.rs` (new endpoints in the role×operation matrix) + new live-gated suite `backend/crates/server/tests/platform_tenants.rs` (CRUD, validation, filter/pagination, audit rows, suspension immediacy); Vitest for service/store/page/form specs

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — existing Cargo workspace + Angular pnpm workspace

**Performance Goals**: List stays a single query (search + status filter + cursor in one statement, as today); no N+1; SC-002's 500-tenant directory well within the existing single-query pattern; no added round-trips on the write paths beyond the audit insert (and the transaction for PATCH)

**Constraints**: Deny-by-default routing (008 `.guarded()` builder — permission is a required argument); 401/403/404/409/422 from the existing `kernel::ApiError` vocabulary (`conflict` for slug collisions, `validation_failed` + `ErrorDetail` for field errors); schema changes via migration only (Constitution VIII); RxJS-first frontend async — new services expose Observables, no `firstValueFrom` outside inherently Promise-based boundaries (constitution v1.2.0); no frontend role→permission mapping (008 FR-010)

**Scale/Scope**: 1 migration; 1 new permission code (catalog 25→26); 3 new endpoints + 1 extended; ~3 new frontend pages + 1 shared form component + 1 data service + 1 SignalStore; 2 route-gate adjustments; 1 platform-nav destination added

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | New endpoints join the existing tenant routes in the `tenancy` module (cohesion with `list_tenants`/`switch_tenant`); `authz` gains only catalog/matrix entries through its documented interface; no cross-module data access | ✅ Pass |
| II. Multi-Tenant Isolation | All new operations are platform-scoped and pass through the existing principal/permission middleware; tenant users are denied by the same server-side boundary; frontend gating is presentation-only | ✅ Pass |
| III. Zero-Trust Security & RBAC | New `platform.tenants.manage` permission, deny-by-default registration, every create/edit/status change audited (app-level `audit::record` + the existing DB slug trigger); no secrets | ✅ Pass |
| IV. AI Provider Independence | Not touched | ✅ N/A |
| V. API-First & Contract Consistency | Endpoints documented in `contracts/rest-api.md`; cursor pagination + error envelope reused; PATCH is partial-update with explicit semantics | ✅ Pass |
| VI. Observability by Default | Request-id/tracing paths unchanged; sensitive changes land in the append-only audit trail with actor/action/time | ✅ Pass |
| VII. Test-First & Regression Discipline | rbac matrix extension (allow/deny for all ten roles on the new operations) + dedicated integration suite + frontend specs required per story | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration-only schema change with CHECK constraints; no new index for the status filter — justified: the directory is a few hundred rows behind an already-indexed `deleted_at IS NULL` scan; revisit if directory growth changes the profile | ✅ Pass |
| IX. Design System Discipline | Pages compose existing shared components (data-table, status-badge, toolbar, search-input, empty/loading states, page-header/container); one new reusable form pattern; no raw Taiga styling | ✅ Pass |
| X. Performance & Efficiency | Single-statement list query; in-memory permission checks; RxJS-first flows (debounced search via operators, no polling) | ✅ Pass |

**Initial gate**: PASS — no violations, Complexity Tracking not required.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no deviations. The one nuanced call (slug-change audit emitted by the DB trigger while other field changes are audited app-side) follows the mechanism the schema already mandates rather than duplicating it.

## Project Structure

### Documentation (this feature)

```text
specs/010-platform-tenant-management/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── rest-api.md      # Endpoint contracts: list filter, create, detail, patch, errors, audit actions
│   └── permissions.md   # Catalog delta: platform.tenants.manage + matrix row + page permissions
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0016_tenant_business_metadata.sql   # NEW — plan + contact columns with CHECKs
└── crates/
    ├── modules/
    │   ├── authz/src/
    │   │   ├── permission.rs               # MODIFIED — PlatformTenantsManage variant (catalog 25→26; parity test list updated)
    │   │   └── matrix.rs                   # MODIFIED — SuperAdmin + Support gain the manage permission
    │   └── tenancy/src/
    │       ├── routes.rs                   # MODIFIED — status filter on list; TenantDetail; create/get/update handlers
    │       └── audit.rs                    # UNCHANGED API — reused for platform.tenant_created/updated/status_changed
    └── server/
        ├── src/router.rs                   # MODIFIED — POST /platform/tenants, GET/PATCH /platform/tenants/{id} via .guarded()
        └── tests/
            ├── rbac.rs                     # MODIFIED — new operations in the role×operation matrix
            └── platform_tenants.rs         # NEW — CRUD/validation/filter/audit/suspension integration suite

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts            # MODIFIED — TenantPlan union, PlatformTenantDetail, create/update payloads
│   ├── authz/permissions.ts                # MODIFIED — 'platform.tenants.manage' + PAGE_PERMISSIONS platform entries rebalanced
│   └── router/
│       ├── app-paths.ts                    # MODIFIED — platform.tenants / new / :id paths
│       └── page-title.ts                   # MODIFIED — platformTenants / platformTenantDetail / platformTenantNew titles
├── layout/topbar/platform-nav.component.ts # MODIFIED — "Tenants" destination (platform.tenants.list)
├── app.routes.ts                           # MODIFIED — platform base gate: platform.admin → platform.tenants.list
└── features/platform/
    ├── platform.routes.ts                  # MODIFIED — tenants list/new/detail child routes (per-route permissions)
    └── tenants/
        ├── platform-tenants.service.ts     # NEW — Observable-based API access (list/get/create/update)
        ├── tenants.store.ts                # NEW — SignalStore: query/filter/cursor state via rxMethod
        ├── tenant-list.component.ts        # NEW — table + search + status filter + load-more + create entry
        ├── tenant-detail.component.ts      # NEW — record view + status action + edit entry (permission-gated)
        └── tenant-form.component.ts        # NEW — shared reactive create/edit form with server-error mapping
```

**Structure Decision**: Backend follows the established pattern — tenant routes stay in `tenancy`, authorization vocabulary in `authz`, registration in `server/router.rs`. Frontend introduces the first real platform feature folder (`features/platform/tenants/`) with feature-scoped service and SignalStore per the spec-002 state rules (feature-local state → SignalStore); `core/` gains only model/permission/path vocabulary.

## Complexity Tracking

No constitution violations — table intentionally empty.
