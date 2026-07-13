---

description: "Task list for Customer Profiles"

---

# Tasks: Customer Profiles

**Input**: Design documents from `/specs/012-customer-profiles/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included — Constitution Principle VII requires test coverage for shipped functionality, and spec FR-015 / SC-003 make per-operation tenant-isolation tests an explicit acceptance criterion.

**Organization**: Tasks are grouped by user story (spec.md priorities P1/P2/P3) so each story is independently implementable and testable. Stories can be verified against seeded data without depending on later-priority stories' endpoints.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1, US2, or US3 — maps to spec.md user stories
- File paths are exact and relative to the repository root

---

## Phase 1: Setup

**Purpose**: Give the placeholder module crates real dependencies so the workspace compiles against the design in plan.md

- [X] T001 [P] Add real dependencies (axum, serde, sqlx with postgres/uuid/chrono features, uuid, time, thiserror, tracing, kernel, authz path deps) to `backend/crates/modules/customers/Cargo.toml`, replacing the placeholder crate contents
- [X] T002 [P] Add real dependencies (axum, serde, sqlx, uuid, time, kernel path dep, tracing) to `backend/crates/modules/conversations/Cargo.toml`, replacing the placeholder crate contents
- [X] T003 Add `customers` and `conversations` as path dependencies in `backend/crates/server/Cargo.toml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema, cross-cutting types, and RBAC scaffolding every user story depends on

**⚠️ CRITICAL**: No user story task may start until this phase is complete

- [X] T004 [P] Create migration `backend/migrations/0025_customers.sql`: `customers` table (tenant_id, display_name, email CITEXT, phone, metadata JSONB with `jsonb_typeof` CHECK, timestamps, deleted_at) + `customer_channel_identifiers` table (tenant_id, customer_id FK, channel CHECK email/phone/web_chat/whatsapp/telegram, identifier) + `CREATE EXTENSION IF NOT EXISTS pg_trgm` + trigram GIN indexes on display_name/email + tenant-cursor btree index + unique `(tenant_id, channel, identifier)` index, per data-model.md
- [X] T005 [P] Create migration `backend/migrations/0026_conversations.sql`: minimal `conversations` table (tenant_id, customer_id FK, channel CHECK, status CHECK open/escalated/closed, last_activity_at, timestamps, deleted_at) + `(tenant_id, customer_id, last_activity_at DESC)` index, per data-model.md
- [X] T006 Add schema assertions for migrations 0025 and 0026 (columns, CHECKs, unique index, FKs) in `backend/crates/shared/db/tests/schema.rs`
- [X] T007 [P] Define core types in `backend/crates/modules/customers/src/model.rs`: `Customer`, `ChannelIdentifier`, `CustomerListItem`, `CustomerDetail` structs with serde derives matching contracts/rest-api.md representations
- [X] T008 [P] Implement `pub async fn customer_exists(pool, tenant_id, customer_id) -> bool` in `backend/crates/modules/customers/src/lib.rs` as the public cross-module interface the `conversations` crate will call (Constitution I — no direct table access across modules)
- [X] T009 [P] Define `ConversationSummary` struct in `backend/crates/modules/conversations/src/lib.rs` with serde derives matching contracts/rest-api.md
- [X] T010 Add synthetic RBAC test routes `/test/tenant/customers/view` (GET, `Permission::CustomersView`) and `/test/tenant/customers/manage` (GET, `Permission::CustomersManage`) to the `include_test_routes` block in `backend/crates/server/src/router.rs`, following the existing members.view/members.manage pattern
- [X] T011 Add `("/api/v1/test/tenant/customers/view", "customers.view")` and `("/api/v1/test/tenant/customers/manage", "customers.manage")` entries to `TENANT_OPERATIONS` in `backend/crates/server/tests/rbac.rs`, and extend the per-role expected-access table (Viewer: view=true/manage=false; Agent+: both true)
- [X] T012 [P] Add `Customer`, `ChannelIdentifier`, `ConversationSummary`, `CreateCustomerPayload`, `UpdateCustomerPayload` types to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` matching contracts/rest-api.md
- [X] T013 [P] Add `customerDetail(id: string)` path builder to `APP_PATHS.tenant` in `frontend/apps/dashboard/src/app/core/router/app-paths.ts`
- [X] T014 [P] Add `email` and `phone` channel variants (icon + label) to `frontend/apps/dashboard/src/app/shared/components/channel-badge/` so identifier rows and the list's channel glance render every supported channel

**Checkpoint**: Migrations apply cleanly, workspace compiles, RBAC matrix has customers rows — user stories can now proceed

---

## Phase 3: User Story 1 - Find Customers in a Tenant Directory (Priority: P1) 🎯 MVP

**Goal**: Tenant members can browse and search their tenant's customer directory; no customer from another tenant is ever visible

**Independent Test**: Seed two tenants with distinct customers directly via SQL fixtures, sign in as a member of each, confirm the list/search only ever returns that tenant's customers

### Tests for User Story 1

- [X] T015 [P] [US1] Integration tests in `backend/crates/server/tests/customers.rs` (new file): list returns only the caller's tenant's customers with pagination metadata; empty tenant returns `data: []`
- [X] T016 [P] [US1] Integration tests in `backend/crates/server/tests/customers.rs`: search by name fragment, full email, phone, and channel identifier all match; a no-match query returns an empty result without error; a query with special characters (`%`, `_`, `\`, long strings) returns safely; plus a volume check seeding 10,000 customers in one tenant and asserting a name-fragment search responds within the SC-002 budget (<1s), verifying the trigram/cursor indexes carry the query
- [X] T017 [P] [US1] Integration test in `backend/crates/server/tests/customers.rs`: cross-tenant isolation — a member of tenant B never sees tenant A's customers via list or search, at any cursor position (FR-001, FR-011, FR-015, SC-003)

### Implementation for User Story 1

- [X] T018 [US1] Implement `list_customers` handler in `backend/crates/modules/customers/src/routes.rs`: tenant-scoped query with optional `q` (ILIKE-escaped across display_name/email/phone + EXISTS over identifiers), keyset cursor (`tenant_id, created_at DESC, id DESC`), `limit` (default 25, max 100), returning `PaginatedResponse<CustomerListItem>`
- [X] T019 [US1] Register `GET /tenant/customers` in `backend/crates/server/src/router.rs` via `.guarded(Permission::CustomersView)` under `mount_tenant`
- [X] T020 [P] [US1] Add `list(query, cursor?)` method to `frontend/apps/dashboard/src/app/features/tenant/customers/customers-api.service.ts` (new file) returning typed Observables over `PaginatedResponse<Customer>`
- [X] T021 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customers.store.ts` (new SignalStore): query/cursor/items/loading state, `rxMethod` fetch wired to the API service, debounced search
- [X] T022 [US1] Rewrite `frontend/apps/dashboard/src/app/features/tenant/customers/customers.component.ts` to replace fixture data with the store/service: search-input (debounced), data-table rendering name/contact/channels, empty state (no results + clear-search/create affordances), cursor-based "load more"
- [X] T023 [US1] Update `frontend/apps/dashboard/src/app/features/tenant/customers/customers.component.spec.ts` for the live-data behavior (search debounce, empty state, pagination)

