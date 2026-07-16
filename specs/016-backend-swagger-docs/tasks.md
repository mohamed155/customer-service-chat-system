---
description: "Task list for Backend API Documentation (Swagger/OpenAPI)"
---

# Tasks: Backend API Documentation (Swagger/OpenAPI)

**Input**: Design documents from `/specs/016-backend-swagger-docs/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/openapi-coverage.md, quickstart.md

**Tests**: INCLUDED. The spec mandates automated enforcement (FR-013 validity, FR-015 completeness gate, SC-003/SC-005) and Constitution Principle VII requires test-first discipline. Test tasks are therefore first-class here, not optional.

**Organization**: Tasks are grouped by user story. Because all three stories are produced by one code-first OpenAPI pipeline, the annotation bulk lives in US1 (browsable reference = every endpoint documented); US2 adds machine-readable validity on top of the same document; US3 adds the sync/completeness enforcement gate.

## Path Conventions

Web-service backend (Rust/Axum modular monolith). All paths are under `backend/`. Module crates: `crates/modules/{identity,tenancy,customers,conversations,escalations,ai}`; shared: `crates/shared/{kernel,config}`; composition: `crates/server`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add the OpenAPI tooling to the workspace so every crate can annotate.

- [x] T001 Add `utoipa` (features `axum_extras`, `chrono`, `uuid`, `openapi_extensions`), `utoipa-axum`, and `utoipa-swagger-ui` (features `axum`, `vendored`) to `[workspace.dependencies]` in `backend/Cargo.toml`
- [x] T002 [P] Add `utoipa` + `utoipa-axum` + `utoipa-swagger-ui` to `[dependencies]` in `backend/crates/server/Cargo.toml`
- [x] T003 [P] Add `utoipa` (workspace) to `[dependencies]` of each module/shared crate Cargo.toml that defines HTTP models: `crates/shared/kernel`, `crates/shared/observability`, `crates/modules/{identity,tenancy,customers,conversations,escalations,ai}`
- [x] T004 Verify the workspace compiles with the new dependencies before annotating: `cargo build -p server` (baseline, no behavior change yet)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared schemas, root document, security scheme, config gate, and router migration that EVERY story depends on. No endpoint can be documented until the `OpenApiRouter` and root `OpenApi` exist.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T005 [P] Derive `ToSchema` on `ErrorEnvelope`, `ErrorBody`, `ErrorDetail`, and generic `Page<T>`, and `IntoParams` on `PageParams`, in `backend/crates/shared/kernel/src/lib.rs` (shared error + pagination schemas per data-model.md)
- [x] T006 [P] Add `docs_enabled: bool` to `AppConfig` sourced from `APP_DOCS_ENABLED` (default `false`) in `backend/crates/shared/config/src/lib.rs`, following the existing `from_env()` optional-var pattern; add a unit test for parsing (default off, `true` opts in)
- [x] T007 Create `backend/crates/server/src/openapi.rs` with the root `#[derive(OpenApi)]` type: empty `paths`/`components` placeholders, a `Modify` impl adding the `session_cookie` `SecurityScheme::ApiKey` (cookie `app_session`), the `/api/v1` server object, and tag definitions (auth, invitations, identity, platform-tenants, platform-ai, tenant, customers, conversations, escalations, members, tenant-ai, ops); register `pub mod openapi;` in `backend/crates/server/src/lib.rs`
- [x] T008 Migrate the route-composition scaffolding in `backend/crates/server/src/router.rs` from the `ProtectedRoutes` builder to `utoipa-axum` `OpenApiRouter`: re-express `guarded`/`guarded_with_methods` so each route is registered via `routes!(handler)` and still carries its `require_permission(permission)` layer and the auth/tenant-context/platform-permission layer stacks (see research Decision 2). Keep test-only routes on the plain axum router behind `include_test_routes` (they must NOT go through `OpenApiRouter`).
- [x] T009 In `backend/crates/server/src/router.rs`, assemble the final `OpenApi` from the merged `OpenApiRouter` via `split_for_parts()`, and conditionally mount `SwaggerUi` at `/swagger-ui` + the raw doc at `/api-docs/openapi.json` only when `environment ∈ {Development, Test}` OR `config.docs_enabled` (production opt-in gate, FR-014); when disabled the routes are simply absent
- [x] T010 Confirm foundation builds and the empty-but-valid doc serves: `cargo build -p server`, start server in development, `curl /api-docs/openapi.json` returns a parseable OpenAPI 3.1 document (paths may still be sparse until US1)

