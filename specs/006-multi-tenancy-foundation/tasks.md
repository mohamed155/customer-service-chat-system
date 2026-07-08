# Tasks: Multi-Tenancy Foundation

**Input**: Design documents from `/specs/006-multi-tenancy-foundation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/http-api.md, quickstart.md

**Tests**: INCLUDED — constitution Principle VII (Test-First) applies; plan R11 defines the isolation-matrix suite. Backend integration tests are live-gated on `DATABASE_URL` (skip with notice, run for real in CI); frontend specs run via Vitest.

**Organization**: Tasks are grouped by user story. Backend endpoints map to the story they serve: `GET /tenant` + middleware → US1, `GET /platform/tenants` + switch → US2, `GET /me` → US3 (frontend bootstrap). The switcher UI lives in US3 per spec US3/AC2, even though US2's acceptance mentions it — US2 is independently testable via the API + audit log (its stated independent test).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)

## Path Conventions

Backend Cargo workspace: module crates in `backend/crates/modules/`, server in `backend/crates/server/`. Frontend: `frontend/apps/dashboard/src/app/` with spec-002 layering (`core/`, `layout/`).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Turn placeholder crates into buildable dependencies and scaffold test harnesses

- [X] T001 [P] Add dependencies to `backend/crates/modules/identity/Cargo.toml` and `backend/crates/modules/tenancy/Cargo.toml` (workspace deps: axum, sqlx, serde, serde_json, uuid, chrono, tracing, async-trait as needed; path deps: kernel, config; tenancy additionally depends on identity); keep both crates compiling (`cargo check -p identity -p tenancy`)
- [X] T002 [P] Create integration-test scaffolding in `backend/crates/server/tests/tenancy.rs`: live-gated pool helper (skip with eprintln when `DATABASE_URL` unreachable — same pattern as `crates/shared/db/tests/schema.rs`), `tower::ServiceExt::oneshot` harness against `server::router::app`, and seed helpers creating unique-per-test users (with/without `platform_role`), tenants (active/suspended), and memberships directly via SQL; add needed `[dev-dependencies]` to `backend/crates/server/Cargo.toml` (tower, http-body-util, uuid, serde_json already in workspace)
- [X] T003 [P] Create `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` with `TenantSummary`, `MembershipSummary`, `MeResponse` DTOs exactly per data-model.md (string-literal unions for roles/status)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Principal resolution — every story needs to know who is calling

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T004 Implement the identity module in `backend/crates/modules/identity/src/lib.rs`: `PlatformRole` (string-mapped to 005 CHECK values), `PrincipalKind`, `Principal { user_id, email, display_name, platform_role }` types + `principal_middleware` that (per research R2) reads `X-Dev-User-Id` only when `AppConfig.app_environment` is development/test, resolves it against `users` (`deleted_at IS NULL`), attaches `Principal` as a request extension, and records `principal.id`/`principal.kind` on the trace span; include unit tests for the pure parts (env gating decision, header parsing)
- [ ] T005 Wire the `/api/v1` scaffold in `backend/crates/server/src/router.rs`: layer `principal_middleware` (state: PgPool + config) onto the `/api/v1` nest, extend `cors_layer` allow-headers with `X-Tenant-ID` always and `X-Dev-User-Id` only in development/test (research R7); keep existing fallbacks and spec-004 tests green (`cargo test -p server`)

**Checkpoint**: Requests can carry a resolvable principal — user story implementation can begin

---

## Phase 3: User Story 1 - Tenant Isolation Enforcement (Priority: P1) 🎯 MVP

**Goal**: Every tenant-scoped request is validated (`X-Tenant-ID`) and authorized server-side; tenant users are confined to assigned active tenants; denials are forbidden-and-indistinguishable (anti-enumeration) and audited.

**Independent Test**: With seeded tenants A/B and a tenant user in A only: A succeeds; B, nonexistent, malformed, and missing tenant contexts are all rejected per contracts/http-api.md §B — fully automatable via the API.

### Tests for User Story 1 ⚠️ write first — must FAIL before T007–T010 exist

- [ ] T006 [US1] Write the isolation-matrix integration tests in `backend/crates/server/tests/tenancy.rs` against `GET /api/v1/tenant`: tenant user own tenant → 200 with correct `TenantSummary`; foreign tenant → 403 code `unauthorized`; nonexistent tenant → 403 with **byte-identical body** (modulo request_id) to the foreign case; missing `X-Tenant-ID` → 400 `validation_failed`; malformed UUID → 400; suspended tenant → member 403 (suspension message) / platform user 200; platform user in any tenant → 200; revoked (soft-deleted) membership → 403 on next request; no principal → 401 `unauthenticated`; denial writes a `tenant.access_denied` audit row with NULL `tenant_id` and `requested_tenant_id` + `reason` in details (FR-013, data-model audit vocabulary)

### Implementation for User Story 1

- [ ] T007 [P] [US1] Implement authorization queries in `backend/crates/modules/tenancy/src/authorize.rs` (research R4): `fetch_tenant(pool, id) -> Option<{id, name, slug, status}>` (`deleted_at IS NULL`), `has_active_membership(pool, tenant_id, user_id) -> bool`; unit-test SQL via the live-gated pattern or leave coverage to T006
- [ ] T008 [P] [US1] Implement the audit helper in `backend/crates/modules/tenancy/src/audit.rs`: `record(pool, action, actor, tenant_id, resource, details)` inserting into 005's `audit_logs`; `access_denied(...)` convenience writing `tenant.access_denied` with NULL `tenant_id` and `{requested_tenant_id, reason}` details; insert failures are `tracing::error!`-logged and swallowed (fail-open per research R6)
- [ ] T009 [US1] Implement `TenantContext` + `tenant_context_middleware` in `backend/crates/modules/tenancy/src/lib.rs` (pipeline per research R3): missing header → 400; non-UUID → 400; tenant absent/deleted → 403; platform principal → allow any status; tenant principal → require active membership AND tenant status `active` (suspended → 403 with suspension message, same `unauthorized` code); on denial call `audit::access_denied`; on success attach `TenantContext { tenant_id, tenant_status, principal_kind }` extension and record `tenant.id` on the trace span (depends on T007, T008)
- [ ] T010 [US1] Implement `GET /api/v1/tenant` in `backend/crates/modules/tenancy/src/routes.rs` (reads only `TenantContext`, returns `TenantSummary`) and mount it in `backend/crates/server/src/router.rs` inside the tenant-context middleware (platform-scoped routes stay outside per FR-004)
- [ ] T011 [US1] Verify: with local Postgres up, `cargo test -p server --test tenancy` — all T006 scenarios green; fix until they pass

**Checkpoint**: Cross-tenant isolation enforced and regression-locked — the platform's core guarantee holds

---

## Phase 4: User Story 2 - Platform User Tenant Switching (Priority: P2)

**Goal**: Platform users can list/search tenants and perform an explicit, audited, stateless switch; tenant users cannot touch `/platform/*`.

**Independent Test**: Via API as a platform user: list directory (includes suspended, excludes deleted), switch to tenant B (200 + `platform.tenant_switched` audit row), then call `GET /tenant` with B's header → 200. As a tenant user: both platform endpoints → 403.

### Tests for User Story 2 ⚠️ write first — must FAIL before T013–T015 exist

- [ ] T012 [US2] Add switching integration tests to `backend/crates/server/tests/tenancy.rs`: `GET /api/v1/platform/tenants` → 200 kernel `Page<TenantSummary>` for platform user (contains active + suspended seeds, never deleted; `q` filters by name/slug); tenant user → 403; no principal → 401; `POST /api/v1/platform/tenants/{id}/switch` → 200 `TenantSummary` + exactly one `platform.tenant_switched` audit row (actor, tenant_id, `tenant_slug` in details); switch to suspended tenant → 200; nonexistent/deleted target → 403 `unauthorized`; tenant-user caller → 403

### Implementation for User Story 2

- [ ] T013 [P] [US2] Implement `GET /api/v1/platform/tenants` in `backend/crates/modules/tenancy/src/routes.rs`: platform-principal gate (403 otherwise), kernel cursor pagination + optional `q` (ILIKE on name/slug), `deleted_at IS NULL`, ordered stably for cursors (contracts/http-api.md §C)
- [ ] T014 [P] [US2] Implement `POST /api/v1/platform/tenants/{id}/switch` in `backend/crates/modules/tenancy/src/routes.rs`: platform-principal gate; `authorize::fetch_tenant` (absent → 403, anti-enumeration); synchronous `audit::record("platform.tenant_switched", …)` then 200 `TenantSummary`; stateless — no server-side selection stored (research R5/R6)
- [ ] T015 [US2] Mount both platform routes in `backend/crates/server/src/router.rs` (inside identity middleware, outside tenant-context middleware) and verify `cargo test -p server --test tenancy` green including T012

**Checkpoint**: Switching works end-to-end at the API level with full audit trail

---

## Phase 5: User Story 3 - Frontend Tenant Context Propagation (Priority: P3)

**Goal**: The dashboard bootstraps the current principal, auto-attaches `X-Tenant-ID` on every API call, shows the switcher to platform users only, persists their selection, and handles forbidden states cleanly — the dashboard's first real API integration.

**Independent Test**: In dev with `app.devUserId` set: platform user sees the switcher, picking a tenant fires the switch call and stamps subsequent requests (DevTools Network); selection survives reload; tenant user sees no switcher and their tenant auto-resolves; a stale persisted tenant is discarded. Vitest specs cover each piece headlessly.

### Backend prerequisite for US3

- [ ] T016 [US3] Add `GET /api/v1/me` integration tests to `backend/crates/server/tests/tenancy.rs` (200 `MeResponse` with platform role and active memberships only — soft-deleted memberships excluded; 401 without principal; ignores `X-Tenant-ID`), then implement the endpoint in `backend/crates/modules/tenancy/src/routes.rs` + mount in `backend/crates/server/src/router.rs` (identity-gated, tenant-context-free) and verify green

### Frontend implementation for User Story 3

- [ ] T017 [P] [US3] Create the `tenantContext` NgRx feature in `frontend/apps/dashboard/src/app/core/state/tenant-context.feature.ts` (+ effects + specs): `{ activeTenant: TenantSummary | null, status: 'idle'|'switching'|'error' }`, actions (set/clear/switchRequested/switchSucceeded/switchFailed), persistence effect mirroring platform selections to localStorage `app.tenant` with rehydrate-and-discard-stale behavior per FR-016 (mirror the `app-ui` theme-effect pattern)
- [ ] T018 [P] [US3] Create `frontend/apps/dashboard/src/app/core/tenant/current-user.service.ts` (+ spec): fetches `GET /me` once at shell bootstrap via `ApiService`, exposes `currentUser` and computed `isPlatformUser` signals, derives the tenant-user default active tenant from sole/primary membership (FR-015)
- [ ] T019 [US3] Create `frontend/apps/dashboard/src/app/core/tenant/tenant-context.service.ts` (+ spec): façade over the store — `select(tenant)` calls `POST /platform/tenants/{id}/switch` then dispatches success; `clear()`; validation of rehydrated selections; the only API features use (depends on T017, T018)
- [ ] T020 [P] [US3] Create interceptors in `frontend/apps/dashboard/src/app/core/http/` (+ specs): `tenant-context.interceptor.ts` attaches `X-Tenant-ID` from the store's active tenant to `apiBaseUrl` requests, excluding `/me` and `/platform/*` paths (contracts §E); `dev-identity.interceptor.ts` attaches `X-Dev-User-Id` from localStorage `app.devUserId` only when `APP_CONFIG.environmentName === 'development'` (research R9)
- [ ] T021 [US3] Register everything in `frontend/apps/dashboard/src/app/app.config.ts`: provide `tenantContext` store feature + effects, add both interceptors after `authTokenInterceptor`, trigger `CurrentUserService` bootstrap via `provideAppInitializer` (depends on T017–T020)
- [ ] T022 [US3] Create `frontend/apps/dashboard/src/app/layout/topbar/tenant-switcher.component.ts` (+ spec) and integrate into `topbar.component.ts`: Taiga-wrapped dropdown/search listing `GET /platform/tenants` (with `q`), shows active tenant, dispatches `TenantContextService.select`, renders **only** when `isPlatformUser()` (FR-015); "no tenant selected" prompt state per US2/AC4
- [ ] T023 [US3] Forbidden-state UX in `frontend/apps/dashboard/src/app/core/errors/http-error.mapper.ts` + `tenant-context` slice (+ specs): map tenant-scoped 403 `unauthorized` to a clear "no access to this tenant" surface (Taiga-wrapped alert), clear the persisted selection when it caused the failure, never render partial data (FR-017, research R10)
- [ ] T024 [US3] Update `frontend/apps/dashboard/src/app/core/router/area-access.guard.ts` (+ spec): platform area requires `isPlatformUser()`; tenant area requires an active tenant context (platform users without a selection are routed to the selection prompt) — replaces the placeholder seam (research R9)

**Checkpoint**: Full-stack tenant context: header in, isolation enforced, switcher visible to the right people only

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: End-to-end validation and documentation alignment

- [ ] T025 Execute `specs/006-multi-tenancy-foundation/quickstart.md` end-to-end (isolation tests, curl walkthrough incl. audit rows, FR-019 prod-mode 401 check, frontend behaviors) and fix any doc/behavior drift — requires running Postgres + backend + dashboard
- [ ] T026 [P] Add a `006-multi-tenancy-foundation` entry to the Recent Changes section of `CLAUDE.md` (tenant context middleware, X-Tenant-ID contract, switcher, dev identity header)
- [ ] T027 Run all quality gates: backend `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (Postgres up); frontend `pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check` — all green

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none — start immediately
- **Foundational (Phase 2)**: needs T001 (crate deps); blocks all stories
- **US1 (Phase 3)**: needs Phase 2 (principal) + T002 (harness)
- **US2 (Phase 4)**: needs Phase 2 + T002; reuses US1's `authorize.rs`/`audit.rs` (T007/T008) — schedule after US1 or pull those two tasks forward
- **US3 (Phase 5)**: T016 needs Phase 2; frontend tasks need T003 + backend endpoints (T010 for real requests, T013/T014 for the switcher, T016 for bootstrap)
- **Polish (Phase 6)**: after US1–US3

### Task-level Dependencies

- T001 → T004 → T005 → all endpoint/middleware work
- T002 → T006, T012, T016 (all integration tests share the harness)
- T006 → T007–T010 → T011 (tests fail first, then green)
- T007 + T008 → T009 → T010; T007/T008 also → T013/T014
- T012 → T013–T014 → T015
- T003 → T017–T020; T017+T018 → T019; T017–T020 → T021 → T022–T024

### Parallel Opportunities

- T001 ∥ T002 ∥ T003 (three independent files)
- T007 ∥ T008 (authorize vs audit modules)
- T013 ∥ T014 (two handlers in routes.rs — coordinate as separate functions)
- T017 ∥ T018 ∥ T020 (store, service, interceptors — different files)
- T026 ∥ T025/T027

---

## Parallel Example: User Story 3

```bash
# After T003 + T016, build the frontend core pieces together:
Task: "T017 tenantContext NgRx feature in core/state/tenant-context.feature.ts"
Task: "T018 CurrentUserService in core/tenant/current-user.service.ts"
Task: "T020 tenant-context + dev-identity interceptors in core/http/"
# Then wire and build UI:
Task: "T021 app.config.ts registration" → "T022 switcher" ∥ "T023 forbidden UX" ∥ "T024 guard"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 (T001–T003) + Phase 2 (T004–T005)
2. Phase 3 US1 (T006–T011): isolation enforced and regression-locked at the API
3. **STOP and VALIDATE**: run the matrix (`cargo test -p server --test tenancy`) + quickstart §2 curls — the existential guarantee holds before any UI exists

### Incremental Delivery

1. US1 → isolation (MVP — the constitutional core)
2. US2 → audited switching for platform staff, API-complete
3. US3 → dashboard integration: `/me`, store, interceptors, switcher, guard
4. Polish → quickstart end-to-end + both quality-gate suites

### Notes

- Anti-enumeration is a *test-pinned byte-equality*, not a convention — keep the 403 constructor shared between "not found" and "no access" paths so they can never drift apart
- Integration tests seed unique data per test (uuid-suffixed emails/slugs) against a shared dev DB — same discipline as 005's schema tests; no TRUNCATE
- Commit after each task or logical group; backend and frontend halves of US3 can land as separate commits