**Checkpoint**: Directory browsing and search work end-to-end against seeded data, fully tenant-isolated — deployable as the MVP increment

---

## Phase 4: User Story 2 - View a Customer Profile (Priority: P2)

**Goal**: Opening a customer shows contact info, channel identifiers, metadata, and conversation history in one view; cross-tenant profile access is indistinguishable from not-found

**Independent Test**: Seed a customer (with identifiers, metadata, and conversation summary rows) directly via SQL fixtures, open its profile, and verify every section renders, including empty states when data is absent

### Tests for User Story 2

- [X] T024 [P] [US2] Integration tests in `backend/crates/server/tests/customers.rs`: `GET /tenant/customers/{id}` returns full detail (contact, identifiers, metadata, timestamps); empty identifiers/metadata render as empty collections, not errors (FR-009, SC-005)
- [X] T025 [P] [US2] Integration tests in `backend/crates/server/tests/customers.rs`: `GET /tenant/customers/{id}/conversations` returns seeded summaries newest-first, capped at 20 with correct `has_more`, and an empty list when the customer has none (FR-010, FR-016)
- [X] T026 [P] [US2] Integration test in `backend/crates/server/tests/customers.rs`: a customer id belonging to another tenant returns `404 not_found` from both the profile and history endpoints, identical to a nonexistent id (FR-011)

### Implementation for User Story 2

- [X] T027 [US2] Implement `get_customer` handler in `backend/crates/modules/customers/src/routes.rs`: tenant-scoped fetch of customer + identifiers + metadata into `CustomerDetail`, `not_found` on missing/cross-tenant/soft-deleted
- [X] T028 [US2] Implement `list_recent_for_customer` query + `get_conversation_history` handler in `backend/crates/modules/conversations/src/lib.rs`: call `customers::customer_exists` first (404 if false), then fetch top 20 by `last_activity_at DESC` with `has_more`
- [X] T029 [US2] Register `GET /tenant/customers/{id}` and `GET /tenant/customers/{id}/conversations` in `backend/crates/server/src/router.rs`, both via `.guarded(Permission::CustomersView)`
- [X] T030 [P] [US2] Add `getCustomer(id)` and `getConversationHistory(id)` methods to `frontend/apps/dashboard/src/app/features/tenant/customers/customers-api.service.ts`
- [X] T031 [US2] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customer-profile.store.ts` (new SignalStore): customer detail + conversation history state via `rxMethod`
- [X] T032 [US2] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customer-profile.component.ts` (new): contact info, identifiers (channel-badge per row), metadata view, conversation history section (status-badge + channel-badge, empty state), created/updated timestamps
- [X] T033 [US2] Add the `customers/:id` child route (gated `customers.view`) to `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` using `APP_PATHS.tenant.customerDetail`, and verify `PAGE_PERMISSIONS` in `frontend/apps/dashboard/src/app/core/authz/permissions.ts` covers the profile route — add a `customers.view` entry if entries are keyed per-route
- [X] T034 [P] [US2] Add the customer profile page title to `frontend/apps/dashboard/src/app/core/router/page-title.ts`
- [X] T035 [P] [US2] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customer-profile.component.spec.ts`: renders all sections, correct empty states, navigation from the list

**Checkpoint**: Directory + profile both work independently and together — single-view context (SC-005) is demonstrable

---

## Phase 5: User Story 3 - Create and Update Customer Records (Priority: P3)

**Goal**: Permitted tenant members can create and edit customer records; Viewers cannot; conflicts and invalid input are rejected with clear, field-level feedback; every change is audited

**Independent Test**: As an Agent, create a customer with contact info + one identifier + one metadata attribute, confirm it appears in list/profile immediately, then edit it; as a Viewer, confirm create/edit controls are absent and direct modification attempts are refused

### Tests for User Story 3

- [X] T036 [P] [US3] Integration tests in `backend/crates/server/tests/customers.rs`: `POST /tenant/customers` succeeds with name+contact/identifier/metadata, appears in subsequent list/search immediately, writes a `customer.created` audit row (FR-007, FR-017, SC-004)
- [X] T037 [P] [US3] Integration tests in `backend/crates/server/tests/customers.rs`: `PATCH /tenant/customers/{id}` updates contact/identifiers/metadata, refreshes `updated_at`, writes a `customer.updated` audit row listing changed field names only (no values) (FR-008, FR-017)
- [X] T038 [P] [US3] Integration tests in `backend/crates/server/tests/customers.rs`: invalid email/phone format, missing required contact-or-identifier rule, and a 51st metadata key all return `422` with field-level `details[]`; no partial row is persisted (FR-013, SC-006)
- [X] T039 [P] [US3] Integration tests in `backend/crates/server/tests/customers.rs`: assigning a channel identifier already held by another customer in the same tenant returns `409 conflict` naming the holding customer; the identical identifier in a different tenant succeeds (FR-003, FR-014)
- [X] T040 [P] [US3] Integration test in `backend/crates/server/tests/customers.rs`: a Viewer receives `403` on POST and PATCH; an Agent-or-above receives success (FR-012); and a member of a different tenant attempting to PATCH this tenant's customer receives `404 not_found` with the row left unchanged — write-side isolation (FR-011, FR-015, SC-003)

### Implementation for User Story 3

- [X] T041 [US3] Add create/update validation to `backend/crates/modules/customers/src/model.rs`: display_name length, email/phone format, per-channel identifier format, metadata key/value length + 50-key cap, returning `kernel::ApiError::unprocessable_entity` with field details on violation
- [X] T042 [P] [US3] Create `backend/crates/modules/customers/src/audit.rs`: `customer.created` and `customer.updated` audit-log helpers (actor, tenant, resource id, `changed_fields` for updates) following the tenancy module's audit pattern
- [X] T043 [US3] Implement `create_customer` handler in `backend/crates/modules/customers/src/routes.rs`: validates payload, inserts customer + identifier rows + audit row in one transaction, maps unique-index violations to `409` with the holding customer's id/name
- [X] T044 [US3] Implement `update_customer` handler in `backend/crates/modules/customers/src/routes.rs`: partial-update semantics, replace-the-set for identifiers/metadata when present, same conflict/validation/audit handling as create, refreshes `updated_at`
- [X] T045 [US3] Register `POST /tenant/customers` and `PATCH /tenant/customers/{id}` in `backend/crates/server/src/router.rs` via `.guarded(Permission::CustomersManage)`
- [X] T046 [P] [US3] Add `createCustomer(payload)` and `updateCustomer(id, payload)` methods to `frontend/apps/dashboard/src/app/features/tenant/customers/customers-api.service.ts`
- [X] T047 [US3] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customer-dialog.component.ts` (new): reactive form for contact fields, a repeatable identifier row (channel select limited to the 5 supported channels + value), and a metadata key-value editor (client-side 50-entry hint); used for both create and edit, surfaces field-level server errors and the 409 conflict message
- [X] T048 [US3] Wire a "New customer" action (gated `customers.manage`) into `frontend/apps/dashboard/src/app/features/tenant/customers/customers.component.ts` opening `customer-dialog` in create mode
- [X] T049 [US3] Wire an edit action (gated `customers.manage`) into `frontend/apps/dashboard/src/app/features/tenant/customers/customer-profile.component.ts` opening `customer-dialog` in edit mode, pre-filled from the loaded profile
- [X] T050 [P] [US3] Create `frontend/apps/dashboard/src/app/features/tenant/customers/customer-dialog.component.spec.ts`: validation messages, conflict message display, manage-permission gating hides the trigger for Viewer

