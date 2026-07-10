# Tasks: RBAC & Permissions

**Input**: Design documents from `/specs/008-rbac-permissions/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/permissions.md, contracts/rest-api.md, quickstart.md

**Tests**: Included — required by spec FR-012/SC-005 and Constitution Principle VII (test-first). Backend integration tests follow the live-gated pattern from `backend/crates/server/tests/tenancy.rs`.

**Organization**: Tasks are grouped by user story. US1 (API enforcement) is the MVP and security boundary; US2 (frontend visibility) and US3 (staff-in-tenant, environment-aware) build on it.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md
- Exact file paths in every description

## Path Conventions

Web app per plan.md: backend Cargo workspace at `backend/crates/`, Angular dashboard at `frontend/apps/dashboard/src/app/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the new `authz` crate and frontend permission vocabulary

- [X] T001 Create `authz` crate skeleton: `backend/crates/modules/authz/Cargo.toml` (deps: `identity`, `config`, `kernel`, axum, serde, tracing, uuid) and `backend/crates/modules/authz/src/lib.rs` with module documentation (Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, Extension Points per constitution) and `pub mod permission; pub mod role; pub mod matrix; pub mod guard;` stubs; register the crate in `backend/Cargo.toml` workspace members and verify `cargo check` passes
- [X] T002 [P] Create frontend permission vocabulary in `frontend/apps/dashboard/src/app/core/authz/permissions.ts`: `Permission` string-literal union covering all 25 codes from `contracts/permissions.md`, plus a `PAGE_PERMISSIONS` const mapping each `APP_PATHS.tenant.*` page and the platform area to its required permission (page→permission table in the contract)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The canonical catalog and matrix every story consumes (FR-001/FR-002)

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T003 [P] Implement `Permission` enum in `backend/crates/modules/authz/src/permission.rs`: all 25 codes from `contracts/permissions.md` with `Display`/`FromStr`/serde serialization to the dot-scoped snake_case strings; unit tests for round-trip and rejection of unknown codes (mirror the `PlatformRole` test style in `backend/crates/modules/identity/src/lib.rs`)
- [X] T004 [P] Implement `TenantRole` enum in `backend/crates/modules/authz/src/role.rs`: `Owner/Admin/Manager/Agent/Viewer` ↔ `owner/admin/manager/agent/viewer` with `Display`/`FromStr` (unknown stored value → `Err`); unit tests for round-trip and invalid values
- [X] T005 Implement the matrix in `backend/crates/modules/authz/src/matrix.rs`: `tenant_role_permissions(TenantRole)`, `platform_role_permissions(PlatformRole)`, `staff_tenant_permissions(PlatformRole, is_production: bool)` as exhaustive `match`es returning `&'static [Permission]` exactly per the three tables in `contracts/permissions.md`; unit tests asserting the data-model invariants: Owner ⊇ Admin ⊇ Manager, Owner − Admin = {billing.view, billing.manage, tenant.delete, owner.assign}, Manager holds no `settings.*`/`billing.*`, Viewer holds only `.view` codes, `staff_tenant_permissions(_, false)` = full tenant set for every role, `staff_tenant_permissions(SuperAdmin, true)` = full tenant set, and every catalog permission is granted to ≥1 role (depends on T003, T004)
- [X] T006 Implement `PermissionSet` and `require_permission` in `backend/crates/modules/authz/src/guard.rs`: `PermissionSet` wrapper with `contains(Permission)`; `require_permission(Permission)` producing an Axum `route_layer` (via `from_fn`) that returns `kernel::ApiError::unauthorized("Access denied")` (403) when the request's `PermissionSet` extension is missing entirely OR lacks the permission (deny by default, FR-003/FR-006), recording `authz.denied_permission` on the tracing span with a `warn!`; unit tests: allow when present, deny when absent, deny when no extension (depends on T005)

**Checkpoint**: `cargo test -p authz` green — user story implementation can begin

---

## Phase 3: User Story 1 - Unauthorized API access is rejected (Priority: P1) 🎯 MVP

**Goal**: Every `/api/v1` operation declares a required permission and refuses callers whose live database role lacks it, with 401 vs 403 kept distinct (FR-003/004/005/006/011 server half)

**Independent Test**: `cargo test --test rbac` — API requests as users holding each of the ten roles (plus a no-role user and an unauthenticated caller) against the declared endpoint matrix; allow/deny matches `contracts/rest-api.md` with no frontend involved

### Tests for User Story 1 (write first, must fail before implementation)

