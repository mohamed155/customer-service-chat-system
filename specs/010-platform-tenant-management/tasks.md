# Tasks: Platform Tenant Management

**Input**: Design documents from `/specs/010-platform-tenant-management/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/rest-api.md, contracts/permissions.md, quickstart.md

**Tests**: Included — required by Constitution Principle VII and the platform's established pattern (every prior feature ships an rbac-matrix extension plus a dedicated live-gated integration suite; frontend specs accompany every new component/service/store).

**Organization**: Tasks are grouped by user story per spec.md. US1 (onboarding) is the MVP and ships a genuinely usable create-and-see-it-listed loop; US2 (find & inspect) upgrades the list into the full directory and adds the detail page; US3 (maintain & control) adds edit and activate/deactivate on top of the detail page US2 built.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 per spec.md
- Exact file paths in every description

## Path Conventions

Web app per plan.md: backend Cargo workspace at `backend/crates/`, migrations at `backend/migrations/`, Angular dashboard at `frontend/apps/dashboard/src/app/`.

---

## Phase 1: Setup

**Purpose**: Schema extension every backend task depends on

- [X] T001 Create `backend/migrations/0016_tenant_business_metadata.sql`: add `plan TEXT NOT NULL DEFAULT 'trial'` with `CHECK (plan IN ('trial','starter','professional','enterprise'))`, `contact_name TEXT NULL` with `CHECK (contact_name IS NULL OR length(contact_name) BETWEEN 1 AND 200)`, `contact_email TEXT NULL` to the `tenants` table, following the existing column/CHECK style in `0004_tenants.sql`; verify via `sqlx database reset -y` from `backend/` that it applies cleanly on top of `0001`–`0015`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared authz vocabulary, response models, and frontend route/nav plumbing every story depends on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T002 [P] Add `Permission::PlatformTenantsManage` to `backend/crates/modules/authz/src/permission.rs`: new enum variant with `#[serde(rename = "platform.tenants.manage")]`, add to the `Display` match and `Self::ALL` array (25→26; leave `Self::TENANT` unchanged — this is a platform-scope code), update the `catalog_parity_with_contract` unit test's expected list to 26 codes including `"platform.tenants.manage"`
- [X] T003 [P] Update `backend/crates/modules/authz/src/matrix.rs`: split the `PlatformRole::Support | PlatformRole::Sales => PLATFORM_TENANT_ACCESS` match arm in `platform_role_permissions` — Support gets a new `PLATFORM_SUPPORT` const (`PlatformTenantsList`, `PlatformTenantsSwitch`, `PlatformTenantsManage`), Sales keeps `PLATFORM_TENANT_ACCESS` unchanged; add `Permission::PlatformTenantsManage` to the `PLATFORM_ALL` const (SuperAdmin); do **not** touch `staff_tenant_permissions` — this permission is platform-scope only, not part of any tenant-scope set (depends on T002)
- [X] T004 Add response/request structs to `backend/crates/modules/tenancy/src/routes.rs`: `PlatformTenantDetail` (id, name, slug, status, plan, contact_name: Option\<String\>, contact_email: Option\<String\>, created_at, updated_at — camelCase serde) used by the create/detail/update handlers built in US1–US3; `CreateTenantRequest`/`UpdateTenantRequest` deserialize structs matching `contracts/rest-api.md` (depends on T001 for the underlying columns)
- [X] T005 [P] Add `'platform.tenants.manage'` to the `Permission` union in `frontend/apps/dashboard/src/app/core/authz/permissions.ts`
- [X] T006 Rebalance the platform area gate: in `frontend/apps/dashboard/src/app/app.routes.ts`, change the platform base route's `requiredPermission` from `PAGE_PERMISSIONS[APP_PATHS.platform.base]` (`platform.admin`) to `'platform.tenants.list'` (keep `canMatch: [areaAccessGuard, permissionGuard]`); in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`, give the `overview-placeholder` child route its own `canMatch: [permissionGuard]` + `data: { requiredPermission: 'platform.admin' }` so it keeps its current SuperAdmin-only gate now that the parent no longer enforces it (depends on T005)
- [X] T007 [P] Add tenant-management path segments to `frontend/apps/dashboard/src/app/core/router/app-paths.ts`: under `platform`, add `tenants: 'tenants'` and `newTenant: 'new'` (detail uses a routed `:id` param, no constant needed)
- [X] T008 [P] Add page-title entries to `frontend/apps/dashboard/src/app/core/router/page-title.ts`: extend `PageTitleKey` with `'platformTenants' | 'platformTenantDetail' | 'platformTenantNew'` and add matching `PAGE_TITLES` entries ("Tenants" / "Tenant details" / "New tenant", each with a short subtitle)
- [X] T009 Add a "Tenants" destination to `PLATFORM_DESTINATIONS` in `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`: `{ label: 'Tenants', path: '/${APP_PATHS.platform.base}/${APP_PATHS.platform.tenants}', permission: 'platform.tenants.list' }` (depends on T005, T007)
- [X] T010 [P] Extend `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`: `TenantPlan` string-literal union (`'trial' | 'starter' | 'professional' | 'enterprise'`); add `plan: TenantPlan` to `TenantSummary`; add `PlatformTenantDetail`, `CreateTenantPayload`, `UpdateTenantPayload` interfaces matching `contracts/rest-api.md`

**Checkpoint**: authz catalog/matrix, shared backend models, and frontend route/nav plumbing are in place — user story implementation can begin

---

## Phase 3: User Story 1 - Onboard a new customer organization (Priority: P1) 🎯 MVP

**Goal**: A Super Admin or Support Engineer can create a tenant from a routed form; it's Active, audited, appears via the API, and is immediately switchable-into. Ships with a minimal list page (no search/filter yet — that's US2) so the create flow has a real launch point and landing page.