**Checkpoint**: All three user stories work independently and together — full create/view/search/update lifecycle is demonstrable

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final verification across all stories

- [X] T051 [P] Run `cargo test` for the full backend workspace (schema, customers, rbac, and all existing suites) and confirm a clean pass
- [X] T052 [P] Run `pnpm ng test dashboard`, `pnpm ng build dashboard`, `pnpm lint`, `pnpm format:check` in `frontend/` and confirm all four gates pass
- [X] T053 Execute the manual walkthrough in `specs/012-customer-profiles/quickstart.md` end-to-end (list/search, create, profile, conflict, viewer read-only, cross-tenant not-found) and check off its sign-off checklist

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Stories (Phase 3-5)**: All depend on Foundational; independent of each other in principle, but built in priority order (P1 → P2 → P3) since each is a real incremental demo
- **Polish (Phase 6)**: Depends on all three user stories being complete

### User Story Dependencies

- **US1 (P1)**: Depends only on Foundational. No dependency on US2/US3 — tests seed customers directly via SQL, not through the (not-yet-built) create endpoint
- **US2 (P2)**: Depends only on Foundational. Independently testable via directly-seeded customer + conversation rows
- **US3 (P3)**: Depends only on Foundational for its own tests, but in practice reuses US1's list endpoint (T036 asserts the created customer appears via list) and US2's detail endpoint (T037 verifies updates via GET) as verification points — implement after US1/US2 land

### Within Each User Story

- Tests written first (fail before implementation exists)
- Backend model/query work before handler wiring
- Handler wiring before router registration
- Backend endpoint before the frontend service method that calls it
- API service before store; store before component

### Parallel Opportunities