- [X] T007 [P] [US1] Create failing integration suite `backend/crates/server/tests/rbac.rs` (live-gated pool + seed helpers copied from the `backend/crates/server/tests/tenancy.rs` pattern): seed one user per tenant role in a tenant, one user per platform role, one user with no role/membership; assert (a) allow/deny per the endpoint table in `contracts/rest-api.md` for all ten roles, (b) 401 `unauthenticated` for anonymous vs 403 `unauthorized` for permission denials, (c) deny-by-default sweep — every registered `/api/v1` route except login/logout/me returns non-2xx for the no-role user, (d) unknown stored role value (seed a bypassing UPDATE with constraint-valid but unmapped handling is impossible via CHECK — instead unit-level: `TenantRole::from_str` failure path treated as deny in middleware test)

### Implementation for User Story 1

- [X] T008 [US1] Widen the membership query in `backend/crates/modules/tenancy/src/authorize.rs`: replace `has_active_membership(pool, tenant_id, user_id) -> bool` with `fetch_membership_role(pool, tenant_id, user_id) -> Option<String>` (same single query, now selecting `role`); update the unit test module
- [X] T009 [US1] Extend tenant context in `backend/crates/modules/tenancy/src/lib.rs`: introduce `TenancyConfig { pool: PgPool, is_production: bool }` as middleware state; `TenantContext` gains `tenant_role: Option<authz::TenantRole>` and `permissions: authz::PermissionSet`; middleware computes the set per request — tenant principal → `tenant_role_permissions` from the fetched membership role (unparseable role → `tracing::error!` + deny with the existing `access_denied` audit), platform principal → `staff_tenant_permissions(platform_role, is_production)`; insert `PermissionSet` into request extensions (depends on T008, uses T005/T006)
- [X] T010 [US1] Add platform-scope authz middleware in `backend/crates/modules/authz/src/guard.rs`: `platform_permission_middleware` deriving `PermissionSet` from `Principal.platform_role` via `platform_role_permissions` (no principal or no platform role → empty set) for routes outside tenant context
- [X] T011 [US1] Rework `backend/crates/server/src/router.rs` into fail-closed route groups: `public_routes()` (auth/login, auth/logout), `me` (authenticated, no permission), `platform_routes()` and `tenant_routes()` built through a `.guarded(path, method_router, Permission)` helper whose signature requires the permission argument; declare permissions per `contracts/rest-api.md` (`GET /tenant` → `overview.view`, `GET /platform/tenants` → `platform.tenants.list`, `POST /platform/tenants/{id}/switch` → `platform.tenants.switch`); apply to both `app()` and `app_with_test_routes()`; pass `TenancyConfig` (with `is_production` from `state.config.environment`) to the tenancy middleware (depends on T009, T010)
- [X] T012 [US1] Remove the now-redundant ad-hoc `PrincipalKind::Platform` checks inside `list_tenants` and `switch_tenant` in `backend/crates/modules/tenancy/src/routes.rs` (enforcement now lives in the declared route layer); add the `permission_denied` reason to the tenant-scope denial audit path in `backend/crates/modules/tenancy/src/audit.rs` usage from the guard integration
- [X] T013 [US1] Verify: `cargo test` from `backend/` — T007 matrix fully green, existing `tenancy.rs`/`auth.rs` suites still pass (update their expectations only where the contract intentionally changed, e.g. routes now requiring declared permissions)

**Checkpoint**: US1 fully functional — the platform is protected regardless of client; MVP deliverable

---

## Phase 4: User Story 2 - Users only see the pages and navigation their role allows (Priority: P2)

**Goal**: Server-computed permissions flow through `/me` into a reactive frontend authz layer that filters navigation, guards routes (no transient restricted content), gates in-page actions, and re-syncs on 403 (FR-007/008/009/010/011 UI half)

**Independent Test**: Sign in as each tenant role — visible nav matches the matrix in `contracts/permissions.md`; deep-link to a disallowed page redirects with no content flash (quickstart §3); `pnpm ng test dashboard` green

### Tests for User Story 2 (write first where practical)

- [X] T014 [P] [US2] Extend `backend/crates/server/tests/rbac.rs` with `/me` payload assertions per `contracts/rest-api.md`: tenant Owner membership carries the full 20-permission array, Viewer carries only `.view` codes, platform Support Engineer gets `platformPermissions` + `staffTenantPermissions`, tenant users get `staffTenantPermissions: null`

### Implementation for User Story 2