**Independent Test**: `cargo test --test platform_tenants` create scenarios plus `POST /api/v1/platform/tenants` as each role; in the dashboard, sign in as Support Engineer, open Tenants → New tenant, submit, land back on the list showing the new Active tenant

### Implementation for User Story 1

- [X] T011 [US1] Implement `create_tenant` in `backend/crates/modules/tenancy/src/routes.rs`: validate name (1–200), slug (format + unique among live tenants → 409 `conflict` on collision), plan (optional, default `trial`), contact email (format, when present) → 422 `validation_failed` with per-field `ErrorDetail` on any failure; insert with `status='active'`; call `tenancy::audit::record(pool, "platform.tenant_created", Some(principal.user_id), None, "tenant", Some(&id.to_string()), &json!({"name":..,"slug":..,"plan":..}))`; return 201 `PlatformTenantDetail` (depends on T004)
- [X] T012 [US1] Register `POST /platform/tenants` in `backend/crates/server/src/router.rs`: add to `platform_routes()` via `.guarded("/platform/tenants", routing::post(tenancy::routes::create_tenant), Permission::PlatformTenantsManage)` (depends on T011)
- [X] T013 [US1] Extend `backend/crates/server/tests/rbac.rs`'s role×operation matrix with the new create endpoint: Super Admin and Support Engineer allow, Developer/Sales/Finance deny (403), every tenant role deny (403), anonymous deny (401) — add to the existing `PLATFORM_OPERATIONS`-style tables and the deny-by-default sweep (depends on T012)
- [X] T014 [P] [US1] Create `backend/crates/server/tests/platform_tenants.rs` (live-gated, seed helpers copied from `tenancy.rs`/`rbac.rs` patterns): create success asserts `status='active'`, `plan='trial'` when omitted, and `GET /platform/tenants` lists it; validation failures for missing name, malformed slug, duplicate live slug (409), malformed contact email; audit row assertion for `platform.tenant_created` (actor, tenant id); switch-into-new-tenant succeeds (depends on T012)
- [X] T015 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/tenants/platform-tenants.service.ts` + spec: `list(params: { q?, status?, cursor? }): Observable<PaginatedResponse<TenantSummary>>` and `create(payload: CreateTenantPayload): Observable<PlatformTenantDetail>`, both via the existing `ApiService` (`list()`/`post()`) — no `firstValueFrom`, pure Observable returns per constitution v1.2.0 (depends on T010)
- [X] T016 [US1] Create `frontend/apps/dashboard/src/app/features/platform/tenants/tenants.store.ts` + spec: `signalStore` with `items`/`loading`/`error` state; `load` as an `rxMethod` calling `platform-tenants.service.list()`; `create` as an `rxMethod` calling `.create()` that on success reloads `items` (US2 adds search/filter/pagination to this store — keep the shape extensible) (depends on T015)
- [X] T017 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/tenants/tenant-form.component.ts` + spec: typed Reactive Form (name, slug, plan `<select>` defaulting to Trial, contactName, contactEmail) with client validators mirroring the backend rules (name required ≤200, slug `^[a-z0-9](-?[a-z0-9])*$` ≤63, email format when present); create-mode submit calls `tenants.store.create()`, maps server `ErrorDetail[]` (422) and slug `conflict` (409) onto the matching form controls, and on success navigates to `/${APP_PATHS.platform.base}/${APP_PATHS.platform.tenants}` (depends on T016)
- [X] T018 [US1] Create `frontend/apps/dashboard/src/app/features/platform/tenants/tenant-list.component.ts` + spec: `app-page-container` + `app-page-header` ("Tenants"), `app-data-table` rendering name/slug/`app-status-badge` (tone `green` for active, `neutral` for suspended) per row, a "New tenant" link to the new-tenant route shown only via `*appHasPermission="'platform.tenants.manage'"`, `app-loading-state` while `tenants.store.loading()`, `app-empty-state` when `items()` is empty (depends on T016)
- [X] T019 [US1] Wire routes in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`: add `{ path: APP_PATHS.platform.tenants, canMatch: [permissionGuard], data: { pageTitle: 'platformTenants', requiredPermission: 'platform.tenants.list' }, loadComponent: () => TenantListComponent }` and `{ path: '${APP_PATHS.platform.tenants}/${APP_PATHS.platform.newTenant}', canMatch: [permissionGuard], data: { pageTitle: 'platformTenantNew', requiredPermission: 'platform.tenants.list' }, loadComponent: () => TenantFormComponent }` (route reachable to all platform roles; the create submit itself stays enforced server-side by `platform.tenants.manage`, and the list page hides the entry button from non-managers per T018) (depends on T017, T018, T009)

**Checkpoint**: US1 fully functional — onboarding works end to end (API → audit → directory → switchable); MVP deliverable

---

## Phase 4: User Story 2 - Find and inspect customer organizations (Priority: P2)

**Goal**: The list page becomes the real directory (search + status filter + pagination) and a detail page shows a tenant's full record.

**Independent Test**: Seed a mixed set of Active/Suspended tenants (`>1` page); verify search matches name/slug, status filter narrows correctly, "Load more" traverses without gaps/duplicates, and the detail page renders the correct record; verify tenant users get 403/redirected on both surfaces

### Implementation for User Story 2

- [X] T020 [US2] Extend `list_tenants` in `backend/crates/modules/tenancy/src/routes.rs`: add optional `status` query param (validate `active`/`suspended`, else 422 `validation_failed`), combine with the existing `q` and cursor filtering in one SQL statement; add `plan` to the `TenantSummary` struct and its `SELECT`/row-mapping (depends on T004, T011)
- [X] T021 [US2] Implement `get_tenant` (detail) in `backend/crates/modules/tenancy/src/routes.rs`: fetch by id excluding soft-deleted rows, return `PlatformTenantDetail`, 404 `not_found` when missing (depends on T004)
- [X] T022 [US2] Register `GET /platform/tenants/{id}` in `backend/crates/server/src/router.rs` requiring `Permission::PlatformTenantsList` (not `.manage` — viewing is open to all platform roles). Note: `{id}` will also carry PATCH (`.manage`) once US3 lands — build the two method routers for this path with their own `route_layer(require_permission(..))` via axum's per-`MethodRouter` `.route_layer()` before `.merge()`ing them into a single `.route("/platform/tenants/{id}", merged)` call, since the existing `.guarded()` helper only attaches one permission per path (depends on T021)
- [X] T023 [US2] Extend `backend/crates/server/tests/rbac.rs`: `GET /platform/tenants/{id}` allowed for all five platform roles, denied for tenant roles; add a `status` filter validation case (bad value → 422) to the deny/validation coverage (depends on T022)
- [X] T024 [P] [US2] Extend `backend/crates/server/tests/platform_tenants.rs`: status filter returns exactly the matching subset alone and combined with `q`; seed >1 page and assert `next_cursor`/`has_more` traversal hits every seeded tenant exactly once; detail 200 for a live tenant and 404 for an unknown/soft-deleted id (depends on T022)
- [X] T025 [US2] Extend `tenants.store.ts` (`frontend/apps/dashboard/src/app/features/platform/tenants/tenants.store.ts`) + spec: add `query` and `statusFilter` state; an `rxMethod` pipeline (`debounceTime(300)`, `distinctUntilChanged()`, `switchMap`) that resets `items`/cursor and reloads on query or filter change; `loadMore()` appending the next page via the existing cursor (depends on T016, T020)
- [X] T026 [US2] Extend `tenant-list.component.ts` + spec: add `app-search-input` bound to `store.query`, a status filter control (All/Active/Suspended), a "Load more" button visible while `hasMore()`, and empty-state copy that reflects an active search/filter (depends on T025, T018)
- [X] T027 [P] [US2] Create `frontend/apps/dashboard/src/app/features/platform/tenants/tenant-detail.component.ts` + spec: `app-page-container`/`app-page-header` ("Tenant details"), fields name/slug/`app-status-badge`/plan/contact name & email (em-dash when absent)/created & updated dates, loaded via `platform-tenants.service` extended with a `get(id)` Observable method (add alongside `list`/`create` in `platform-tenants.service.ts`) (depends on T010, T015)
- [X] T028 [US2] Wire the detail route in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`: `{ path: '${APP_PATHS.platform.tenants}/:id', canMatch: [permissionGuard], data: { pageTitle: 'platformTenantDetail', requiredPermission: 'platform.tenants.list' }, loadComponent: () => TenantDetailComponent }`; make each `tenant-list.component.ts` row a `routerLink` to its detail page (depends on T027, T026)