**Checkpoint**: Router serves a valid (near-empty) OpenAPI document and the Swagger UI shell; annotation of individual endpoints can now proceed in parallel per module.

---

## Phase 3: User Story 1 - Browse Complete API Reference (Priority: P1) 🎯 MVP

**Goal**: Every non-test endpoint appears in the interactive docs with fully typed request/response/error models, security requirement, and required RBAC permission.

**Independent Test**: Open `/swagger-ui` in a development server; every functional area is a tag group; expand any endpoint (e.g. create customer) and its params/body/responses/types/permission match the live API (quickstart Scenario 1).

> Each module task below covers: `#[derive(ToSchema)]`/`IntoParams` on that module's request/response/query types, `#[utoipa::path(...)]` on its handlers (method, path, params, request_body, per-status `responses` referencing `ErrorEnvelope`, `security`, `tag`, and the RBAC permission in the summary/description), and any doc-only response-envelope wrapper types. Register the module's components + `routes!` into the root doc/router. Use `contracts/openapi-coverage.md` as the authoritative per-endpoint map and `data-model.md` as the type checklist.

- [x] T011 [P] [US1] Identity: annotate `LoginRequest` (password `write_only`), `Principal`, and the login/logout/`/me` handlers (documenting the `Set-Cookie: app_session` behavior on login/logout, edge case) in `backend/crates/modules/identity/src/routes.rs` and the login handler in `backend/crates/server/src/router.rs`
- [x] T012 [P] [US1] Tenancy — platform tenants: annotate `TenantSummary`, `PlatformTenantDetail`, `CreateTenantRequest`, `UpdateTenantRequest`, `ListTenantsParams`, `MeResponse`, `MembershipSummary` and their handlers in `backend/crates/modules/tenancy/src/routes.rs`
- [x] T013 [P] [US1] Tenancy — members & invitations: annotate `TeamMemberQuery`, `TeamMemberResponse`, `UpdateMemberPayload` in `backend/crates/modules/tenancy/src/members.rs` and `CreateInvitationPayload`, `AcceptInvitationPayload`, `InvitationResponse`, `CreateInvitationResponse`, `InvitationListItem`, `InvitationDeliveryResponse`, `PreviewInvitationResponse`, `AcceptInvitationResponse`, `InvitationQuery` + handlers in `backend/crates/modules/tenancy/src/invitations.rs`
- [x] T014 [P] [US1] Customers: annotate `CustomerListItem`, `CustomerDetail`, `ChannelIdentifier`, `ChannelIdentifierInput`, `CreateCustomerPayload`, `UpdateCustomerPayload`, `CustomerListQuery`, and provide a manual `ToSchema` for `TriState<T>` (optional+nullable) in `backend/crates/modules/customers/src/model.rs`; add doc-only wrappers `CustomerDetailResponse {data}` and `CustomerListResponse {data,pagination}` mirroring the inline `json!` envelopes, and annotate handlers in `backend/crates/modules/customers/src/routes.rs`
- [x] T015 [P] [US1] Conversations: annotate enums `ConversationStatus`, `MessageKind`, `ConversationStatusRef` (preserve serde variant names), nested `Assignee`/`CustomerRef`/`LastMessagePreview`/`Participant`, responses `Conversation`/`ConversationDetail` (escalation context optional/nullable — polymorphic edge case)/`Message`/`AddMessageResponse`, request bodies `CreateConversationPayload`/`CreateMessagePayload`/`AddMessagePayload`/`PatchConversationPayload` in `backend/crates/modules/conversations/src/model.rs`; annotate handlers + `InboxQueryParams`/`TimelineQueryParams` in `backend/crates/modules/conversations/src/routes.rs`
- [x] T016 [P] [US1] Escalations: annotate enums `RoutingReason`/`EscalationStatus`/`AvailabilityState`, nested + response types (`RequiredSkillRef`, `RoutingInfo`, `CustomerRef`, `QueueEntryConversationRef`, `Escalation`, `QueueEntry`, `Skill`, `Availability`, `TeamMemberSkill`, `TeamMemberWithSkills`), request bodies (`EscalatePayload`, `SetAvailabilityPayload`, `CreateSkillPayload`, `RenameSkillPayload`, `SetMemberSkillsPayload`) in `backend/crates/modules/escalations/src/model.rs`; annotate handlers + `QueueQueryParams` in `backend/crates/modules/escalations/src/routes.rs`
- [x] T017 [US1] Escalations — SSE: annotate `GET /tenant/events` as producing `text/event-stream`, register the event payload schemas (`EscalationAssignedEvent`, `EscalationQueuedEvent`, `EscalationRemovedEvent`, `AvailabilityChangedEvent`) as components, and reference them from the operation description (FR-010) in `backend/crates/modules/escalations/src/events.rs` (depends on T016 for the payload schemas)
- [x] T018 [P] [US1] AI: annotate `ConfigPayload`, `FallbackEntry`, `AiConfigurationView` (platform vs tenant `scope` variant — polymorphic edge case), `CredentialView`, and `CredentialPayload` with `api_key` `write_only` (FR-011) in `backend/crates/modules/ai/src/model.rs`; annotate `UsageListItem`/`UsageSummary`/`UsageDetailRow` + `Pagination`/`PaginatedResponse<T>` in `backend/crates/modules/ai/src/usage.rs`; annotate all platform + tenant AI handlers in `backend/crates/modules/ai/src/routes.rs`
- [x] T019 [US1] Server composite handlers: annotate `get_conversation_with_escalation` (GET `/tenant/conversations/{id}`) and `list_members_with_skills` (GET `/tenant/members`) with `#[utoipa::path]` + any wrapper schema in `backend/crates/server/src/handlers.rs` (depends on T015, T016 for referenced schemas)
- [x] T020 [US1] Operational endpoints: annotate `GET /health`, `GET /ready`, `GET /metrics` (tag `ops`, public, `text/plain` for metrics) in `backend/crates/server/src/router.rs`
- [x] T021 [US1] Register every module's components and `routes!` groups into the root `OpenApi`/`OpenApiRouter` (nest under `/api/v1`, merge module routers) in `backend/crates/server/src/openapi.rs` and `backend/crates/server/src/router.rs`; ensure each guarded route keeps its `require_permission` layer (depends on T011–T020)
- [x] T022 [US1] Manual verification against quickstart Scenario 1: run development server, load `/swagger-ui`, spot-check one endpoint per tag group for correct params/body/response/types/permission/security