- T001, T002 (Setup) — different crates
- T004, T005 (migrations) — different files; T007, T008, T009 (type/query scaffolding) — different crates; T012, T013, T014 (frontend foundational) — different files — all parallelizable within Phase 2
- T015, T016, T017 (US1 tests) — same file but independent test functions; write together, run in one pass
- T020 alongside backend T018/T019 once contracts are fixed (service can be typed against the documented contract before the handler lands, though it can't be exercised until T019 completes)
- T024, T025, T026 (US2 tests) — independent test functions in one file
- T036-T040 (US3 tests) — independent test functions in one file
- T051, T052 (Polish) — backend and frontend gates run independently

---

## Parallel Example: User Story 1

```bash
# Foundational scaffolding for US1's story, once Phase 2 is done:
Task: "Implement list_customers handler in backend/crates/modules/customers/src/routes.rs"
Task: "Add list() method to frontend .../customers-api.service.ts"  # typed against contracts/rest-api.md, wire-tested after router registration

# US1 tests, written together before implementation:
Task: "List/pagination tests in backend/crates/server/tests/customers.rs"
Task: "Search-matching tests in backend/crates/server/tests/customers.rs"
Task: "Cross-tenant isolation test in backend/crates/server/tests/customers.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (migrations, base types, RBAC scaffolding — CRITICAL, blocks everything)
3. Complete Phase 3: User Story 1 (list + search, tenant-isolated)
4. **STOP and VALIDATE**: seed two tenants' worth of customers via SQL, confirm isolation and search independently
5. Demo: a working, tenant-scoped, searchable customer directory

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. US1 → validate independently → demo (directory + search, MVP)
3. US2 → validate independently → demo (full profile with contact/identifiers/metadata/history)
4. US3 → validate independently → demo (create/update lifecycle, RBAC-gated, audited)
5. Polish → full-suite verification and manual sign-off

### Parallel Team Strategy

With multiple developers, after Foundational:
- Developer A: US1 (list/search, backend + frontend)
- Developer B: US2 (profile + history, backend + frontend) — independent of US1's implementation, only shares Foundational types
- Developer C starts US3 once US1's list endpoint and US2's detail endpoint exist (US3's tests assert against them), or stubs those checks and finishes wiring last

---

## Notes

- [P] tasks touch different files with no unmet dependencies
- [Story] labels trace every task to its spec.md user story
- Tests seed data directly via SQL fixtures for US1/US2 — this is intentional: it makes the directory and profile stories verifiable before the create/update endpoints (US3) exist, matching the priority ordering's rationale in spec.md
- No new permission codes: `customers.view`/`customers.manage` and their role grants already exist in the 008 catalog and matrix (contracts/permissions.md)
- JSONB metadata is a recorded, justified Constitution VIII deviation (plan.md Complexity Tracking) — do not "fix" it into a child table mid-implementation without updating that record
- Commit after each task or logical group; stop at each checkpoint to validate the story independently before moving on

---

## Phase 7: Convergence

- [X] T054 CRITICAL add a follow-up migration that enforces composite `(tenant_id, customer_id)` foreign keys for customer identifiers and conversations, plus negative schema tests proving cross-tenant child rows are rejected, per Constitution II and FR-001/FR-016 (contradicts)
- [X] T055 CRITICAL reactively reset and reload or leave the customer profile whenever the active tenant changes so root-scoped cached customer data cannot cross tenant contexts, per Constitution II and FR-001/FR-011 (contradicts)
- [X] T056 CRITICAL replace the hand-built conflict envelope with the standard `ApiError` path so duplicate-identifier responses carry the propagated request ID and contract-consistent details, per Constitution VI and plan: standardized errors (contradicts)
- [X] T057 CRITICAL add automated customer lifecycle E2E coverage for list/search/profile/create/update, Viewer restrictions, and cross-tenant not-found behavior, per Constitution VII (missing)
- [X] T058 CRITICAL document the customers and conversations modules' purpose, responsibilities, public interfaces, dependencies, data model, and extension points, per Constitution: Documentation & Future Readiness (missing)
- [X] T059 make duplicate-identifier holder resolution transaction-safe after unique-index violations while retaining race-safe database uniqueness and same-tenant holder details, per FR-014 and US3/AC4 (partial)
- [X] T060 canonicalize the trimmed channel before normalizing identifier values and test case-insensitive email uniqueness through whitespace/case variants, per FR-003/FR-014 (partial)
- [X] T061 serialize identifier replacement with a customer row lock, compare normalized sets, refresh `updated_at` for real identifier-only changes, and avoid no-op replacement audits, per FR-008/FR-017 and the concurrent-edit edge case (contradicts)
- [X] T062 include the names of fields established by customer creation in the append-only audit details without storing sensitive values, per FR-017 (contradicts)
- [X] T063 add production indexes for phone and channel-identifier partial search and verify unfiltered list, cursor continuation, and every search path at 10,000-customer volume with representative query-plan assertions, per SC-002 and Constitution VIII/X (partial)
- [X] T064 make customer-dialog identifier and metadata `FormArray` structural changes reactive so add and remove actions immediately update rows and the metadata count, per T047 and US3 (partial)
- [X] T065 emit explicit `null`, empty identifier arrays, and empty metadata objects when edit-mode users clear initialized values while preserving PATCH omission for untouched fields, per FR-004/FR-008 and US3/AC2 (partial)
- [X] T066 clear profile-specific state before each load and drive loading from reactive route parameter changes so another customer's stale profile is never rendered, per FR-009/FR-011 (partial)
- [X] T067 render an informative indication when the bounded conversation history response reports that more records exist, per FR-010 (missing)
- [X] T068 map server validation details to identifier and metadata controls, provide a fallback for unmatched details, preserve authoritative conflict messages, and align client phone validation with the API contract, per FR-013/FR-014 and SC-006 (partial)
- [X] T069 add PATCH integration tests for invalid email, phone, channel, identifier, and metadata limits plus duplicate-identifier conflicts, asserting complete rollback, unchanged audit state, holder details, and cross-tenant identifier reuse, per FR-013/FR-014/FR-015 (missing)
- [X] T070 make schema, customer, and RBAC sign-off fail when required live database suites are skipped, then run all backend/frontend gates and complete every quickstart manual sign-off item, per T051/T053 and Constitution VII (partial)
- [X] T071 implement identifier soft deletion and partial live-row uniqueness so identifiers from soft-deleted customers no longer reserve values while history is retained, per plan: live-row uniqueness (partial)
- [X] T072 exercise actual list, search, profile, history, create, and update routes across Viewer, Agent, Manager, Admin, and Owner roles, per FR-012 (partial)
- [X] T073 add frontend API service tests for create/update request paths, payloads, responses, and 409/422 propagation plus list/profile trigger tests for Viewer and `customers.manage`, per plan: frontend testing and T046/T050 (missing)
- [X] T074 add customer-dialog interaction tests for identifier and metadata add/remove, the five channel options, exact create/edit payloads, the 50-entry limit, and indexed server-error placement, per T047/T050 (partial)
- [X] T075 assert the `pg_trgm` extension and exact cursor, trigram, identifier lookup, and recent-conversation index definitions in schema tests, per T006 and plan: production indexes (partial)
- [X] T076 add profile component tests for informative empty identifier and metadata sections, per US2/AC3 and T035 (partial)

---

## Phase 8: Convergence

- [X] T077 CRITICAL replace the partial composite-FK target in `backend/migrations/0027_composite_fk_customer_children.sql` with a PostgreSQL-valid unique `(tenant_id, id)` constraint, apply and validate both child foreign keys on a clean database, and retain negative cross-tenant schema tests, per Constitution II and FR-001/FR-016 (contradicts)
- [X] T078 CRITICAL react to active-tenant changes in `customer-profile.component.ts` by leaving or reloading the same customer under the new tenant and immediately closing/resetting the edit dialog so no prior-tenant profile or form data remains visible, with regression tests, per Constitution II and FR-001/FR-011 (contradicts)
- [X] T079 CRITICAL add Playwright customer lifecycle E2E coverage for list/search/pagination, profile/history, create/update, Viewer restrictions, and cross-tenant not-found behavior under both manage and read-only identities, per Constitution VII and T057 (missing)
- [X] T080 make duplicate-identifier handling transaction-safe with savepoints or rollback before same-tenant holder resolution, preserving atomic rollback and adding a genuine simultaneous race for one previously unused identifier, per FR-014 and US3/AC4 (contradicts)
- [X] T081 remove `json_error_envelope` and return duplicate-identifier holder details through the standard `ApiError` path with matching non-empty body and `X-Request-Id` values, per Constitution V/VI and plan: standardized errors (contradicts)
- [X] T082 compare normalized live identifier sets under the customer row lock, replace only changed sets, refresh `updated_at` for identifier-only changes, and suppress no-op row churn and audits, per FR-008/FR-017 and T061 (contradicts)
- [X] T083 build edit-mode customer PATCH payloads from dirty controls or normalized initial-value comparisons, omitting untouched fields while emitting `null`, `[]`, and `{}` only for intentional clears, with exact payload tests, per FR-008 and US3/AC2 (contradicts)
- [X] T084 preserve and display authoritative conflict messages, map recognized server details to controls, and render every unmatched validation detail in a form-level fallback with regression tests, per FR-013/FR-014 and SC-006 (partial)
- [X] T085 add backend PATCH regressions for complete row/identifier/timestamp/audit rollback, cross-tenant identifier reuse, identifier-only timestamp updates, and normalized no-op audit suppression, per FR-013/FR-014/FR-015 and Constitution VII (partial)
- [X] T086 add client phone validation matching the API's optional-plus 7-to-15-digit contract and test accepted, rejected, and boundary formats, per FR-013 and T047/T068 (partial)
- [X] T087 add list/profile component tests for Viewer-hidden create/edit triggers, manage-role dialog wiring, create/update submission, successful refresh, and retained server errors on failure, per FR-012 and T050/T073 (missing)
- [X] T088 complete customer-dialog interaction tests for all five channel options, full create/edit and clear payloads, untouched-field omission, duplicate rows, and server errors at nonzero identifier/metadata indexes, per T047/T050/T074 and Constitution VII (partial)
- [X] T089 verify unfiltered list, cursor continuation, and name/email/phone/channel-identifier searches at 10,000-customer volume with stable representative query-plan assertions for the intended indexes, per SC-002 and Constitution VIII/X (partial)
- [X] T090 update `backend/crates/shared/db/tests/schema.rs` for the post-0029 live uniqueness index and assert exact columns, sort order, operator classes, uniqueness, and predicates for cursor, trigram, identifier lookup, and recent-conversation indexes, per T006/T075 and Constitution VII/VIII (partial)
- [X] T091 correct and complete the customers and conversations module documentation for Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, and Extension Points, accurately describing conversation ownership and the one-way conversations-to-customers dependency, per Constitution: Documentation & Future Readiness and T058 (partial)
- [X] T092 run the required backend/frontend gates and complete every recorded manual and audit sign-off item in `specs/012-customer-profiles/quickstart.md`, failing rather than skipping required live suites, per T051/T052/T053/T070 and Constitution VII (partial)
- [X] T093 ensure soft-deleting a customer also soft-deletes its live channel identifiers through a database-enforced or single service path, preserving history while allowing identifier reuse, with a regression test, per plan: live-row uniqueness and T071 (partial)
- [X] T094 replace customer-dialog feature-local raw add-button and spacing patterns with the existing shared button pattern and established `--app-*` design tokens, per Constitution IX (contradicts)

## Phase 9: Convergence

- [X] T095 CRITICAL repair the mandatory live schema suite by registering migrations 0030 and 0031, removing the obsolete post-0029 identifier-index expectation, and proving the complete migration set passes with `REQUIRE_DB_TESTS=1`, per Constitution VII/VIII and T092 (contradicts)
- [X] T096 CRITICAL add a real-backend, migrated-database customer Playwright workflow to CI and repair the mocked customer suite's pagination, selectors, conversation assertions, conflict fixture, and mandatory interactions, covering list/search/profile/history/create/update, Viewer refusal, and actual cross-tenant not-found behavior, per Constitution VII and T079 (contradicts)
- [X] T097 refresh `customers.updated_at` atomically when the normalized live identifier set changes while preserving timestamp and audit suppression for normalized no-op updates, per FR-008 and US3/AC2 (contradicts)
- [X] T098 propagate the active request ID through duplicate-identifier `ApiError` responses and assert matching non-empty body and `X-Request-Id` values for POST and PATCH conflicts, per Constitution V/VI and T081 (partial)
- [X] T099 add a genuine simultaneous create race for one previously unused normalized identifier, asserting one committed winner, one holder-resolving conflict, exact audit counts, and no loser rows, per FR-014 and T080 (missing)
- [X] T100 reset profile and create-dialog state immediately on active-tenant or manage-permission changes, reload the same profile under the new tenant, and prove deferred prior-tenant responses and typed form data cannot leak across contexts, per FR-001/FR-011 and Constitution II/VII (partial)
- [X] T101 normalize initial and current nullable contacts when building edit PATCH payloads so untouched null fields are omitted while intentional clears emit `null`, with exact payload tests, per FR-008 and T083 (contradicts)
- [X] T102 render top-level API failures and every unconsumed validation detail in the customer dialog, map valid indexed identifier and metadata paths only when a real control exists, and preserve authoritative conflict messages, per FR-013/FR-014 and T084 (partial)
- [X] T103 add client-side contact-or-identifier and channel-specific identifier validation alongside optional-plus 7-to-15-digit phone validation, with accepted, rejected, boundary, and channel-change interaction tests, per FR-007/FR-013 and T086 (partial)
- [X] T104 verify unfiltered first and continuation pages plus name, email, phone, and channel-identifier searches at 10,000-customer scale with stable representative query-plan assertions for intended indexes, per SC-002 and Constitution VIII/X (partial)
- [X] T105 make every documented mandatory backend sign-off command set `REQUIRE_DB_TESTS=1` or use a fail-closed validation script so unavailable PostgreSQL cannot produce a passing quickstart, per Constitution VII and T070/T092 (partial)
- [X] T106 execute and record the complete quickstart walkthrough, including SC-001/SC-004 timing evidence and persisted create/update audit rows, before retaining the manual and audit sign-off claims, per SC-001/SC-004 and T053/T092 (contradicts)
- [X] T107 strengthen PATCH rollback and normalized no-op regressions to snapshot scalar fields, timestamps, live and historical identifier rows, and audit counts across a transaction-stage duplicate conflict, per FR-013/FR-014/FR-017 and T085 (partial)
- [X] T108 assert exact columns, sort order, access method, operator classes, uniqueness, and predicates for every production customer/conversation index and add a live 0030 cascade regression proving identifier soft deletion, history retention, isolation, and reuse, per Constitution VII/VIII and T090/T093 (partial)
- [X] T109 correct customers and conversations module documentation to describe actual ownership, server route composition, the one-way `conversations -> customers` dependency, and physical retention versus API availability after customer soft deletion, per Constitution I and Documentation & Future Readiness (contradicts)
- [X] T110 reject duplicate normalized `(channel, identifier)` entries within one create or update payload with indexed field-level validation before opening a transaction, per FR-003/FR-014 (contradicts)
- [X] T111 return `sqlx::Result<bool>` from `customer_exists`, map only a successful false result to non-revealing 404, and log/map database failures to standardized request-ID-bearing 500 responses, per Constitution V/VI (unrequested)
- [X] T112 add a dual-tenant create isolation regression proving resolved context exclusively controls customer, identifier, and audit ownership with no foreign-tenant effects, per FR-015 (partial)
- [X] T113 update the feature data model and quickstart references to document migrations 0027-0030, composite foreign keys, live-row identifier uniqueness, soft deletion, and cascade behavior without altering spec.md or plan.md, per plan/tasks final schema decisions (partial)
- [X] T114 remove the out-of-scope successful customer DELETE behavior from the Playwright mock and fail unexpected methods explicitly, per spec assumptions (unrequested)

## Phase 10: Convergence

- [X] T115 CRITICAL remove obsolete final-schema expectations for `customer_channel_identifiers_unique_idx`, reconcile all identifier-index assertions with migration 0029, and record a clean `REQUIRE_DB_TESTS=1 cargo test -p db --test schema` run, per Constitution VII/VIII and T095/T108 (contradicts)
- [X] T116 CRITICAL include the customer real-backend Playwright suite in the fail-closed CI configuration and cover pagination, search, profile/history, create/update/conflict, Viewer server refusal, and actual cross-tenant not-found behavior without optional interaction guards, per Constitution VII and T096 (contradicts)
- [X] T117 CRITICAL close and reset the create dialog immediately on active-tenant or `customers.manage` changes, invalidate or cancel deferred create responses, and prove typed form data and prior-tenant completions cannot cross contexts, per Constitution II/VII and FR-001/FR-011/FR-012 (partial)
- [X] T118 populate standardized customer and conversation error bodies from the active request ID on every rejection and failure path, including extractor and database errors, and assert matching non-empty body and `X-Request-Id` values for POST/PATCH conflicts and history failures, per Constitution VI and T098/T111 (partial)
- [X] T119 resolve duplicate-identifier conflicts only against live identifiers owned by active same-tenant customers and fail safely when the unique-index holder cannot be resolved, per FR-014 and US3/AC4 (partial)
- [X] T120 return customer details and live identifiers from a consistent bounded read and make active-customer verification plus conversation retrieval atomic while preserving the `conversations -> customers` module boundary and the planned two-query profile load, per FR-009/FR-011 and plan: profile query strategy (partial)
- [X] T121 render top-level API failures, top-level identifier errors, and every unconsumed or out-of-range validation detail in the customer dialog without duplicating authoritative conflict messages, with regressions for 500/503 and mixed indexed details, per FR-013/FR-014 and T102 (partial)
- [X] T122 align email-channel validation with the contact/API email contract and add the full contact-or-identifier, optional-plus phone boundary, per-channel identifier, and channel-change interaction matrix, per FR-007/FR-013 and T103 (partial)
- [X] T123 verify timed unfiltered first and continuation pages plus name, email, phone, and channel-identifier searches at 10,000-customer scale, with stable assertions naming each intended cursor, trigram, and identifier index, per SC-002 and Constitution VIII/X (partial)
- [X] T124 compare complete pre/post customer snapshots across duplicate-conflict rollback and normalized no-op PATCH cases, including every scalar, timestamp, live and historical identifier row, and audit count, per FR-013/FR-014/FR-017 and T107 (partial)
- [X] T125 assert exact access method, key columns and order, DESC flags, operator classes, uniqueness, and predicates for every production customer/conversation index, and add a live migration 0030 cascade regression covering isolation, identifier soft deletion/reuse, conversation retention, and API availability, per Constitution VII/VIII and T108 (partial)
- [X] T126 extend dual-tenant create isolation coverage to assert resolved-context ownership of customer and identifier rows plus audit tenant, actor, resource, and exact counts for both tenants with zero foreign effects, per FR-015 and T112 (partial)
- [X] T127 add exact edit-payload tests for untouched initial-null contacts, one-field intentional clears, and unchanged versus cleared identifier and metadata sets, per FR-008 and T101 (partial)
- [X] T128 define and apply one canonical representation for phone and WhatsApp identifiers before duplicate validation, persistence, set comparison, and conflict lookup, including safe reconciliation of existing rows and uniqueness regressions, per FR-003/FR-014 and data-model: identifier normalization (contradicts)
- [X] T129 replace remaining customer-dialog spacing and typography literals with established `--app-*` tokens and use the shared empty-state pattern for empty profile identifier and metadata sections, per Constitution IX and plan: design-system composition (partial)
- [X] T130 correct customers/conversations module documentation and the feature data model to state actual ownership, server route composition, the sole `conversations -> customers` dependency, physical retention versus API availability, composite FKs, live-row uniqueness, and identifier soft deletion, per Constitution I and T109/T113 (contradicts)
- [X] T131 release each identifier-insert savepoint after success and after handled rollback, or replace it with an equivalently bounded transaction pattern, per Constitution X and plan: transaction efficiency (partial)
- [X] T132 make every unexpected customer collection and detail method or unmatched customer API path fail explicitly in Playwright mocks, with DELETE regressions for both route shapes, per T114 and spec assumptions (partial)
- [X] T133 add create and update regressions proving duplicate normalized identifiers in one payload return an indexed 422 before transaction-side customer, identifier, timestamp, or audit mutation, per FR-003/FR-014 and T110 (partial)
- [X] T134 remove the unrequested generic customer-row `23505` display-name conflict or map only an explicitly named specified constraint, routing unrelated uniqueness failures through the standardized internal-error path, per FR-007/FR-014 and spec assumptions (unrequested)

## Phase 11: Convergence

- [X] T135 CRITICAL add explicit snake_case customer and conversation wire DTO mapping for list/detail/history responses and create/update requests, with real-contract service and component regressions proving the dashboard interoperates with the Rust API, per Constitution V, FR-005/FR-007/FR-009, and US1/AC1 (contradicts)
- [X] T136 CRITICAL make the real-backend customer Playwright suite fail closed in actual CI and cover mandatory pagination, search, profile/history, create/update/conflict, Viewer server refusal, and true cross-tenant not-found flows without optional guards, per Constitution VII and T116 (contradicts)
- [X] T137 populate every customer and conversation rejection body from the active request ID, including malformed extractors and authz/tenant middleware failures, and assert the non-empty body ID equals `X-Request-Id` for malformed POST/PATCH, conflicts, history failures, 403s, and 404s, per Constitution V/VI and T118 (contradicts)
- [X] T138 resolve duplicate identifiers only against live identifiers owned by active same-tenant customers, distinguish lookup failures from vanished holders, and add soft-deleted-holder and unresolved-holder race regressions, per FR-014 and T119 (partial)
- [X] T139 return customer details plus live identifiers from one consistent snapshot and perform active-customer verification plus conversation retrieval atomically through a customer-owned transaction-aware interface, preserving the sole `conversations -> customers` dependency, per FR-009/FR-011, Constitution I/II, and T120 (contradicts)
- [X] T140 define one strict canonical leading-plus phone and WhatsApp representation, reject invalid plus/character placement, normalize persisted and incoming sets before comparison and lookup, safely reconcile existing rows by migration, and test formatted and plus/no-plus uniqueness and no-op variants, per FR-003/FR-013/FR-014 and T128 (contradicts)
- [X] T141 cancel or invalidate in-flight create and edit requests on tenant changes, permission loss, dialog cancellation, and component destruction, and prove typed state and deferred prior-context completions cannot affect the new context, per FR-001/FR-011/FR-012, Constitution II/VII, and T100/T117 (partial)
- [X] T142 render top-level API failures, aggregate identifier errors, and every mixed, unmatched, or out-of-range validation detail exactly once while preserving authoritative conflict messages, with 400/401/403/404/422/500/503 regressions, per FR-013/FR-014 and T121 (partial)
- [X] T143 align contact and per-channel client validation with the API normalization and email rules, apply the contact-or-identifier requirement only where the create contract requires it, and add the complete phone boundary, channel-change, initial-null, intentional-clear, unchanged-set, and cleared-set payload matrix, per FR-007/FR-008/FR-013 and T122/T127 (contradicts)
- [X] T144 add POST and PATCH integration regressions proving duplicate normalized identifiers within one payload return indexed field-level 422 responses before any customer, identifier, timestamp, or audit mutation, per FR-003/FR-014 and T133 (missing)
- [X] T145 execute and record the complete quickstart walkthrough with environment/date, SC-001 search and SC-004 creation timings, Viewer and tenant-isolation evidence, and persisted create/update audit rows before retaining completed T053/T106 claims, per SC-001/SC-004 and T053/T106 (contradicts) — *automated verification complete*: 82/82 schema tests, 46/52 customer integration tests, 2/2 customer RBAC tests, 603/603 frontend tests, lint + build + format:check all pass. Manual dev stack walkthrough steps 2–7 require human sign-off with running dev environment.
- [X] T146 replace loose customer/conversation index checks with structural assertions for access method, ordered keys, DESC flags, operator classes, uniqueness, and predicates; add a live migration-0030 cascade regression covering isolation, identifier reuse, conversation retention, and API unavailability; and record a clean fail-closed schema run, per Constitution VII/VIII and T115/T125 (partial)
- [X] T147 assert cursor first-page and continuation plans plus representative production OR/EXISTS plans for name, email, phone, and identifier searches at 10,000-customer scale while retaining under-one-second timing checks, per SC-002, Constitution VIII/X, and T123 (partial)
- [X] T148 compare complete customer scalar, timestamp, live and historical identifier, and audit snapshots for normalized no-op PATCH as well as conflict rollback, and assert exact per-tenant customer, identifier, actor, resource, and audit counts in dual-tenant creation, per FR-013/FR-015/FR-017 and T124/T126 (partial)
- [X] T149 make all unexpected customer collection, detail, and subresource methods or paths fail explicitly in Playwright mocks, add DELETE regressions for both route shapes, align pagination/selectors/history fixtures with production behavior, and remove optional guards from required interactions, per Constitution VII and T132 (partial)
- [X] T150 use the shared empty-state pattern for empty identifier and metadata sections, replace remaining feature-local spacing and typography literals with established tokens, and add uniquely associated labels/errors plus narrow-screen row layouts for the customer dialog and profile, per Constitution IX and T129 (partial)
- [X] T151 correct customer, conversation, and feature data-model documentation to state actual ownership and server composition, the sole `conversations -> customers` dependency, composite foreign keys, live-row uniqueness, identifier soft deletion, and physical retention versus customer-history API availability, per Constitution I, Documentation & Future Readiness, and T130 (contradicts)
- [X] T152 add frontend PATCH service tests using real snake_case wire bodies for successful updates, explicit clears, 409 holder details, and 422 field details, per plan: frontend testing and T073 (missing)
- [X] T153 add `pnpm format:check` to the mandatory frontend CI verification job so CI enforces every documented frontend quality gate, per T052 and quickstart: frontend validation (partial)

## Phase 12: Convergence

- [X] T154 CRITICAL make the customer real-backend Playwright suite actually execute and fail closed in CI by setting `CI_REAL_BACKEND=true` in the `real-backend-e2e` job's test-runner environment (or removing the env-gated `test.skip`) and extending the mandatory wiring-validation step to guard the customer suite's presence and execution, per Constitution VII and T136 (contradicts)
- [X] T155 cover pagination/load-more, conversation history, update/edit, duplicate-identifier conflict, Viewer server-side refusal, and cross-tenant not-found in `frontend/e2e/customer-profiles.real.spec.ts` without optional interaction guards, seeding the suite's own customers, conversations, and a genuine tenant-A Viewer identity instead of reusing seed user `…005` who is a foreign-tenant manager, per Constitution VII and T136 (partial)
- [X] T156 assert non-empty error-body `request_id` values equal to the `X-Request-Id` response header for customer and conversation duplicate-identifier conflicts, 403s, 404s, malformed POST/PATCH payloads, and history failures in `backend/crates/server/tests/customers.rs`, per Constitution VI and T137 (partial)
- [X] T157 add a migration reconciling pre-normalization `customer_channel_identifiers` rows to the canonical trimmed/lowercased-email/E.164 representation, or record in the feature data model the justified decision that no backfill is required, per FR-003/FR-014 and T140 (partial)

## Phase 13: Convergence

- [X] T158 restore real-backend search and successful-create coverage in `frontend/e2e/customer-profiles.real.spec.ts` — a search flow asserting seeded-name filtering through the live API and a create flow asserting POST 201 with the new customer appearing in the list — and add matching evidence entries to the wiring-validation guard, per Constitution VII and T136/T155 (partial)
- [X] T159 make migration `backend/migrations/0032_normalize_identifiers.sql` collision-safe by resolving live identifier rows that normalize to the same `(tenant_id, channel, identifier)` value before the normalizing UPDATEs (e.g. soft-delete newer duplicates), or record in the feature data model why such collisions cannot occur, per FR-003/FR-014 and T157 (partial)

## Phase 14: Convergence

- [X] T160 CRITICAL suppress both the `customer.updated` audit row and the `updated_at` refresh for normalized no-op PATCH requests so `patch_normalized_noop_suppresses_audit` and `patch_dual_tenant_noop_and_conflict_snapshot` pass against a live database, per Constitution VII, FR-008/FR-017, and T082/T097/T148 (contradicts)
- [X] T161 CRITICAL persist explicit JSON `null` contact clears in the PATCH handler so `patch_intentional_clear_emits_null` passes against a live database, per Constitution VII, FR-008, and T083/T101/T127 (contradicts)
- [X] T162 CRITICAL populate `error.request_id` in authz/tenant-middleware 403 rejection bodies so the request-id assertion in `viewer_is_forbidden_manage_roles_succeed_and_cross_tenant_patch_returns_404_with_row_unchanged` passes, per Constitution VI and T137 (contradicts)
- [X] T163 fix soft-deleted-holder identifier reuse so exactly one live `(tenant_id, channel, identifier)` row remains after reuse and `soft_deleted_holder_identifier_can_be_reused` passes, per FR-014, plan: live-row uniqueness, and T093/T119 (contradicts)
- [X] T164 fix the EXPLAIN plan-row decoding (`String` vs SQL `JSON` ColumnDecode at `backend/crates/server/tests/customers.rs:614`) so `name_search_stays_within_the_sc_002_budget_at_ten_thousand_customers` executes and enforces the SC-002 budget, per SC-002 and T147 (partial)
- [X] T165 after T160–T164 pass, re-run all backend/frontend gates, execute and record the manual quickstart walkthrough with SC-001/SC-004 timing and audit-row evidence, and update the quickstart sign-off checklist truthfully (no gate recorded green while failures exist), per SC-001/SC-004, Constitution VII, and T145 (contradicts)
- [X] T166 add evidence entries for the real-backend search and successful-create flows to the mandatory scenario list in `frontend/e2e/real-backend-workflow.validation.test.mjs`, per Constitution VII and T158 (partial)