**Checkpoint**: US1 + US2 independently functional — the directory is a real management surface; server remains the enforcement boundary

---

## Phase 5: User Story 3 - Maintain and control a customer organization (Priority: P3)

**Goal**: Managers can edit a tenant's record and activate/deactivate it from the detail page, with immediate member-facing effect and full audit coverage.

**Independent Test**: Edit name/slug/plan/contact and verify persistence + validation; deactivate a tenant with a signed-in member and verify their very next request is refused while staff retain visibility; verify every change (including the DB-trigger-written slug audit) is recorded

### Implementation for User Story 3

- [X] T029 [US3] Implement `update_tenant` in `backend/crates/modules/tenancy/src/routes.rs`: partial update accepting any subset of `name`/`slug`/`plan`/`contactName`/`contactEmail`/`status`; validate only provided fields (same rules as create; slug collision → 409); run the whole operation in **one DB transaction** that first executes `SELECT set_audit_actor($1)` with `principal.user_id`, then the `UPDATE` (this is mandatory — the existing `tenants_slug_change_audit` trigger from migration `0015` raises an exception on any slug UPDATE without a transaction-local actor set), then commits; after commit, call `audit::record` with `"platform.tenant_updated"` for non-slug field changes (details: old/new per changed field, slug excluded — the trigger already wrote `tenant.slug_changed`) and/or `"platform.tenant_status_changed"` for a status change (details: old_status/new_status) — a PATCH touching both emits both; 404 for unknown/soft-deleted id (depends on T004, T021)
- [X] T030 [US3] Register `PATCH /platform/tenants/{id}` in `backend/crates/server/src/router.rs`: merge into the same `{id}` path built in T022, layering `require_permission(Permission::PlatformTenantsManage)` on the PATCH `MethodRouter` before merging with the GET one (depends on T022, T029)
- [X] T031 [US3] Extend `backend/crates/server/tests/rbac.rs`: `PATCH /platform/tenants/{id}` allowed for Super Admin and Support Engineer, denied for Developer/Sales/Finance and every tenant role (depends on T030)
- [X] T032 [P] [US3] Extend `backend/crates/server/tests/platform_tenants.rs`: edit success for name/plan/contact fields persists and is reflected on the next `GET`; slug edit succeeds and `SELECT * FROM audit_logs WHERE action='tenant.slug_changed' AND resource_id=$1` returns a row with the correct actor (proves the `set_audit_actor` transaction contract); slug collision → 409, nothing changed; status change to `suspended` → a signed-in member's next `GET /api/v1/tenant` (with `X-Tenant-ID`) is refused, reactivation restores it on the next request; `platform.tenant_updated`/`platform.tenant_status_changed` audit rows assert actor/old/new values; two sequential PATCHes on the same tenant both land and both audit (depends on T030)
- [X] T033 [US3] Add `update(id: string, payload: UpdateTenantPayload): Observable<PlatformTenantDetail>` to `frontend/apps/dashboard/src/app/features/platform/tenants/platform-tenants.service.ts` + spec case (depends on T015)
- [X] T034 [US3] Extend `tenant-form.component.ts` + spec with an edit mode: accepts an initial `PlatformTenantDetail` input, pre-fills the form, submits via `platform-tenants.service.update()` (through the store or directly — keep symmetry with create), same error-mapping behavior including 409 on slug conflict (depends on T017, T033)
- [X] T035 [US3] Extend `tenant-detail.component.ts` + spec: an "Edit" entry point (routes to `tenant-form.component.ts` in edit mode, e.g. `/platform/tenants/:id/edit`) and an "Activate"/"Deactivate" action with a confirmation step before calling `update()` with the new `status` — both gated by `*appHasPermission="'platform.tenants.manage'"` so Developer/Sales/Finance see the record read-only (depends on T027, T034)
- [X] T036 [US3] Add the edit route in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`: `{ path: '${APP_PATHS.platform.tenants}/:id/edit', canMatch: [permissionGuard], data: { pageTitle: 'platformTenantDetail', requiredPermission: 'platform.tenants.list' }, loadComponent: () => TenantFormComponent }` (route reachable; submit enforced server-side by `.manage`) (depends on T034, T035)

**Checkpoint**: All three user stories independently functional — full tenant lifecycle management is live

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates and end-to-end validation

- [X] T037 [P] Backend quality gates from `backend/`: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` — all green, including the extended `rbac.rs` matrix, updated `catalog_parity_with_contract`, and the new `platform_tenants.rs` suite
- [X] T038 [P] Frontend quality gates from `frontend/`: `pnpm ng build dashboard` (watch the initial-bundle budget — the 009 feature already sits at the raised 600kb warning threshold; a large new feature area may need lazy-loading verification), `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` — all green
- [X] T039 Execute `specs/010-platform-tenant-management/quickstart.md` §1–§5 end to end (onboarding, directory search/filter/pagination, edit/status-change with member-refusal timing, audit SQL spot-checks, regression checks against 008/009 behaviors) and record results in the PR description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies
- **Foundational (Phase 2)**: T002/T003 after nothing (parallel); T004 after T001; T005→T006; T007/T008 independent; T009 after T005+T007; T010 independent — BLOCKS all user stories
- **US1 (Phase 3)**: After Phase 2. Backend chain T011→T012→{T013,T014}; frontend chain T015→T016→{T017,T018}→T019 (T019 also needs T009)
- **US2 (Phase 4)**: After US1's T004/T011 (backend) and T009/T016/T018 (frontend). Backend: T020 ∥ T021→T022→{T023,T024}. Frontend: T025→T026; T027 (needs T015) → T028
- **US3 (Phase 5)**: After US2's T022 (shares the `{id}` path) and T021 (struct/handler pattern). Backend: T029→T030→{T031,T032}. Frontend: T033 ∥ T034(needs T017)→T035(needs T027)→T036
- **Polish (Phase 6)**: After all desired stories