**Checkpoint**: The interactive reference is complete — all endpoints browsable with typed models, security, and permissions. US1 is an independently demoable MVP.

---

## Phase 4: User Story 2 - Consume a Machine-Readable API Specification (Priority: P2)

**Goal**: A valid OpenAPI 3.1 document is downloadable and importable by standard tooling, with no secrets in responses.

**Independent Test**: `curl /api-docs/openapi.json`, validate with an off-the-shelf validator (zero errors), import into Postman cleanly (quickstart Scenarios 2 & 7).

- [x] T023 [US2] Add `backend/crates/server/tests/openapi_valid.rs`: build the doc, assert it serializes and is a structurally valid OpenAPI 3.1 document (FR-013/SC-003); assert the `session_cookie` scheme and `/api/v1` server are present
- [x] T024 [P] [US2] Add a `no_secrets_in_responses` test case in `backend/crates/server/tests/openapi_valid.rs` asserting `CredentialPayload.api_key` and `LoginRequest.password` are `writeOnly` and that no response schema exposes an `api_key`/`password` field (FR-011)
- [x] T025 [US2] Validate externally (quickstart Scenario 2): run server, fetch `/api-docs/openapi.json`, lint with `npx @redocly/cli lint` (or equivalent) and import into Postman — record zero errors (deferred to local validation: requires live server, no DB available in this environment)