- [X] T015 [US2] Extend `MeResponse`/`MembershipSummary` in `backend/crates/modules/tenancy/src/routes.rs`: add `platform_permissions: Vec<String>`, `staff_tenant_permissions: Option<Vec<String>>` (environment-resolved via `staff_tenant_permissions(role, is_production)`; `None` for tenant users), and per-membership `permissions: Vec<String>` from `tenant_role_permissions` — all serialized camelCase; make T014 pass (depends on T011 for `TenancyConfig`/environment access)
- [X] T016 [P] [US2] Extend frontend API models in `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`: `MeResponse` gains `platformPermissions: Permission[]`, `staffTenantPermissions: Permission[] | null`; `MembershipSummary` gains `permissions: Permission[]` (types from `core/authz/permissions.ts`); update fixtures used by existing specs
- [X] T017 [US2] Implement `PermissionsService` in `frontend/apps/dashboard/src/app/core/authz/permissions.service.ts` + `permissions.service.spec.ts`: computed signal `Set<Permission>` per the data-model derivation (platform ∪ active-tenant set, using `staffTenantPermissions` for platform users and the matching membership's `permissions` otherwise, from `CurrentUserService.currentUser` + the NgRx `tenantContext` slice); expose `has(permission: Permission): boolean`; specs cover tenant user, platform user with/without active tenant, and no-role user (empty set) (depends on T002, T016)
- [X] T018 [US2] Implement `permissionGuard` in `frontend/apps/dashboard/src/app/core/authz/permission.guard.ts` + `permission.guard.spec.ts`: `CanMatch` reading `route.data['requiredPermission']`; missing data key → deny (fail closed); redirect to the user's first permitted tenant page in sidebar order, falling back to `/tenant/select` (research R8); specs cover allow, deny+redirect, zero-permission fallback (depends on T017)
- [X] T019 [P] [US2] Implement `*appHasPermission` structural directive in `frontend/apps/dashboard/src/app/core/authz/has-permission.directive.ts` + `has-permission.directive.spec.ts` rendering content only when `PermissionsService.has()` is true, reactive to permission changes (FR-009) (depends on T017)
- [X] T020 [US2] Declare route permissions: add `requiredPermission` data (from `PAGE_PERMISSIONS`) + `permissionGuard` to every tenant page route in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` and to the platform area (`platform.admin`) in `frontend/apps/dashboard/src/app/app.routes.ts`; update `frontend/apps/dashboard/src/app/app.routes.spec.ts` (depends on T018)
- [X] T021 [US2] Filter sidebar navigation in `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` + `sidebar.component.spec.ts`: each nav item declares its permission from `PAGE_PERMISSIONS`; items (and emptied groups) hidden unless `PermissionsService.has()` passes; specs assert the exact visible set for Support Agent, Viewer, and Owner per `contracts/permissions.md` (FR-007/SC-002) (depends on T017)
- [X] T022 [US2] Wire 403-triggered permission refresh in `frontend/apps/dashboard/src/app/core/http/api-error.interceptor.ts` + `api-error.interceptor.spec.ts`: on 403 with code `unauthorized`, trigger `CurrentUserService.load()` re-fetch (guard re-evaluation then routes away from no-longer-permitted pages — FR-011 UI half, quickstart §5); ensure no refresh loop when `/me` itself is the 403 source (depends on T017; touches `frontend/apps/dashboard/src/app/core/tenant/current-user.service.ts` if a refresh method is needed)
- [X] T023 [US2] Verify: from `frontend/` run `pnpm ng test dashboard` and `pnpm ng build dashboard`; manually validate quickstart §3 scenarios (Support Agent nav set, Viewer settings deep-link redirect, Owner full nav)

**Checkpoint**: US1 + US2 independently functional — users see only allowed pages, server still the sole enforcer

---

## Phase 5: User Story 3 - Platform staff get role-appropriate access inside a tenant (Priority: P3)

**Goal**: Staff-in-tenant capabilities verified environment-aware — full access in non-production, least privilege in production (FR-005a)

**Independent Test**: `cargo test --test rbac` staff-matrix cases constructing the router with production vs non-production config; switch into a tenant as each platform role and confirm allow/deny per the production staff table in `contracts/permissions.md`

### Tests for User Story 3 (write first, must fail before implementation gaps close)

- [X] T024 [P] [US3] Extend `backend/crates/server/tests/rbac.rs` with the staff-in-tenant environment matrix: build the app once with `Environment::Production` config and once with `Environment::Development`; assert per `contracts/permissions.md` — Super Admin full tenant access in both; in production Support Engineer succeeds on `overview.view`-guarded `GET /tenant` but is denied any `settings.manage`-guarded operation, Sales/Finance denied all `.manage` operations; in non-production every platform role succeeds on every tenant operation

### Implementation for User Story 3

- [X] T025 [US3] Close any gaps T024 exposes in the staff permission path: environment plumbing through `TenancyConfig.is_production` in `backend/crates/server/src/router.rs` / `backend/crates/modules/tenancy/src/lib.rs`, and `staff_tenant_permissions` production rows in `backend/crates/modules/authz/src/matrix.rs` exactly matching the contract table
- [X] T026 [US3] Extend `/me` staff assertions in `backend/crates/server/tests/rbac.rs`: `staffTenantPermissions` reflects the environment (full set under non-production config, role-scoped set under production config) so the dashboard staff experience follows the environment with no frontend changes
- [X] T027 [P] [US3] Add staff-context specs to `frontend/apps/dashboard/src/app/core/authz/permissions.service.spec.ts`: platform user with active tenant resolves permissions from `staffTenantPermissions` (not memberships); sidebar spec case in `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.spec.ts` for a production-shaped Support Engineer payload (conversations/customers/knowledge-base visible, settings hidden)

**Checkpoint**: All three user stories independently functional

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Contract parity, quality gates, end-to-end validation

- [X] T028 [P] Add a catalog-parity unit test in `backend/crates/modules/authz/src/matrix.rs` (or `permission.rs`): the implemented permission codes exactly equal the 25 codes listed in `specs/008-rbac-permissions/contracts/permissions.md` (guards against silent drift between contract and code); verify frontend `permissions.ts` union compiles against the same list
- [X] T029 [P] Backend quality gates from `backend/`: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` — all green
- [X] T030 [P] Frontend quality gates from `frontend/`: `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` — all green
- [X] T031 Execute `specs/008-rbac-permissions/quickstart.md` manual scenarios end-to-end (§2 spot-checks, §3 per-role navigation, §4 staff-in-tenant with `ENVIRONMENT=development`, §5 immediate role change via SQL downgrade) and record results in the PR description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: T003/T004 after T001; T005 after T003+T004; T006 after T005 — BLOCKS all user stories
- **US1 (Phase 3)**: After Phase 2. T007 first (failing tests); T008 → T009 → T011; T010 parallel with T008/T009; T012 after T011; T013 last
- **US2 (Phase 4)**: Backend part (T014, T015) after US1's T011; frontend chain T016 → T017 → {T018, T019, T021, T022} → T020 → T023. T002 (Setup) is its only other prerequisite
- **US3 (Phase 5)**: After US1 (enforcement plumbing) — T024 first, then T025/T026; T027 after US2's T017
- **Polish (Phase 6)**: After all desired stories

### User Story Dependencies

- **US1 (P1)**: Only Foundational — independently testable via API alone
- **US2 (P2)**: Needs US1's route enforcement + `TenancyConfig` for the `/me` extension; UI value testable independently once `/me` serves permissions
- **US3 (P3)**: Needs US1's middleware path; frontend spec task additionally needs US2's PermissionsService

### Parallel Opportunities

- Phase 1: T001 ∥ T002
- Phase 2: T003 ∥ T004
- US1: T007 (tests) ∥ T010 while T008/T009 proceed
- US2: T014 ∥ T016 at phase start; T019 ∥ T021 ∥ T022 after T017
- US3: T024 ∥ T027
- Polish: T028 ∥ T029 ∥ T030
- With two developers after US1: one takes US2 frontend chain, the other US3 backend matrix tests

## Parallel Example: User Story 2

```bash
# After T017 (PermissionsService) lands, launch in parallel:
Task: "T019 *appHasPermission directive in core/authz/has-permission.directive.ts"
Task: "T021 Sidebar permission filtering in layout/sidebar/sidebar.component.ts"
Task: "T022 403-triggered refresh in core/http/api-error.interceptor.ts"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup → Phase 2 Foundational (catalog + matrix + guard)
2. Phase 3 US1: failing rbac.rs matrix → enforcement plumbing → matrix green
3. **STOP and VALIDATE**: `cargo test --test rbac` — the API is protected for all ten roles even with zero frontend changes
4. Deliverable security boundary; deploy/demo if ready

### Incremental Delivery

1. US1 → API enforcement verified (MVP)
2. US2 → `/me` permissions + full dashboard experience (nav, guards, action gating, 403 re-sync)
3. US3 → environment-aware staff access verified in both configurations
4. Polish → parity test, quality gates, quickstart run

---

## Notes

- Backend integration tests are live-gated (skip without a reachable `DATABASE_URL`) per the existing `tenancy.rs` harness — run them locally with the dev database up
- T011 changes middleware state types; expect compile-guided updates in `app()`/`app_with_test_routes()` and existing test builders
- Never introduce a frontend role→permission mapping — the frontend consumes `/me` arrays only (FR-010)
- Commit after each task or logical group; every checkpoint is a safe stopping point