### User Story Dependencies

- **US1 (P1)**: Only Foundational — independently testable via API + a minimal list/create UI loop
- **US2 (P2)**: Needs US1's `PlatformTenantDetail`/`TenantSummary` plumbing and the list page/store/route shell US1 built; independently testable once its own endpoints/pages land
- **US3 (P3)**: Needs US2's detail page and the shared `{id}` route (GET+PATCH merge point); independently testable via API alone even before the edit UI lands

### Parallel Opportunities

- Foundational: T002 ∥ T003 ∥ T005 ∥ T007 ∥ T008 ∥ T010
- US1: T014 ∥ (T015 → T016 → {T017 ∥ T018})
- US2: T024 ∥ T027; T020 can run alongside T021
- US3: T032 ∥ T033
- Polish: T037 ∥ T038

## Parallel Example: Foundational

```bash
# Launch together at the start of Phase 2:
Task: "T002 Add PlatformTenantsManage to authz/src/permission.rs"
Task: "T003 Update authz/src/matrix.rs Support/SuperAdmin grants"
Task: "T005 Add 'platform.tenants.manage' to core/authz/permissions.ts"
Task: "T007 Add tenants/newTenant path segments to core/router/app-paths.ts"
Task: "T008 Add platform tenant page-title entries"
Task: "T010 Extend core/api/tenant-api.models.ts"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup → Phase 2 Foundational (authz + models + route/nav plumbing)
2. Phase 3 US1: create endpoint → minimal list page → create form
3. **STOP and VALIDATE**: `cargo test --test platform_tenants` + sign in as Support Engineer and onboard a tenant end to end
4. Deliverable: onboarding works; deploy/demo if ready

### Incremental Delivery

1. US1 → onboarding (MVP)
2. US2 → directory becomes searchable/filterable/paginated; detail page added
3. US3 → edit + activate/deactivate on the detail page
4. Polish → quality gates, quickstart run, audit spot-checks

---

## Notes

- The `{id}` path carries two permissions on two methods (GET `platform.tenants.list`, PATCH `platform.tenants.manage`) — T022/T030 call this out explicitly since the existing `.guarded()` router helper only attaches one permission per `.route()` call.
- The `tenants_slug_change_audit` DB trigger (migration 0015) is non-negotiable: any slug UPDATE without a preceding `set_audit_actor()` in the same transaction will raise and roll back — T029 is written around this constraint, not against it.
- Commit after each task or logical group; every checkpoint is a safe stopping point.

## Phase 7: Convergence

- [X] T040 CRITICAL provide `TenantsStore` at root or the platform tenant route/component boundary and add a routed component test that resolves the real store without a test-only provider per T016 and US1/AC1 (missing)
- [X] T041 CRITICAL reorder `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` so `tenants/new` and `tenants/:id/edit` precede `tenants/:id`, and add route-order coverage proving `/platform/tenants/new` loads `TenantFormComponent` in create mode per T019 and SC-001 (contradicts)
- [X] T042 CRITICAL make tenant creation and PATCH persistence plus required app-level audits atomic, move the PATCH old-row read inside the transaction with appropriate row locking, preserve trigger-owned slug auditing, and add rollback/concurrency tests per FR-009, the concurrent-edit edge case, and Constitution III (partial)
- [X] T043 return HTTP 422 with code `validation_failed` and field details for tenant-management validation failures, updating the kernel constructor or adding a dedicated constructor plus contract tests per FR-002, FR-004, FR-011, and `contracts/rest-api.md` (contradicts)
- [X] T044 reject uppercase tenant slugs instead of lowercasing them before validation in create and update handlers, with create/PATCH regression tests per FR-002, FR-004, US1/AC2, and US3/AC2 (contradicts)
- [X] T045 replace `firstValueFrom`, Promise-returning store writes, and async component submit/status flows in `features/platform/tenants/` with Observable operator composition and `rxMethod` write pipelines per Constitution frontend asynchronous requirements and T016/T033 (contradicts)
- [X] T046 render exact backend `ErrorDetail.message` values beside matching tenant form controls, map slug conflicts onto the slug control, add `aria-invalid` and error-description relationships, and test rendered 409/422 messages per FR-011 and T017/T034 (partial)
- [X] T047 complete backend tenant-management coverage with API-based create-then-list verification, standalone status filtering, anonymous detail denial, all tenant-role create denial and deny-by-default sweep coverage, PATCH validation/mixed-audit cases, and soft-deleted slug reuse per T014, T023, T024, T032, and Constitution VII (partial)
- [X] T048 restore `app-page-container` around the tenant detail page and make tenant directory rows semantic routed links with keyboard and browser link behavior tests per T027, T028, and Constitution IX (partial)
- [X] T049 support the specified initial `PlatformTenantDetail` edit input while retaining routed loading, and render a non-submittable loading/error/retry state when edit initialization fails per T034 and FR-011 (partial)
- [X] T050 add a `platform.tenants.manage`-gated create action inside the unfiltered tenant-directory empty state per US2/AC5 and FR-011 (partial)
- [X] T051 eliminate duplicate initial and clear-filter list requests caused by the component search effect plus store load, and add request-count tests around debounce/filter reset behavior per T025/T026 (partial)
- [X] T052 make `pnpm format:check` green, execute `specs/010-platform-tenant-management/quickstart.md` sections 1-5 with podman-backed services, and record the automated/manual results for the completion report or PR per T038/T039 (partial)

## Phase 8: Convergence

- [X] T053 CRITICAL fix the reproducible CSRF test failure, run the complete backend `cargo test` gate with no failures or skipped feature suites, and correct `quickstart-results.md` so a failing gate is not labeled PASS per T037 and Constitution VII (contradicts)
- [X] T054 execute and record the real signed-in tenant-member next-request refusal after suspension and next-request recovery after reactivation, using an authenticated member context rather than inferred API/unit evidence per T039 and SC-005 (partial)
- [X] T055 run and record the dashboard E2E onboarding, directory search plus Suspended-filter intersection, pagination, deep-link refusal/no-content-flash, edit/status controls, view-only action absence, and Support `/platform` landing scenarios, including `pnpm test:e2e`, per T039/T052 and SC-001 through SC-006 (partial)
- [X] T056 preserve absent-versus-null PATCH field semantics, reject null or blank name/slug/plan/status, allow contact clearing only through explicit JSON null, and add field-level 422 regression tests per FR-004, T029, and T043 (partial)
- [X] T057 default only an omitted create plan and reject supplied blank plan/contact metadata with field-level 422 errors and regression tests per FR-002, T011, and T043 (partial)
- [X] T058 validate tenant and contact name limits by Unicode character count consistently with PostgreSQL `length()`, adding multibyte boundary tests for create and update per FR-002/FR-004 and the data model (partial)
- [X] T059 accumulate all semantic field validation failures into one 422 `validation_failed` response and distinguish malformed JSON from valid JSON with unknown or wrong-typed fields so contracted field details are returned per FR-011 and `contracts/rest-api.md` (partial)
- [X] T060 prevent Developer, Sales, and Finance users from rendering enabled create/edit management forms on direct navigation by adding a manage-permission form gate or stronger route guard, with direct-deep-link tests per FR-008 and SC-006 (contradicts)
- [X] T061 derive tenant edit mode and update identity from a supplied initial `PlatformTenantDetail` when no route parameter exists, and test initial-input-only update submission per T049/T034 (partial)
- [X] T062 render the exact server validation message and matching ARIA error element for the plan control, with a 422 plan-error accessibility test per T046 and FR-011 (partial)
- [X] T063 replace shared `ReplaySubject`/`switchMap` write bridges and manual component subscriptions with lifecycle-safe RxJS operator pipelines using explicit ordered write concurrency, and test repeated writes, completion/error delivery, ordering, and exactly one HTTP request per invocation per T045 and Constitution frontend async requirements (contradicts)
- [X] T064 add one atomic query/status reset operation for Clear filters and assert exactly one resulting list request rather than permitting two per T051 (partial)
- [X] T065 verify T001-T039 against the current implementation and quality evidence, then mark each genuinely completed original checklist item `[X]` before reporting all tasks complete per the implementation completion contract (contradicts)
- [X] T066 rerun `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm test:e2e`, `pnpm lint`, and `pnpm format:check` against the final current tree, then update `quickstart-results.md` to match the actual source and unqualified outcomes per T038/T052 (partial)

## Phase 9: Convergence

- [X] T067 CRITICAL add Playwright coverage for tenant onboarding, combined search/status filtering, pagination, tenant-user deep-link refusal without content flash, edit/status controls, view-only action absence, and Support `/platform` landing; run the configured `pnpm test:e2e` gate and correct `quickstart-results.md` with the actual outcome per Constitution VII, SC-001 through SC-006, T055, and T066 (contradicts)
- [X] T068 execute and accurately record a signed-in tenant member's successful request before suspension, immediate refusal on the next request after suspension, and immediate recovery after reactivation; remove the contradictory single-actor PASS claim from `quickstart-results.md` per FR-005, SC-005, T039, and T054 (partial)
- [X] T069 reject blank `contactName` and `contactEmail` values on PATCH with field-level 422 errors, preserve absent fields unchanged, permit clearing only through explicit JSON `null`, and replace the contradictory blank-clears regression test per T056 (contradicts)
- [X] T070 replace the remaining manual subscriptions in the platform tenant form/detail components with lifecycle-safe RxJS operator or rxMethod pipelines while preserving ordered writes, error delivery, navigation, retries, and status updates through regression tests per T063 (partial)

## Phase 10: Convergence

- [X] T071 CRITICAL correct the stale platform-destination Playwright assertion to match the intended `/platform` landing behavior, run the complete `pnpm test:e2e` suite with zero failures, add the E2E gate and actual counts to `quickstart-results.md`, and remove its false claim that E2E is unconfigured before reporting all gates green per Constitution VII, T066, and T067 (contradicts)
- [X] T072 extend `frontend/e2e/platform-tenant-management.spec.ts` to assert combined search plus Suspended filtering, tenant-user deep-link refusal without any tenant-management content flash during delayed identity resolution, actual edit and deactivate/reactivate interactions with request/result assertions, and Support navigation from `/platform` per T067, SC-002, FR-007, and the specified edge cases (partial)
- [X] T073 give the Playwright Support identity `platform.tenants.manage`, verify Support sees and can use create/edit/status controls, and keep management actions absent for Developer, Sales, and Finance per FR-008, SC-006, and T067 (contradicts)

## Phase 11: Convergence

- [X] T074 add a representative 500-tenant pagination verification that proves complete traversal without duplicates or gaps and records comparative response/render timing evidence per SC-002 (partial)
- [X] T075 strengthen the delayed-identity deep-link Playwright scenario to assert that tenant-management headings, tenant records, detail content, and management controls never render before refusal and redirect per FR-007, US2/AC6, the deep-link edge case, and T072 (partial)
- [X] T076 measure the tenant onboarding Playwright flow from form start through successful list return and assert the one-minute completion threshold per SC-001 (partial)
- [X] T077 add UI authorization matrix coverage for all ten platform and tenant roles, verifying tenant-management visibility and refusal behavior for each role per SC-003 (partial)
- [X] T078 exercise search, status filtering, and tenant-detail inspection separately as Developer, Sales, and Finance while confirming management actions remain unavailable per SC-006 (partial)
- [X] T079 execute and accurately record an independently signed-in tenant member's successful request before suspension, immediate refusal after suspension, and immediate recovery after reactivation without presenting integration-test evidence as a separate manual run per US3/AC3-4 and SC-005 (partial)
- [X] T080 return and assert HTTP 201 from the Playwright tenant-onboarding mock so browser coverage matches the create-tenant REST contract per contracts/rest-api.md (unrequested)
- [X] T081 update `quickstart-results.md` to describe the current queued Subject plus `rxMethod`/`concatMap` tenant-store write implementation rather than the obsolete duplicate-service-observable design per plan: frontend async/state decision (contradicts)
- [X] T082 correct the CSRF note in `quickstart-results.md` to describe the current scoped-router path handling rather than claiming `OriginalUri` is used per plan: backend routing decision (contradicts)

## Phase 12: Convergence

- [X] T083 add Playwright tenant-management authorization coverage for Owner, Admin, Manager, Agent, and Viewer so the UI refusal matrix exercises all ten specified roles rather than substituting `noRole` for tenant roles per T077, SC-003, and FR-007 (missing)
- [X] T084 rerun the complete backend, frontend unit, build, lint, format, and Playwright gates against the current tree and update `quickstart-results.md` from 26 to 34 E2E tests, from 66 to 67 `platform_tenants.rs` tests, and to the actual current tenancy/auth counts before claiming all gates green per Constitution VII (contradicts)
- [X] T085 add and record a frontend 500-tenant directory render-time comparison against another dashboard page while preserving T074's complete cursor traversal evidence per T074, SC-002, and Constitution X (partial)
- [X] T086 replace sampled delayed-identity assertions with continuous DOM mutation observation and cover list, detail, create, and edit deep links so any transient tenant-management content render fails the Playwright scenario per T075, FR-007, and the deep-link edge case (partial)
- [X] T087 record the measured onboarding duration, mocked-browser scope, threshold, and reproducible command in `quickstart-results.md` without presenting it as production latency evidence per T076 and SC-001 (partial)
- [X] T088 refresh the execution date and source-tree metadata in `quickstart-results.md` so the evidence identifies the current verified implementation rather than stale commit/worktree descriptions per Constitution VII (contradicts)
- [X] T089 execute T074 with a reachable PostgreSQL instance, prove the live-gated suite did not skip, and record the 505-row traversal count, page count, elapsed time, and command output in `quickstart-results.md` per T074 and Constitution VII (partial)

## Phase 13: Convergence

- [X] T090 install a `MutationObserver` before Angular bootstrap in each delayed-identity Playwright scenario, retain every matching protected-content mutation, and fail after redirect if list, detail, create, or edit tenant-management content appeared transiently per T086, FR-007, and the deep-link edge case (missing)
- [X] T091 measure the 500-tenant directory and a representative lightweight dashboard page under equivalent mocked conditions, assert and log their comparative render times, and record the result in `quickstart-results.md` per T085, SC-002, and Constitution X (missing)
- [X] T092 instrument T074 to emit the observed 505-row count, cursor page count, and elapsed duration, execute it against reachable PostgreSQL without a live-gate skip, and record the command plus output in `quickstart-results.md` per T089 and Constitution VII (missing)
- [X] T093 replace T074's truncating `elapsed.as_secs() < 5` assertion with a direct `Duration::from_secs(5)` comparison and preserve a useful failure message per T074 and SC-002 (partial)
- [X] T094 replace stale commit and generic uncommitted-change labels in `quickstart-results.md` with an unambiguous current source-snapshot identifier and remove obsolete working-tree wording per T088 and Constitution VII (contradicts)
- [X] T095 exercise list, detail, create, and edit deep-link refusal for Owner, Admin, Manager, Agent, and Viewer, combining the ten-role matrix with continuous no-content-flash evidence per T083, SC-003, and FR-007 (partial)

## Phase 14: Convergence

- [X] T096 validate create and update slugs exactly as supplied against `^[a-z0-9](-?[a-z0-9])*$`, returning field-level 422 errors for trailing hyphens or surrounding whitespace and adding no-mutation regressions per FR-002 and FR-004 (contradicts)
- [X] T097 observe added text nodes and character-data changes with precise protected-content selectors in the pre-bootstrap Playwright observer, then rerun list, detail, create, and edit refusal coverage for every tenant role per FR-007 and SC-003 (partial)
- [X] T098 compare the 500-tenant directory with a representative lightweight dashboard page under equivalent mocked conditions, assert a defined comparative threshold, and record both measurements and the reproducible command per SC-002 and T091 (partial)
- [X] T099 execute the instrumented 505-row pagination test against reachable PostgreSQL without a live-gate skip and record the exact command, row count, page count, duration, and emitted output per T092 and Constitution VII (partial)
- [X] T100 replace stale commit labels with one unambiguous current source-snapshot identifier and either execute the specified live browser workflows or qualify API, mocked-browser, and integration evidence without unqualified PASS claims per T039, T094, and Constitution VII (contradicts)
- [X] T101 preserve omitted-versus-null semantics for create `plan`, default only an omitted field, reject explicit JSON null with a field-level 422 error, and assert no tenant or audit row is created per FR-002 and T057 (partial)
- [X] T102 assert that Platform and Tenants navigation is absent for Owner, Admin, Manager, Agent, and Viewer in addition to direct-link refusal per FR-007 and SC-003 (partial)
- [X] T103 return the direct tenant detail and PATCH response bodies from the management-control Playwright mocks and assert the actual name, status, edit link, and status-action behavior per FR-003 and the REST contract (contradicts)
- [X] T104 add tenant-detail PATCH with a harmless valid body to the RBAC deny-by-default operation inventory so the generic fail-closed sweep covers the management write route per T047 and Constitution VII (partial)

## Phase 15: Convergence

- [X] T105 cancel or invalidate pending debounced tenant-search emissions when filters are reset, and add a fake-timer regression proving type-then-immediate-clear leaves a blank query and exactly one final unfiltered request per FR-001, SC-002, and the search/filter/pagination edge case (partial)
- [X] T106 replace the one-response 500-tenant browser benchmark with realistic 25-row cursor pagination, compare it against a defined representative lightweight dashboard page under equivalent conditions with an asserted relative threshold, and regenerate `quickstart-results.md` with exact commands, emitted measurements, current source references, and qualified evidence per SC-002, T091, T098, T100, and Constitution VII/X (contradicts)
- [X] T107 serialize rapid tenant status intents from the latest committed status, preserve ordered result delivery, and add tests proving every transition is applied/audited in order and the final state matches the last action per FR-005 and the rapid repeated status-toggling edge case (partial)
- [X] T108 separate tenant-detail load failures from activate/deactivate action failures so a failed write preserves the loaded record and status, renders accessible action-specific feedback, and supports retry per FR-011 (partial)
- [X] T109 add Playwright coverage for create and edit 409/422 responses plus no-match search/filter states, asserting exact inline server messages, `aria-invalid`/`aria-describedby`, no navigation or mutation, Clear filters, and manager-only empty-state creation per FR-011, US1/AC2, US2/AC5, US3/AC2, and Constitution VII (partial)
- [X] T110 validate create/PATCH plan and PATCH status values exactly as supplied without trimming, reject surrounding whitespace with field-level 422 responses, and add no-mutation regressions per FR-002, FR-004, and `contracts/rest-api.md` (partial)
- [X] T111 centralize bounded contact-email validation for create and PATCH, reject control/whitespace characters and malformed mailbox/domain structures consistently, and add representative field-level 422 regressions per FR-002 and FR-004 (partial)
- [X] T112 replace native `window.confirm` status confirmation with the established accessible dialog pattern, including labelled title/description, focus trap/restoration, Escape/cancel behavior, destructive-action semantics, and keyboard tests per T035 and Constitution IX (partial)
- [X] T113 extend the explicit-null create-plan regression to assert that neither a tenant row nor a `platform.tenant_created` audit row is written per T101, FR-009, and Constitution VII (partial)

## Phase 16: Convergence

- [X] T114 CRITICAL add a migration-backed index strategy supporting the production tenant-directory search/status/cursor query, with query-plan and 500-tenant regressions, or explicitly resolve and govern the constitutional deviation per Constitution VIII and FR-001 (contradicts)
- [X] T115 strengthen the centralized bounded contact-email validator to reject control/whitespace characters and malformed mailbox/domain dot structures consistently on create and PATCH, with representative field-level 422 and no-mutation regressions per T111, FR-002, and FR-004 (partial)
- [X] T116 replace the handwritten incomplete status-confirmation behavior with the established accessible dialog pattern, including initial focus, focus trap, focus restoration, reliable Escape/cancel handling, non-interactive backdrop semantics, and keyboard tests per T112 and Constitution IX (partial)
- [X] T117 remove obsolete `window.confirm` expectations and test the actual dialog open, cancel, confirm, Escape, focus, and status-request flow per T112 and Constitution VII (contradicts)
- [X] T118 add the fake-timer type-then-immediate-clear regression proving the pending debounced search is invalidated, query remains blank, and exactly one final unfiltered request occurs per T105 and SC-002 (partial)
- [X] T119 serialize rapid tenant status intents from the latest committed server state without `switchMap` cancellation, preserving every transition/result in order and testing the final state plus ordered requests/audits per T107, FR-005, and the rapid repeated status-toggling edge case (partial)
- [X] T120 preserve already-loaded tenant rows, query, and status filter when load-more fails, render accessible pagination-specific feedback, and retry the failed cursor rather than resetting the directory per FR-011 (partial)
- [X] T121 render accessible detail-load feedback using the available server error while keeping load retry separate from action-specific failures per T108 and FR-011 (partial)
- [X] T122 complete Playwright create and edit 409/422 coverage with exact inline messages, `aria-invalid`/`aria-describedby`, no navigation or mutation, Clear filters, and manager-only empty-state creation assertions per T109, US1/AC2, US2/AC5, US3/AC2, and Constitution VII (partial)
- [X] T123 align the no-match Playwright assertion with the intended tenant-directory empty-state copy and verify the clear-filter recovery behavior per T109 and US2/AC5 (contradicts)
- [X] T124 make the 505-tenant browser benchmark assert 25-row cursor pages, exactly 21 pages, complete unique traversal without gaps or duplicates, and a defined relative threshold against a representative lightweight dashboard page under equivalent mocked conditions per T106 and SC-002 (partial)
- [X] T125 regenerate `quickstart-results.md` from the current source snapshot with consistent test counts, exact commands and emitted measurements, qualified mocked/integration evidence, and no unsupported T105/T106/T112 completion claims per T100 and Constitution VII (contradicts)

## Phase 17: Convergence

- [X] T126 CRITICAL update the migration inventory and index assertions in `backend/crates/shared/db/tests/schema.rs` for migration 0017 and both tenant-directory indexes, then execute the live schema suite against migrated PostgreSQL per T114 and Constitution VII/VIII (contradicts)
- [X] T127 CRITICAL make the 505-tenant browser benchmark assert `limit=25` on all requests, exactly 21 pages, the exact gap-free 1–505 sequence, and a defined relative threshold against a representative lightweight dashboard page under equivalent mocked conditions per T124, SC-002, and Constitution X (partial)
- [X] T128 add migration-backed `EXPLAIN (FORMAT JSON)` regressions for cursor-only, status-plus-cursor, search, and combined tenant-directory query shapes, validating the production index strategy and correcting index key order if the plans expose an avoidable scan or sort per T114 and Constitution VIII/X (missing)
- [X] T129 reject all control/whitespace characters and invalid local-part characters in the shared contact-email validator, then add create and PATCH API regressions asserting field-level 422 responses, unchanged data, and no audit writes per T115, FR-002, FR-004, and FR-011 (partial)
- [X] T130 replace the handwritten tenant-status overlay behavior with the established accessible dialog pattern, including scoped initial focus, robust focus containment, reliable Escape/cancel handling, safe focus restoration, and a non-focusable presentation backdrop per T116 and Constitution IX (partial)
- [X] T131 add tenant-status dialog regressions for labelled title/description, initial focus, forward/reverse Tab containment, Escape, Cancel, Confirm, invoker-focus restoration, and exactly one request only after confirmation per T116/T117 and Constitution VII (missing)
- [X] T132 queue status-toggle intents submitted before prior responses complete, derive each transition from the latest committed server response, and prove ordered request payloads/results plus the final state without cancellation per T119, FR-005, and the rapid-toggle edge case (partial)
- [X] T133 complete Playwright create/edit 409/422 and empty-state coverage with exact control-linked `aria-invalid`/`aria-describedby` messages, unchanged URLs and data, exact failed-request counts, and manager-versus-viewer empty-state creation assertions per T122, FR-011, and Constitution VII (partial)
- [X] T134 exercise Clear filters in the no-match Playwright scenario and assert restored tenants, blank query/status controls, and exactly one final request without `q` or `status` per T123 and US2/AC5 (partial)
- [X] T135 regenerate `quickstart-results.md` only after the remaining gates pass, using one unambiguous current source snapshot, internally consistent test counts, exact commands/output/measurements, qualified mocked versus live evidence, and task completion claims matching `tasks.md` per T125 and Constitution VII (contradicts)
- [X] T136 add a mandatory database-enabled verification mode for the 505-row backend traversal so missing or unreachable PostgreSQL fails the intended gate, and record execution against schema including migration 0017 per T114, SC-002, and Constitution VII (partial)
- [X] T137 add one filtered-cursor load-more failure/retry regression proving rows, query, status filter, and cursor remain unchanged and the retry sends identical parameters before appending results per T120 and FR-011 (partial)
- [X] T138 announce asynchronous tenant-detail load failures through an appropriate alert/live region or managed error focus while preserving the exact server message and separate retry/action-error paths, with accessibility tests per T121 and FR-011 (partial)
- [X] T139 strengthen the fake-timer reset regression to assert the complete final unfiltered list parameters and no additional request after the debounce interval per T118 and SC-002 (partial)