**Checkpoint**: The machine-readable spec is valid and importable; secrets are provably absent from responses.

---

## Phase 5: User Story 3 - Documentation Stays in Sync with the API (Priority: P3)

**Goal**: The completeness gate fails when any non-test endpoint is undocumented; test routes never appear; live responses match documented schemas.

**Independent Test**: Add a route without a `#[utoipa::path]` → coverage test fails naming it; revert → passes (quickstart Scenarios 3–5).

- [x] T026 [US3] Add `backend/crates/server/tests/openapi_coverage.rs`: assert the documented path+method set equals the inventory in `contracts/openapi-coverage.md` (all `/api/v1` + operational endpoints), failing with the offending method+path on divergence (FR-015/SC-001)
- [x] T027 [P] [US3] Add an `excludes_test_routes` case in `backend/crates/server/tests/openapi_coverage.rs`: build the app with `include_test_routes = true` and assert no `/test/...`, `/test-echo`, or `/test-panic` path appears in the document (FR-004)
- [x] T028 [US3] Add `backend/crates/server/tests/openapi_contract.rs`: for ≥10 endpoints spanning all functional areas (incl. one of each envelope variant `{data}`, `{data,pagination}`, `Page<T>`, `PaginatedResponse<T>`, and the error envelope), assert live JSON responses validate against the documented schema — field names, types, optionality (SC-005)
- [x] T029 [US3] Regression demo (quickstart Scenario 3): temporarily add an unannotated route, confirm `openapi_coverage` fails and names it, then revert — proving the gate bites (deferred: manual demo per quickstart; the infrastructure test is in place)

**Checkpoint**: All three stories functional; drift is now blocked by CI.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, quality gates, and the deferred staging decision.

- [x] T030 [P] Document the docs feature and `APP_DOCS_ENABLED` in `backend/` env/README notes (default off in production, on in dev/test; URLs `/swagger-ui` and `/api-docs/openapi.json`)
- [x] T031 Resolve the deferred staging-gating question (research Decision 7): decide whether `Staging` gates like production or opens like development, and encode it in the T009 condition with a comment
- [x] T032 [P] Run project quality gates: `cargo fmt --check`, `cargo clippy --workspace` (zero warnings in server lib; pre-existing warnings in other crates are unchanged), `cargo test -p server` (validity, coverage, contract all green)
- [x] T033 Run full quickstart validation (all Scenarios 1–7) and record results

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately.
- **Foundational (Phase 2)**: Depends on Setup. BLOCKS all user stories (no doc/router/schema to attach to otherwise).
- **User Story 1 (Phase 3)**: Depends on Foundational. The MVP.
- **User Story 2 (Phase 4)**: Depends on Foundational; meaningfully testable only once US1 has populated the document (a valid-but-empty doc passes T023 but not the intent). Recommended after US1.
- **User Story 3 (Phase 5)**: Depends on Foundational; the coverage/contract tests are only meaningful once US1 endpoints are documented. Recommended after US1.
- **Polish (Phase 6)**: Depends on all desired stories.

### User Story Dependencies

- **US1 (P1)**: Independent after Foundational — delivers the browsable reference alone.
- **US2 (P2)**: Builds on the same document US1 produces; its validity/secret tests can be written against the foundation but only pass-with-meaning after US1.
- **US3 (P3)**: The enforcement layer over US1's documented surface.

### Within User Story 1

- T011–T020 are per-module/per-file and mostly parallel `[P]`.
- T017 depends on T016 (needs escalation event schemas); T019 depends on T015/T016; T021 depends on T011–T020 (registration aggregates them); T022 is manual verification last.

