# Tasks: Backend Core Infrastructure

**Input**: Design documents from `specs/004-backend-core-infra/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/core-http.md, quickstart.md

**Tests**: INCLUDED — the spec requires them (FR-018, SC-007, "Tests cover health and error behavior") and the constitution mandates test-first. Within each story, test tasks come first and must fail before implementation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Environment note**: The Rust toolchain is not installed on the current dev machine (see plan.md). T001 makes verification possible; without it, tasks can be authored but not verified.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US4)

## Path Conventions

Cargo workspace at `backend/`; crates under `backend/crates/` (`server`, `shared/config`, `shared/db`, `shared/cache` (new), `shared/kernel`, `shared/observability`). All paths below are repo-relative.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Toolchain and dependency groundwork for every later task

- [X] T001 Install the Rust toolchain via rustup (stable) and verify `cargo --version` works from `backend/` (see quickstart.md Prerequisites; required before any test/build task can be verified)
- [X] T002 Add workspace dependencies to `backend/Cargo.toml` `[workspace.dependencies]`: `redis` (features `tokio-comp`, `connection-manager`), `tower-http` (features `cors`, `catch-panic`), and `http-body-util` (dev/test body reading); confirm the workspace still resolves with `cargo check` (research.md R1, R6, R10)
- [X] T003 Create the new crate `backend/crates/shared/cache/` with `Cargo.toml` (name `cache`, workspace version/edition, deps: `redis`, `tokio`, `tracing`) and an empty `src/lib.rs`; verify the `crates/shared/*` workspace-member glob picks it up via `cargo check -p cache`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Config, state, and server skeleton that every user story builds on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 [P] Extend `AppConfig` in `backend/crates/shared/config/src/lib.rs` with the full data-model.md matrix: `environment` as enum (`production|staging|development|test`), `cors_allowed_origins: Vec<String>` (comma-separated, origin-validated, non-empty required in production), `log_format: LogFormat` enum (`json|pretty`, environment-dependent default), `db_max_connections` (default 10), `db_acquire_timeout_ms` (default 3000), `ready_probe_timeout_ms` (default 2000), `shutdown_grace_seconds` (default 10); implement a `Debug` impl that redacts `database_url`/`redis_url` (FR-002, FR-011)
- [X] T005 [P] Unit tests for config in `backend/crates/shared/config/src/lib.rs` (`#[cfg(test)]`, using explicit var maps rather than process env where possible): missing required var → descriptive error naming the variable; invalid port/enum/origin → error; defaults applied; production requires non-empty CORS origins; `Debug` output contains no secret values (FR-002, SC-005)
- [X] T006 [P] Add lazy pool construction to `backend/crates/shared/db/src/lib.rs`: `pub fn lazy_pool(database_url: &str, max_connections: u32, acquire_timeout: Duration) -> PgPool` using `PgPoolOptions::connect_lazy_with` so construction never dials Postgres (research.md R2, FR-007, FR-008a); add `sqlx` `time`-related imports as needed and a unit test asserting construction succeeds with an unreachable URL
- [X] T007 [P] Implement the `Cache` wrapper in `backend/crates/shared/cache/src/lib.rs`: holds `redis::Client` (constructed without I/O) plus a `tokio::sync::OnceCell<ConnectionManager>` initialized on first use; expose `async fn ping(&self) -> Result<(), String>` returning safe error strings (no URL/credentials) (research.md R1, FR-008, FR-008a, FR-011); unit test: construction with an unreachable URL succeeds
- [X] T008 Make observability init config-driven in `backend/crates/shared/observability/src/lib.rs`: add `config` path dependency to `backend/crates/shared/observability/Cargo.toml`; `init_observability(log_format: LogFormat)` selects `.json()` or pretty fmt layer, keeping `EnvFilter`/`RUST_LOG` behavior (research.md R8, FR-013 format half — request-ID guarantees land in US3)
- [X] T009 Create `backend/crates/server/src/state.rs` with `AppState { config: Arc<AppConfig>, db: PgPool, cache: cache::Cache, health_checks: Vec<Arc<dyn HealthCheck>> }` (Clone); add `cache` and `db` path dependencies plus `tower`/`tower-http` to `backend/crates/server/Cargo.toml` (data-model.md AppState) — note: compiles only after T012 introduces the `HealthCheck` trait if ordered first, so keep `health_checks` behind a placeholder type alias until T012 or execute T012 first when running sequentially
- [X] T010 Split routing out of `backend/crates/server/src/main.rs` into `backend/crates/server/src/router.rs`: `pub fn app(state: AppState) -> Router` preserving current behavior (existing `/api/v1` nest, envelope fallback, request-ID middleware layering); `main.rs` becomes composition root: load `AppConfig` (exit non-zero with descriptive stderr message on failure, <5 s — SC-005), call `init_observability`, build `AppState`, bind, serve
- [X] T011 Add graceful shutdown to `backend/crates/server/src/main.rs`: `axum::serve(...).with_graceful_shutdown(...)` resolving on Ctrl-C (all platforms) or SIGTERM (`#[cfg(unix)]`), bounded by `shutdown_grace_seconds` via `tokio::time::timeout`, with shutdown-start/complete log events (research.md R9, FR-016)

**Checkpoint**: `cargo check --workspace` passes; server boots from env config and shuts down gracefully — user story implementation can now begin

---

## Phase 3: User Story 1 — Operator verifies the service is alive and ready (Priority: P1) 🎯 MVP

**Goal**: Root-level `GET /health` (liveness, no dependency consultation) and `GET /ready` (Postgres + Redis probes, per-dependency status, 503 + HealthReport body on failure), with boot-unready-and-recover semantics

**Independent Test**: quickstart.md scenario 1 — start with dependencies up: both endpoints 200; stop Redis: `/ready` 503 naming `cache`, `/health` still 200; restart Redis: `/ready` recovers without process restart

### Tests for User Story 1 (write first, must fail before implementation)

- [X] T012 [US1] Define the `HealthCheck` trait (`name() -> &'static str`, `async fn check() -> Result<(), String>`) and `HealthReport`/`CheckResult` serde types in new file `backend/crates/shared/observability/src/health.rs` with unit tests for report rendering (`ready` iff all ok; `error` field only on failure) — trait must exist first so both tests and impls compile (data-model.md, research.md R3)
- [X] T013 [P] [US1] Router tests in `backend/crates/server/tests/health.rs` using `tower::ServiceExt::oneshot` with mock `HealthCheck` implementations: `/health` → 200 `{"status":"ok"}` without invoking any check; `/ready` all-ok → 200 `ready` with both named checks; db-failing → 503 with `database: error`, `cache: ok`; cache-failing → 503 mirrored; a check that sleeps past `ready_probe_timeout_ms` → 503 with `"timed out"` and endpoint returns within the ceiling (FR-004, FR-005, FR-006, contract shapes from contracts/core-http.md)

### Implementation for User Story 1

- [X] T014 [US1] Implement `/health` and `/ready` handlers in `backend/crates/shared/observability/src/health.rs`: liveness returns static ok; readiness runs all registered checks concurrently (`futures`/`join_all` or `tokio::join`), each wrapped in `tokio::time::timeout(ready_probe_timeout_ms)`, renders `HealthReport`, status 200/503; delete the obsolete stub `health`/`ready` handlers from `backend/crates/shared/observability/src/lib.rs` (FR-004, FR-005, FR-006)
- [X] T015 [P] [US1] Implement `PgHealthCheck` (name `database`, `SELECT 1` via the pool) in `backend/crates/shared/db/src/lib.rs`, with safe error mapping (no connection string in messages) (FR-007, FR-011)
- [X] T016 [P] [US1] Implement `RedisHealthCheck` (name `cache`, delegates to `Cache::ping`) in `backend/crates/shared/cache/src/lib.rs` (FR-008)
- [X] T017 [US1] Wire routes in `backend/crates/server/src/router.rs` and `state.rs`: `/health`, `/ready`, `/metrics` (existing stub) at root — removed from the `/api/v1` nest (research.md R12); seed `AppState.health_checks` with `PgHealthCheck` + `RedisHealthCheck` in `main.rs`; T013 tests now pass
- [X] T018 [US1] Env-gated live integration test in `backend/crates/server/tests/live_deps.rs`: when `TEST_DATABASE_URL` and `TEST_REDIS_URL` are both set, real probes succeed end-to-end (`/ready` 200); test exits early (skips) when unset so the default suite needs no infrastructure (research.md R11)

**Checkpoint**: MVP — quickstart scenario 1 fully reproducible; `cargo test -p server -p observability -p db -p cache` green without live services

---

## Phase 4: User Story 2 — API consumers receive consistent, structured errors (Priority: P2)

**Goal**: Every non-2xx response (except failing `/ready`) is the v1 error envelope with correct status↔code mapping, request-ID stamping at the boundary, and zero internal-detail leakage — including panics

**Independent Test**: quickstart.md scenario 2 — unknown route → 404 envelope; malformed JSON body → 400 envelope; forced internal failure → 500 envelope with generic message, full detail only in logs

### Tests for User Story 2 (write first, must fail before implementation)

- [X] T019 [P] [US2] Unit tests in `backend/crates/shared/kernel/src/lib.rs` (`#[cfg(test)]`): each new constructor produces the contract's status + `error.code` pair (400 `validation_failed`, 401 `unauthenticated`, 403 `unauthorized`, 404 `not_found`, 409 `conflict`, 422 `unprocessable`, 429 `rate_limited`, 500 `internal_error`); `details[]` serializes per contract; envelope JSON shape matches contracts/core-http.md exactly (FR-009, FR-010)
- [X] T020 [P] [US2] Router tests in `backend/crates/server/tests/errors.rs` (using test-only routes registered behind `#[cfg(test)]`/a test router extension): unknown route → 404 envelope with `request_id` matching `^req_`; POST with malformed JSON to a JSON-extracting test route → 400 envelope; a test route that panics → 500 envelope whose message contains no panic text/paths, process keeps serving a subsequent request (FR-009, FR-010, FR-011, edge case "handler panics")

### Implementation for User Story 2

- [X] T021 [US2] Extend `ApiError` in `backend/crates/shared/kernel/src/lib.rs`: constructors for the full status map above, `with_details(Vec<ErrorDetail>)`, and stop generating a random `request_id` inside `ApiError::new` (leave it empty; the boundary layer owns stamping) (FR-009, FR-010, research.md R10)
- [X] T022 [US2] JSON-rejection mapping in `backend/crates/shared/kernel/src/lib.rs` (or new `backend/crates/shared/kernel/src/extract.rs`): an `ApiJson<T>` extractor wrapping `axum::Json` whose rejection converts to `ApiError::validation` with a safe message, so all future endpoints inherit envelope-shaped 400s (FR-009, FR-010)
- [X] T023 [US2] Boundary layers in `backend/crates/server/src/router.rs`: (a) `tower_http::catch_panic::CatchPanicLayer` with a custom responder producing the 500 envelope and logging full panic detail at `error` level under the request span; (b) a `map_response`/middleware layer that buffers envelope error bodies and stamps `error.request_id` + guarantees the `X-Request-Id` header from the request context, replacing the fallback's manual header threading (FR-011, FR-012 stamping half, research.md R10); T019+T020 now pass

**Checkpoint**: All error paths envelope-shaped and leak-free; US1 behavior unchanged (`/ready` 503 still HealthReport)

---

## Phase 5: User Story 3 — Every request is traceable end to end (Priority: P3)

**Goal**: `req_<UUIDv7>` request IDs — generated or validated-and-honored — on every response header, every request-scoped log record, and a per-request trace span with a completion summary record

**Independent Test**: quickstart.md scenario 3 — response carries `X-Request-Id: req_<uuid>`; valid inbound ID echoed unchanged; malformed inbound ID replaced; logs show the same ID on all records for the request plus a method/path/status/latency summary

### Tests for User Story 3 (write first, must fail before implementation)

- [X] T024 [P] [US3] Unit tests for the format module in `backend/crates/shared/observability/src/request_id.rs`: `generate()` output matches `^req_[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$` (40 chars) and is monotonically sortable across sequential calls; `validate()` accepts the canonical form and rejects: missing prefix, uppercase UUID, non-UUID suffix, overlong values, empty, `<script>` (FR-012, contracts/core-http.md, research.md R5)
- [X] T025 [P] [US3] Router tests in `backend/crates/server/tests/tracing.rs`: no inbound header → response header matches the format; valid inbound `req_…` → echoed verbatim; malformed inbound → replaced (differs from input, matches format); with a JSON-format capture subscriber (custom test `MakeWriter` buffer), a handled request emits records all carrying the same `request_id` and one summary record containing `method`, `path`, `status`, `latency_ms` (FR-012, FR-013, FR-014, SC-004)

### Implementation for User Story 3

- [X] T026 [US3] Extract and harden request-ID handling into `backend/crates/shared/observability/src/request_id.rs`: `REQUEST_ID_HEADER` constant, `generate()` (`req_` + `Uuid::now_v7()`), `validate()` per R5 (prefix + `Uuid::parse_str` on suffix + length 40), middleware moved from `lib.rs` and tightened from "any non-empty" to validate-or-replace; request ID stored in request extensions as `RequestContext` for downstream layers (FR-012, data-model.md RequestContext)
- [X] T027 [US3] Trace middleware in new file `backend/crates/shared/observability/src/trace.rs`: per-request `tracing::info_span` with `request_id`/`method`/`path` fields entered for the handler chain; on completion emit one `info` summary event with `status` and `latency_ms` (research.md R7, FR-013, FR-014)
- [X] T028 [US3] Wire layer ordering in `backend/crates/server/src/router.rs`: request-ID middleware outermost, then trace span, then catch-panic/stamping (US2 layers), so every log record — including panic and fallback paths — carries the final ID; T024+T025 now pass (FR-013, FR-014)

**Checkpoint**: Given only an `X-Request-Id` value, all log records for that request are locatable; US1/US2 tests still green

---

## Phase 6: User Story 4 — Browser clients can call the API across origins (Priority: P4)

**Goal**: Config-driven CORS allowlist: allowed origins get the standard method/header grants (exposing `x-request-id`); all other origins get no grant

**Independent Test**: quickstart.md scenario 4 — preflight from `http://localhost:4200` (configured) returns the allow headers; preflight from `http://evil.example` returns no `Access-Control-Allow-Origin`

### Tests for User Story 4 (write first, must fail before implementation)

- [X] T029 [US4] Router tests in `backend/crates/server/tests/cors.rs`: OPTIONS preflight from an allowed origin → `Access-Control-Allow-Origin` echoes the origin, allowed methods include GET/POST/PATCH/PUT/DELETE/OPTIONS, allowed headers include `content-type`, `authorization`, `x-request-id`, `idempotency-key`, and `x-request-id` is exposed; preflight from a non-listed origin → no allow-origin header; simple GET from allowed origin carries the grant (FR-015, SC-006, contracts/core-http.md CORS)

### Implementation for User Story 4

- [X] T030 [US4] CORS layer builder in `backend/crates/server/src/router.rs` (helper `fn cors_layer(config: &AppConfig) -> CorsLayer`): exact-origin allowlist parsed from `config.cors_allowed_origins`, method/header/expose grants per contracts/core-http.md, no wildcard; wire into the router stack outside the `/api/v1` nest so future business routes inherit it; T029 now passes (FR-015, research.md R6)

**Checkpoint**: All four user stories independently verified

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Constitution-mandated documentation, workspace-wide quality gates, and final validation

- [X] T031 [P] Create `backend/.env.example` documenting every variable from the data-model.md config matrix with safe example values and required/optional/default annotations (no real secrets)
- [X] T032 [P] Add crate-level doc comments (`//!` Purpose / Responsibilities / Public Interfaces / Dependencies / Extension Points, per constitution "Documentation & Future Readiness") to `backend/crates/shared/cache/src/lib.rs`, `backend/crates/shared/observability/src/lib.rs`, `backend/crates/shared/config/src/lib.rs`, `backend/crates/shared/db/src/lib.rs`, `backend/crates/shared/kernel/src/lib.rs`, and `backend/crates/server/src/main.rs`
- [X] T033 Verify FR-018 coverage checklist against the suite (each enumerated case maps to a named test) and run the full gate from `backend/`: `cargo test --workspace`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` — all green
- [ ] T034 Execute quickstart.md scenarios 1–5 end-to-end against locally provisioned Postgres + Redis (or document precisely which scenarios could not run if local infrastructure is unavailable) and check off the "Done when" list

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 unblocks all verification; T002 → T003
- **Foundational (Phase 2)**: depends on Phase 1; T004 → T005; T004 → T008/T009/T010 (config fields consumed); T009 → T010 → T011; **blocks all user stories**
- **US1 (Phase 3)**: depends on Phase 2; T012 → T013/T014; T014+T015+T16 → T017 → T018
- **US2 (Phase 4)**: depends on Phase 2 (uses the existing scaffolded request-ID middleware; US3 later tightens it without breaking US2); T019/T020 → T021 → T022 → T023
- **US3 (Phase 5)**: depends on Phase 2; T024/T025 → T026 → T027 → T028 (T028 orders US2's layers if present, but US3 is testable without US2)
- **US4 (Phase 6)**: depends on Phase 2 only; T029 → T030
- **Polish (Phase 7)**: T031/T032 anytime after Phase 2; T033/T034 after all desired stories

### User Story Dependencies

All four stories depend only on Foundational — none depends on another story's completion. Shared-file coordination (not logical dependency): US2's T023 and US3's T028 both edit `server/src/router.rs` layering; if executed in parallel, merge T028's ordering last.

### Parallel Opportunities

- Phase 2: T004, T005 (after T004), T006, T007 touch four different crates — parallelizable after T003
- US1: T013 ∥ implementation prep after T012; T015 ∥ T016 (different crates)
- Across stories: after Phase 2, US1/US2/US3/US4 can proceed on separate branches/developers (router.rs merge note above)
- Polish: T031 ∥ T032

## Parallel Example: User Story 1

```bash
# After T012 (trait exists), launch together:
Task: "Router tests with mock HealthChecks in backend/crates/server/tests/health.rs"   # T013
Task: "PgHealthCheck in backend/crates/shared/db/src/lib.rs"                            # T015
Task: "RedisHealthCheck in backend/crates/shared/cache/src/lib.rs"                      # T016
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 (T001–T003) — toolchain + deps
2. Phase 2 (T004–T011) — config/state/server skeleton
3. Phase 3 (T012–T018) — health/ready
4. **STOP and VALIDATE**: quickstart scenario 1 + `cargo test` — deployable, orchestrator-probeable service

### Incremental Delivery

Each subsequent story (US2 errors → US3 tracing → US4 CORS) lands as an independently testable increment; quickstart has a per-story scenario. T033/T034 close the feature.

## Notes

- Test tasks must be written and observed failing before their story's implementation tasks (constitution Principle VII)
- Commit after each task or logical group
- `/ready`-failure body is the HealthReport shape, never the envelope — don't "fix" that in US2 (spec clarification Q2)
- `ApiError::new` currently self-generates request IDs — removing that in T021 is intentional (boundary stamping owns it)

---

## Phase 8: Convergence

**Purpose**: Close gaps found by `/speckit-converge` between what Phases 1–7 claim is done and what the code actually does. Evidence for each item is in the convergence findings report (session of 2026-07-07).

- [X] T035 Replace `CatchPanicLayer::new()` in `backend/crates/server/src/router.rs` with `CatchPanicLayer::custom(...)` using a handler that produces the standard JSON error envelope (`kernel::ApiError::internal_error`, stamped with the request ID, no panic text/paths in the body — full detail logged server-side instead) per FR-009, FR-010, FR-011, Constitution V (contradicts)
- [X] T036 Reorder `backend/crates/server/src/router.rs` so `/health`, `/ready`, `/metrics`, the `/api/v1` nest, and the root fallback are all registered before any `.layer()` call, so `stamp_request_id_middleware`, `CatchPanicLayer`, and `trace_middleware` wrap the entire router (not just routes registered before `.nest()`/`.fallback()` per axum's layer-at-call-time semantics) per Constitution VI, FR-011, FR-013, FR-014, plan.md Project Structure (contradicts)
- [X] T037 Replace the `let _guard = span.enter(); next.run(request).await` pattern in `backend/crates/shared/observability/src/trace.rs` with `.instrument(span)` so the span guard is never held across an await point (tracing's documented anti-pattern; risks log/span misattribution across concurrently-running requests) per FR-013, FR-014, SC-004 (contradicts)
- [X] T038 Add the missing FR-018 test cases to `backend/crates/server/tests/errors.rs`: a malformed-JSON-body request to a route using the `ApiJson` extractor asserting a 400 envelope, and a panicking test route asserting a 500 envelope with no panic text/paths in the body and the server continuing to serve a subsequent request afterward per FR-018, US2/AC2, US2/AC3 (missing)
- [X] T039 Add a configurable bind address (e.g. `BIND_ADDRESS`, default `0.0.0.0`) to `AppConfig` in `backend/crates/shared/config/src/lib.rs` and use it in `backend/crates/server/src/main.rs` instead of the hardcoded `0.0.0.0` per FR-001 (partial)
- [X] T040 Change the empty-`cors_allowed_origins` fallback in `cors_layer()` (`backend/crates/server/src/router.rs`) from `AllowOrigin::any()` to a deny-all origin list, so an empty allowlist denies all cross-origin requests rather than granting universal access per FR-015 (contradicts)
- [X] T041 Consolidate or remove the redundant request-ID response-stamping logic duplicated between `backend/crates/server/src/middleware.rs` (`stamp_request_id_middleware`) and `backend/crates/shared/observability/src/request_id.rs` (`request_id_middleware`) per research.md R7, R10 (unrequested)

---

## Phase 9: Convergence

**Purpose**: Close a gap introduced by the Phase 8 fixes (T035–T041 were re-verified against the current source, not just their checkboxes, before this phase was appended).

- [X] T042 Remove `POST /test-echo` and `GET /test-panic` from `backend/crates/server/src/router.rs`'s `pub fn app()` (currently ungated, unauthenticated, and permanently reachable in the production binary) and instead expose them only to the test build via a separate `pub fn app_with_test_routes()` that `backend/crates/server/tests/errors.rs` calls explicitly — so production deployments never serve these two endpoints per Constitution III (contradicts)