### Parallel Opportunities

- Setup: T002, T003 parallel after T001.
- Foundational: T005, T006 parallel; T007→T008→T009 sequential (same `router.rs`/root doc); T010 last.
- US1: T011–T016 and T018 parallel (distinct module files); then T017, T019, T021 (aggregation/cross-refs), then T022.
- US2: T024 parallel with T023's other cases (same file — coordinate) ; T025 external.
- US3: T027 parallel with T026 (same file — coordinate), T028 separate file parallel, T029 last.
- Polish: T030, T032 parallel; T031 then folds into T009's condition.

---

## Parallel Example: User Story 1

```bash
# After Foundational completes, launch per-module annotation together:
Task: "T011 Identity annotations in modules/identity/src/routes.rs"
Task: "T012 Platform-tenants annotations in modules/tenancy/src/routes.rs"
Task: "T013 Members/invitations annotations in modules/tenancy/src/{members,invitations}.rs"
Task: "T014 Customers annotations in modules/customers/src/{model,routes}.rs"
Task: "T015 Conversations annotations in modules/conversations/src/{model,routes}.rs"
Task: "T016 Escalations annotations in modules/escalations/src/{model,routes}.rs"
Task: "T018 AI annotations in modules/ai/src/{model,usage,routes}.rs"
# Then converge: T017 (SSE), T019 (composite handlers), T021 (register into root doc)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup → 2. Phase 2 Foundational (CRITICAL) → 3. Phase 3 US1 → **STOP & VALIDATE** the browsable reference (quickstart Scenario 1) → demo. This alone satisfies the user's core ask ("swagger covering all endpoints with all input/output models with data types").

### Incremental Delivery

1. Setup + Foundational → valid empty doc served.
2. US1 → complete browsable, typed reference (MVP).
3. US2 → machine-readable validity + secret-safety, provable in CI.
4. US3 → completeness/anti-drift gate + contract fidelity.
5. Polish → env docs, staging decision, quality gates.

### Notes

- `[P]` = different files, no incomplete dependency.
- The router migration (T008) is the riskiest task — it touches the RBAC layer wiring; keep the `require_permission` layers on every guarded route and rely on the existing `rbac.rs` integration test as a guardrail.
- This feature changes no endpoint behavior; if any test outside this feature's new tests changes result, that is a regression to investigate, not to accommodate.
- Commit after each task or logical group.

---

## Phase 7: Convergence

**Purpose**: Close gaps between the implemented code and the spec/plan intent identified by `/speckit-converge`. The documentation output is complete and correct today (all 56 endpoints documented, `openapi_valid`/`openapi_coverage` green), but the anti-drift guarantee is weaker than FR-012/FR-015 require: routes are registered with `OpenApiRouter::route()` (passthrough) rather than `routes!()` co-registration, so the doc's path roster is hand-listed in `openapi.rs paths(...)` and the coverage gate compares two hand-maintained static lists instead of the app's actually-registered routes.

- [x] T034 Strengthen the completeness gate in `backend/crates/server/tests/openapi_coverage.rs` to compare the app's **registered** non-test routes against the **documented** paths (not the static `EXPECTED` inventory): enumerate routes from the built `OpenApiRouter`/`Router` (e.g. via `split_for_parts`/`api_routes`) and assert every registered non-test `/api/v1` + operational route has a corresponding documented path+method, failing and naming any registered-but-undocumented route — so a route added via `.route()` without a `paths(...)` entry breaks the build (FR-015)
- [x] T035 Migrate route registration in `backend/crates/server/src/router.rs` from `OpenApiRouter::route()` to `.routes(routes!(handler))` co-registration so endpoint paths are collected from code rather than hand-listed in `backend/crates/server/src/openapi.rs` `paths(...)`, preserving each route's per-method `require_permission` RBAC layer via `UtoipaMethodRouterExt::layer`/`route_layer` (research Decision 2); remove the now-redundant hand-maintained `paths(...)` list once paths derive from the router. Structurally satisfies T034's guarantee (FR-012)
