---
description: "Task list for 032-platform-analytics-health"
---

# Tasks: Platform Analytics & System Health

**Input**: Design documents from `/specs/032-platform-analytics-health/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/platform-analytics-api.md, quickstart.md

**Tests**: Included. spec.md Success Criteria SC-002 / SC-007 / SC-008 and plan.md Principle VII require automated coverage per endpoint. Inventory-file edits (`rbac.rs`, `auth.rs`, `openapi_coverage.rs`, `permissions.ts`, nav specs) are **mandatory**, not optional — omitting them breaks compilation or reddens existing suites.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- Every task names its exact file path.

## Path Conventions

- Backend: `backend/crates/...`, migrations `backend/migrations/`
- Frontend: `frontend/apps/dashboard/src/app/...`

## Facts verified at HEAD — read before starting

These were checked against source; do **not** re-derive them, and do not trust contradicting text elsewhere:

| Fact | Value |
|---|---|
| `Permission::ALL` array size | `[Self; 31]` today → becomes `[Self; 32]` |
| `catalog_parity_with_contract` test array | `[&str; 31]` today → becomes `[&str; 32]` |
| Permission string method | `impl fmt::Display` (there is **no** `as_str()`) |
| `openapi_coverage.rs` EXPECTED path format | **no** `/api/v1` prefix, e.g. `("GET", "/platform/audit-logs")` |
| `rbac.rs` PLATFORM_OPERATIONS path format | **with** `/api/v1` prefix, e.g. `"/api/v1/platform/audit-logs"` |
| `HealthCheck::name()` values already registered | `"database"` (PgHealthCheck), `"cache"` (RedisHealthCheck) — exactly the DTO names |
| Handler response body | bare DTO via `Json(dto)` — **no** `{"data": ...}` wrapper (`AnalyticsSummaryResponse` in analytics is dead code; do not copy it) |
| `platform` crate in `server/Cargo.toml` | **absent** — must be added |
| `platform/Cargo.toml` | has **no** `[dependencies]` section at all |
| Frontend route path `overview-placeholder` | must stay unchanged (`breadcrumb.ts:52`, `breadcrumb.spec.ts:82,94,102,105,198,208`) |
| `app.routes.spec.ts:63` | navigates `/platform/overview-placeholder` and asserts text contains `Platform overview` |
| `platform-nav.component.spec.ts:94-98` | asserts destinations **by index** — adding entries breaks it |
| Migration numbering | max is `0059`; `0055`/`0058` are absent. Next is `0060`. |

> `cargo fmt --check` is red repo-wide at HEAD for unrelated reasons. Only check formatting of files this feature touches.

---

## Phase 1: Setup

**Purpose**: Make the placeholder `platform` crate compile as part of the workspace.

- [ ] T001 Replace the whole of `backend/crates/modules/platform/Cargo.toml` with the package block it already has plus this dependency block:

  ```toml
  [dependencies]
  axum.workspace = true
  chrono = { workspace = true, features = ["serde"] }
  kernel = { path = "../../shared/kernel" }
  observability = { path = "../../shared/observability" }
  serde.workspace = true
  sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }
  storage = { path = "../../shared/storage" }
  tokio.workspace = true
  tracing.workspace = true
  utoipa.workspace = true
  uuid.workspace = true
  ```

- [ ] T002 In `backend/crates/server/Cargo.toml`, add `platform = { path = "../modules/platform" }` to `[dependencies]` (alphabetically after `observability`).

- [ ] T003 Create the crate skeleton in `backend/crates/modules/platform/src/`: replace `lib.rs` with a module doc comment (Purpose / Responsibilities / Public Interfaces / Dependencies / Data Model / Extension Points — copy the section headings from `backend/crates/modules/analytics/src/lib.rs`) followed by `pub mod model;`, `pub mod queries;`, `pub mod routes;`. Then create three **empty** files `model.rs`, `queries.rs`, `routes.rs` (empty `.rs` files are valid Rust and compile). Verify with `cd backend && cargo check -p platform`.

**Checkpoint**: `cargo check -p server` passes with the platform crate linked.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Migration + the new permission wired through every exact-inventory file. **No user story can start until this phase is complete** — a missing permission entry fails compilation and reddens `authz` tests.

- [ ] T004 Create `backend/migrations/0060_platform_analytics_indexes.sql` with exactly these four additive indexes and nothing else:

  ```sql
  -- Migration 0060: Platform analytics & health — cross-tenant, time-ranged read paths.
  CREATE INDEX IF NOT EXISTS ai_usage_records_created_idx
      ON ai_usage_records (created_at DESC);

  CREATE INDEX IF NOT EXISTS outbox_pending_created_idx
      ON outbox_events (created_at)
      WHERE processed_at IS NULL;

  CREATE INDEX IF NOT EXISTS outbox_pending_attempts_idx
      ON outbox_events (attempts)
      WHERE processed_at IS NULL;

  CREATE INDEX IF NOT EXISTS conversations_status_active_idx
      ON conversations (status)
      WHERE deleted_at IS NULL;
  ```

- [ ] T005 In `backend/crates/modules/authz/src/permission.rs` add the new permission in three places: (a) the enum — `#[serde(rename = "platform.analytics.view")]` above `PlatformAnalyticsView,`, placed right after the `PlatformAdmin` variant; (b) `pub const ALL` — change the length from `[Self; 31]` to `[Self; 32]` and add `Self::PlatformAnalyticsView,` after `Self::PlatformAdmin,`; (c) the `impl fmt::Display` match — add `Self::PlatformAnalyticsView => "platform.analytics.view",` after the `Self::PlatformAdmin` arm.

- [ ] T006 In `backend/crates/modules/authz/src/permission.rs`, in the `catalog_parity_with_contract` test, change `let contract_codes: [&str; 31]` to `[&str; 32]` and add the string `"platform.analytics.view",` after `"platform.admin",`.

- [ ] T007 In `backend/crates/modules/authz/src/matrix.rs`, add `Permission::PlatformAnalyticsView,` to **all five** platform arrays: `PLATFORM_ALL`, `PLATFORM_DEVELOPER`, `PLATFORM_TENANT_ACCESS`, `PLATFORM_SUPPORT`, `PLATFORM_FINANCE` (FR-001). Missing any one of them fails the `assert_eq!(granted, HashSet::from(Permission::ALL))` test at the bottom of that file.

- [ ] T008 In `backend/crates/server/tests/auth.rs`, add `"platform.analytics.view"` to the `"platformPermissions"` JSON array (~line 245, after `"platform.admin"`).

- [ ] T009 [P] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts`, add `| 'platform.analytics.view'` to the `Permission` union (after `'platform.admin'`).

- [ ] T010 [P] In `frontend/apps/dashboard/src/app/core/router/app-paths.ts`, add two entries to the `platform` object: `systemHealth: 'system-health',` and `aiProviders: 'ai-providers',`. Do **not** rename `overviewPlaceholder`.

- [ ] T011 Run `cd backend && cargo test -p authz` and confirm it passes.

**Checkpoint**: Permission catalog is consistent; migration applies. User stories can begin.

---

## Phase 3: User Story 1 - Platform staff see the whole business at a glance (Priority: P1) 🎯 MVP

**Goal**: `GET /platform/analytics/summary` returns cross-tenant tenant/conversation/AI-usage totals, and the Platform Overview page renders them.

**Independent Test**: Sign in as a Sales platform user, open Platform Overview, confirm tenant totals / active conversations / AI usage render and do not change when the tenant switcher changes tenant.

### Backend — US1

- [ ] T012 [US1] In `backend/crates/modules/platform/src/model.rs`, add the summary DTOs and range validation. All structs derive `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]`; all field names stay `snake_case`:
  - `SummaryQuery { from: Option<String>, to: Option<String> }` (derive `Debug, Deserialize, ToSchema`)
  - `ResolvedRange { from_date: NaiveDate, to_date: NaiveDate, from_ts: DateTime<Utc>, to_ts: DateTime<Utc> }` (derive `Debug, Clone`)
  - `DateRangeDto { from: NaiveDate, to: NaiveDate }`
  - `TenantCountsDto { total: i64, active: i64, suspended: i64, created_in_range: i64 }`
  - `AiUsageTotalsDto { total_calls: i64, successful_calls: i64, failed_calls: i64, input_tokens: i64, output_tokens: i64 }`
  - `PlatformUsageSummaryDto { range: DateRangeDto, tenants: TenantCountsDto, active_conversations: i64, ai_usage: AiUsageTotalsDto, generated_at: DateTime<Utc> }`
  - `pub fn resolve_range(query: SummaryQuery, today: NaiveDate) -> Result<ResolvedRange, String>` — copy the structure of `resolve_query` in `backend/crates/modules/analytics/src/model.rs` but with **no channel handling**: parse `%Y-%m-%d`; default `to = today`, `from = to - 29 days`; error `"from must be on or before to"` when `from > to`; error `"Date range must not exceed 365 days"` when `(to - from).num_days() + 1 > 365`; `from_ts` = `from` at 00:00:00Z, `to_ts` = (`to` + 1 day) at 00:00:00Z (exclusive upper bound).

- [ ] T013 [US1] In `backend/crates/modules/platform/src/queries.rs`, add three functions using `sqlx::query_as::<_, (..)>` / `sqlx::query_scalar` with `.bind(...)` (runtime-checked — do **not** use the `query!` macro). None of them filter by `tenant_id` (FR-005):

  ```rust
  pub async fn tenant_counts(pool: &PgPool, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>)
      -> sqlx::Result<(i64, i64, i64, i64)>
  // SELECT COUNT(*)::bigint,
  //        COUNT(*) FILTER (WHERE status = 'active')::bigint,
  //        COUNT(*) FILTER (WHERE status = 'suspended')::bigint,
  //        COUNT(*) FILTER (WHERE created_at >= $1 AND created_at < $2)::bigint
  // FROM tenants WHERE deleted_at IS NULL

  pub async fn active_conversation_count(pool: &PgPool) -> sqlx::Result<i64>
  // SELECT COUNT(*)::bigint FROM conversations
  // WHERE status IN ('open','pending') AND deleted_at IS NULL

  pub async fn ai_usage_totals(pool: &PgPool, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>)
      -> sqlx::Result<(i64, i64, i64, i64, i64)>
  // SELECT COUNT(*)::bigint,
  //        COUNT(*) FILTER (WHERE status = 'success')::bigint,
  //        COUNT(*) FILTER (WHERE status = 'failure')::bigint,
  //        COALESCE(SUM(input_tokens), 0)::bigint,
  //        COALESCE(SUM(output_tokens), 0)::bigint
  // FROM ai_usage_records WHERE created_at >= $1 AND created_at < $2
  ```

- [ ] T014 [US1] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_analytics_summary(State(pool): State<PgPool>, Query(query): Query<model::SummaryQuery>) -> Response`. Take **no** `TenantContext` (platform routes receive no `X-Tenant-ID`). On `resolve_range` error return `ApiError::unprocessable_entity(msg).into_response()` (422). Call the three queries; on any `Err`, `tracing::error!(%e, "…")` and return `ApiError::internal_error("Failed to load platform summary")`. Return `(StatusCode::OK, Json(dto)).into_response()` — a bare DTO, no `data` wrapper. Add the `#[utoipa::path]` attribute above it, copying the shape from `backend/crates/modules/analytics/src/routes.rs`, with: `get`, `path = "/platform/analytics/summary"`, `tag = "platform-analytics"`, `operation_id = "get_platform_analytics_summary"`, params `from`/`to` as `Option<String>`, responses 200 (`body = model::PlatformUsageSummaryDto`) and 422.

- [ ] T015 [US1] In `backend/crates/server/src/router.rs`, inside `platform_routes(...)` (after the `audit::routes::list_platform_audit_logs` block, ~line 330), add:

  ```rust
  .routes(
      routes!(platform::routes::get_platform_analytics_summary)
          .layer(require_permission(Permission::PlatformAnalyticsView)),
  )
  ```

- [ ] T016 [US1] In `backend/crates/server/tests/openapi_coverage.rs`, add `("GET", "/platform/analytics/summary"),` to the `EXPECTED` array (next to the other `/platform/...` entries). **No `/api/v1` prefix** — the entries in this file are unprefixed.

- [ ] T017 [US1] In `backend/crates/server/tests/rbac.rs`, add `"/api/v1/platform/analytics/summary",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/analytics/summary", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT`. **With `/api/v1` prefix** here.

- [ ] T018 [US1] Create `backend/crates/server/tests/platform_analytics.rs`. Copy the header block (imports, `TEST_ENV`, `test_config()`, `app_state()`, `require_db_tests()`) verbatim from `backend/crates/server/tests/analytics_api.rs`. Add these tests, each guarded by `if !require_db_tests() { return; }`:
  1. Seeds 3 tenants (2 `active`, 1 `suspended`) plus 1 soft-deleted tenant → response `tenants.total == 3` and the soft-deleted one is excluded (FR-006).
  2. Seeds conversations across two tenants in `open`/`pending`/`resolved` → `active_conversations` counts only `open` + `pending`, summed across tenants (FR-007).
  3. Seeds `ai_usage_records` across two tenants → `ai_usage` totals and success/failure split match the seeded numbers (FR-008).
  4. `?from=2026-07-25&to=2026-06-25` → `422`; a span of 400 days → `422` (FR-009).
  5. No query params → response `range` spans the last 30 days inclusive (FR-009).
  6. A tenant-role user (no platform role) calling `GET /api/v1/platform/analytics/summary` → `403` with no aggregate data in the body (FR-003, SC-002).

### Frontend — US1

- [ ] T019 [P] [US1] Create `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts` with wire types matching the JSON exactly (snake_case) plus camelCase app types and a mapper, following the wire/`fromWire` pattern in `core/api/tenant-api.models.ts`:
  - `PlatformSummaryWire` — `{ range: { from: string; to: string }, tenants: { total: number; active: number; suspended: number; created_in_range: number }, active_conversations: number, ai_usage: { total_calls: number; successful_calls: number; failed_calls: number; input_tokens: number; output_tokens: number }, generated_at: string }`
  - `PlatformSummary` — same shape with `createdInRange`, `activeConversations`, `aiUsage`, `totalCalls`, `successfulCalls`, `failedCalls`, `inputTokens`, `outputTokens`, `generatedAt`
  - `export function platformSummaryFromWire(wire: PlatformSummaryWire): PlatformSummary`

- [ ] T020 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview-api.service.ts` — an `@Injectable({ providedIn: 'root' })` service that injects `ApiService` and exposes `getSummary(query: { from?: string; to?: string }): Observable<ApiResponse<PlatformSummary>>` calling `this.api.get<PlatformSummaryWire>('/platform/analytics/summary', params)` and mapping with `platformSummaryFromWire`. Copy the `buildParams` helper style from `features/platform/audit-logs/platform-audit-logs-api.service.ts`.

- [ ] T021 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.store.ts` — a `signalStore` following `features/platform/billing/platform-billing.store.ts`. State: `{ summary: PlatformSummary | null; loading: boolean; error: string | null; from: string | null; to: string | null }`. Methods: `load(query)` via `rxMethod` with `catchError` writing `error` and `loading: false`; `setRange(from, to)` that patches state and re-loads.

- [ ] T022 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.ts` — standalone, `ChangeDetectionStrategy.OnPush`, `providers: [PlatformOverviewStore]`. Template uses `<app-page-container>` + `<app-page-header title="Platform overview" />`. Render that header **unconditionally**, outside every `@if (loading())` / `@if (summary())` block — `app.routes.spec.ts:63` navigates with `provideHttpClientTesting()` and never flushes the request, so the store is still `loading` when it asserts the text `Platform overview` is present. Below the header: a date-range control bound to `setRange`, and `<app-metric-card>` tiles for: total tenants (with active/suspended and created-in-range as sub-text), active conversations, total AI calls, successful/failed calls, input tokens, output tokens. Render `<app-loading-state>` while `loading()`, `<app-inline-alert>` when `error()`, and `<app-empty-state>` with a "No data yet" message when every figure is `0` (Edge Cases). Label the range and `generatedAt` as UTC (FR-029). Add a link to System Health rendered only when `permissions.has('platform.diagnostics.view')` (FR-011, FR-004).

- [ ] T023 [US1] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts`, add `[APP_PATHS.platform.overviewPlaceholder]: 'platform.analytics.view',` to `PAGE_PERMISSIONS`.

- [ ] T024 [US1] In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`, update the `APP_PATHS.platform.overviewPlaceholder` route: change `requiredPermission` from the literal `'platform.admin'` to `PAGE_PERMISSIONS[APP_PATHS.platform.overviewPlaceholder]`, and change `loadComponent` to `import('./overview/platform-overview.component').then((m) => m.PlatformOverviewComponent)`. Leave `path` and `pageTitle: 'platformOverview'` unchanged.

- [ ] T025 [US1] Delete `frontend/apps/dashboard/src/app/features/platform/overview-placeholder/platform-overview-placeholder.component.ts` and its now-empty directory. Confirm with `grep -rn "PlatformOverviewPlaceholderComponent" frontend/` that no references remain.

- [ ] T026 [US1] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, change the existing `'Platform overview'` entry in `PLATFORM_DESTINATIONS` so its `path` is a backtick template literal `/${APP_PATHS.platform.base}/${APP_PATHS.platform.overviewPlaceholder}` (same style as the `Tenants` entry above it) and its `permission` is `'platform.analytics.view'`. Then update `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts`: the index-based assertion at ~line 103 currently expects `routerMock.navigate` to be called with `['/platform']` — change it to `['/platform/overview-placeholder']`. Leave the `options[0..3]` label order assertions unchanged (this task adds no destinations).

- [ ] T027 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.store.spec.ts` — mirror `features/tenant/analytics/analytics.store.spec.ts`: a mock api service returning `of({ data: MOCK_SUMMARY })`, asserting the store populates `summary` and clears `loading`; and a `throwError` case asserting `error` is set and `loading` is `false`.

- [ ] T028 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.spec.ts` — renders metric values from a stubbed store, renders the zero-state message when all figures are `0`, and hides the System Health link when `PermissionsService.has` returns `false` for `'platform.diagnostics.view'`.

**Checkpoint**: US1 is independently shippable — overview endpoint + page work with no health or provider code present.

---

## Phase 4: User Story 2 - Platform engineers can tell whether the system is healthy right now (Priority: P2)

**Goal**: `GET /platform/health` returns a three-state overall verdict plus per-dependency results for database, cache, queue, storage; the System Health page renders them and auto-refreshes every 30 s.

**Independent Test**: Seed unprocessed `outbox_events` and stop a dependency, confirm the page shows the expected degraded/unhealthy states with numeric evidence, and returns to healthy after the backlog clears.

### Storage prerequisites — US2

- [ ] T029 [US2] In `backend/crates/shared/storage/src/lib.rs`, add `async fn probe(&self) -> Result<(), StorageError>;` to the `ObjectStorage` trait, then implement it in both implementors in the same file: `S3Storage::probe` calls `self.client.head_bucket().bucket(&self.bucket).send().await` and maps any error to `StorageError::Other(e.to_string())`; `InMemoryStorage::probe` returns `Ok(())`. Do **not** use `get()` on a reserved key (research R1).

- [ ] T030 [US2] In `backend/crates/modules/knowledge/src/upload.rs` (~line 505), add a `probe` implementation to the `FailingStorage` test double that returns `Err(StorageError::Other("probe failed".into()))`, so the unhealthy path stays covered. This is required for the crate to compile after T029.

- [ ] T031 [US2] In `backend/crates/shared/storage/src/lib.rs`, add the backend descriptor and one centralized selection function:

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum StorageKind { S3, InMemory }

  #[derive(Debug, Clone, Copy)]
  pub struct StorageBackend { pub kind: StorageKind, pub s3_configured: bool }

  pub async fn select_storage(s3: Option<&config::S3Config>)
      -> (std::sync::Arc<dyn ObjectStorage>, StorageBackend)
  ```

  Behaviour: when `s3` is `Some` and `S3Storage::new` succeeds → `(S3Storage, { kind: S3, s3_configured: true })`. When `s3` is `Some` and it fails → keep the existing `tracing::warn!` and return `(InMemoryStorage, { kind: InMemory, s3_configured: true })`. When `s3` is `None` → `(InMemoryStorage, { kind: InMemory, s3_configured: false })`.

- [ ] T032 [US2] In `backend/crates/server/src/main.rs`, replace the inline S3-or-fallback block (~lines 85-95) with `let (storage, _storage_backend) = storage::select_storage(state.config.s3.as_ref()).await;`.

- [ ] T033 [US2] In `backend/crates/server/src/router.rs`, replace the duplicated S3-or-fallback block (~lines 998-1010) with a call to `select_storage`. Because the surrounding function is sync, keep the existing `tokio::runtime::Handle::current().block_on(...)` wrapper. When a `storage` argument was injected by a test (`Some(_)`), do **not** call `select_storage` — use the injected value with the descriptor `StorageBackend { kind: StorageKind::InMemory, s3_configured: false }` (a trait object cannot be introspected, and this value never falsely reports fallback).

- [ ] T034 [US2] In `backend/crates/server/src/router.rs`, add two `Extension` layers **alongside the existing `.layer(Extension(storage.clone()))` on the merged router** (~lines 1046-1051), not inside `platform_routes` (research R3 ordering trap):

  ```rust
  .layer(Extension(state.health_checks.clone()))
  .layer(Extension(storage_backend))
  ```

### Health endpoint — US2

- [ ] T035 [US2] In `backend/crates/modules/platform/src/model.rs`, add the health DTOs:
  - `#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)] #[serde(rename_all = "snake_case")] pub enum HealthVerdict { Healthy, Degraded, Unhealthy }` — declare the variants in that order so the derived `Ord` makes `healthy < degraded < unhealthy` and "worst" is `max()`.
  - `#[serde(tag = "kind", rename_all = "snake_case")] pub enum DependencyDetail { Queue { pending_count: i64, oldest_pending_age_seconds: Option<i64>, stuck_retry_count: i64 }, Storage { backend: String, s3_configured: bool } }`
  - `DependencyHealthDto { name: String, verdict: HealthVerdict, reason: Option<String>, probe_ms: u64, detail: Option<DependencyDetail> }`
  - `SystemHealthDto { overall: HealthVerdict, dependencies: Vec<DependencyHealthDto>, read_at: DateTime<Utc> }`

- [ ] T036 [US2] In `backend/crates/modules/platform/src/queries.rs`, add:

  ```rust
  pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, Option<DateTime<Utc>>, i64)>
  // SELECT COUNT(*)::bigint,
  //        MIN(created_at),
  //        COUNT(*) FILTER (WHERE attempts > 5)::bigint
  // FROM outbox_events WHERE processed_at IS NULL
  ```

- [ ] T037 [US2] Create `backend/crates/modules/platform/src/health.rs` with pure, unit-testable functions (no I/O), and add `pub mod health;` to `backend/crates/modules/platform/src/lib.rs`:
  - `pub fn queue_verdict(pending: i64, oldest_age_seconds: Option<i64>, stuck: i64) -> (HealthVerdict, Option<String>)` — **unhealthy** when `pending >= 1000` OR `oldest > 1800s`; **degraded** when `pending >= 100` OR `oldest >= 300s` OR `stuck > 0`; otherwise **healthy** (FR-014, FR-015). Evaluate unhealthy first. The reason string names both the backlog count and the oldest-wait age, e.g. `"150 events waiting; oldest waiting 12m"`.
  - `pub fn storage_verdict(backend: StorageBackend, probe_ok: Result<(), String>) -> (HealthVerdict, Option<String>)` — **unhealthy** when `kind == InMemory && s3_configured` with reason `"configured for S3 but running on the in-process fallback backend"` (FR-016), or when `probe_ok` is `Err` with the error as reason; otherwise **healthy** (FR-017a: readable-but-unwritable is healthy).
  - `pub fn overall(verdicts: &[HealthVerdict]) -> HealthVerdict` — the max, defaulting to `Healthy` for an empty slice (FR-012).
  - Add `#[cfg(test)]` unit tests pinning the boundaries: pending 99 → healthy, 100 → degraded, 999 → degraded, 1000 → unhealthy; oldest 299s → healthy, 300s → degraded, 1800s → degraded, 1801s → unhealthy; `stuck = 1` with `pending = 0` → degraded.

- [ ] T038 [US2] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_health(State(pool): State<PgPool>, Extension(checks): Extension<Vec<Arc<dyn HealthCheck>>>, Extension(storage): Extension<Arc<dyn ObjectStorage>>, Extension(backend): Extension<StorageBackend>) -> Response`:
  - For each entry in `checks`, run `tokio::time::timeout(Duration::from_secs(5), check.check())`, measuring elapsed ms with `std::time::Instant`. `Ok(Ok(()))` → healthy; `Ok(Err(e))` → unhealthy with `e` as reason; `Err(_)` → unhealthy with reason `"timed out"` (FR-018). Use `check.name()` as the DTO `name` — the registered values are already exactly `"database"` and `"cache"`.
  - Queue: call `queries::queue_health`, convert `oldest_pending_at` to an age in seconds against `Utc::now()`, pass to `health::queue_verdict`, attach `DependencyDetail::Queue`. If the query errors, `tracing::error!` and emit the queue dependency as unhealthy with the error as reason — **do not** fail the whole response (FR-028).
  - Storage: run `storage.probe()` inside a 5 s timeout, pass the result to `health::storage_verdict`, attach `DependencyDetail::Storage { backend: "s3" | "in_memory", s3_configured }`.
  - Always return `200` regardless of verdict. Add the `#[utoipa::path]` attribute: `get`, `path = "/platform/health"`, `tag = "platform-health"`, `operation_id = "get_platform_health"`, no params, response 200 `body = model::SystemHealthDto`.

- [ ] T039 [US2] In `backend/crates/server/src/router.rs`, register the route inside `platform_routes(...)`:

  ```rust
  .routes(
      routes!(platform::routes::get_platform_health)
          .layer(require_permission(Permission::PlatformDiagnosticsView)),
  )
  ```

- [ ] T040 [US2] In `backend/crates/server/tests/openapi_coverage.rs` add `("GET", "/platform/health"),` to `EXPECTED` (unprefixed). In `backend/crates/server/tests/rbac.rs` add `"/api/v1/platform/health",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/health", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` (prefixed).

- [ ] T041 [US2] Create `backend/crates/server/tests/platform_health.rs` (same header block as `platform_analytics.rs`). Tests:
  1. Empty queue + reachable deps → `overall == "healthy"`, four dependencies present, each with a `probe_ms` and a `read_at` on the envelope.
  2. Insert ~150 unprocessed `outbox_events` with `created_at` 12 minutes ago → queue `degraded`, `detail.pending_count == 150`, `detail.oldest_pending_age_seconds` ≈ 720, `overall == "degraded"`.
  3. Insert ≥1000 unprocessed events → queue and `overall` both `unhealthy`.
  4. One unprocessed event with `attempts = 6` and a small backlog → queue at least `degraded` (FR-015).
  5. A platform user holding `platform.analytics.view` but **not** `platform.diagnostics.view` (Sales/Support/Finance) → `403` (FR-002).
  6. A tenant-role user → `403` (FR-003).
  7. Response JSON contains no `request_content`, `response_content`, or credential key (SC-008).

  > Deliberately **not** automated here: the silent-storage-fallback case (FR-016). T033 gives the test-injected storage path the descriptor `s3_configured: false`, which makes fallback unreproducible in-process by design. That scenario is covered manually by quickstart Scenario 4 step 5 under T070. Do not "fix" T033 to make this testable.

### Frontend — US2

- [ ] T042 [P] [US2] Create `frontend/apps/dashboard/src/app/shared/components/health-indicator/health-indicator.component.ts` — standalone, `OnPush`, selector `app-health-indicator`. Inputs: `verdict = input.required<'healthy' | 'degraded' | 'unhealthy'>()`. Renders a `tui-icon` **and** a text label together (`Healthy` / `Degraded` / `Unhealthy`) so the state is readable with colour removed (FR-027, SC-006). Colour comes from `--app-green` / `--app-amber` / `--app-red` tokens, following the token usage in `shared/components/status-badge/status-badge.component.ts`.

- [ ] T043 [P] [US2] Create `frontend/apps/dashboard/src/app/shared/components/health-indicator/health-indicator.component.spec.ts` — asserts the rendered `textContent` contains the label for each of the three verdicts (this is the colour-independence check).

- [ ] T044 [US2] In `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts`, add `SystemHealthWire` / `SystemHealth` types and `systemHealthFromWire`, mapping `probe_ms` → `probeMs`, `read_at` → `readAt`, `pending_count` → `pendingCount`, `oldest_pending_age_seconds` → `oldestPendingAgeSeconds`, `stuck_retry_count` → `stuckRetryCount`, `s3_configured` → `s3Configured`.

- [ ] T045 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health-api.service.ts` — `getHealth(): Observable<ApiResponse<SystemHealth>>` calling `this.api.get<SystemHealthWire>('/platform/health')` and mapping with `systemHealthFromWire`.

- [ ] T046 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.store.ts` — `signalStore` with state `{ health: SystemHealth | null; loading: boolean; error: string | null }`, a `load()` `rxMethod`, and `withHooks({ onInit })` that starts a 30-second `setInterval` calling `load()` and clears it in `onDestroy` (FR-019). Keep the previously loaded `health` on error so the page does not blank out.

- [ ] T047 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.ts` — `<app-page-container>` + `<app-page-header title="System health" />`, an overall `<app-health-indicator>`, a manual Refresh button calling `store.load()`, the `readAt` timestamp labelled UTC, and one `<app-dashboard-card>` per dependency showing name, `<app-health-indicator>`, `reason` when present, `probeMs`, and the queue/storage `detail` fields when present. Each card renders independently so one failing panel does not blank the page (FR-028).

- [ ] T048 [US2] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts` add `[APP_PATHS.platform.systemHealth]: 'platform.diagnostics.view',` to `PAGE_PERMISSIONS`. In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` add a route for `APP_PATHS.platform.systemHealth` with `canMatch: [permissionGuard]`, `data: { pageTitle: 'platformSystemHealth', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.systemHealth] }`, lazy-loading `SystemHealthComponent`. In `frontend/apps/dashboard/src/app/core/router/page-title.ts` add `'platformSystemHealth'` to the union and `platformSystemHealth: { title: 'System health', subtitle: 'Platform administration' },` to the map.

- [ ] T049 [US2] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, add a `PLATFORM_DESTINATIONS` entry with `label: 'System health'`, `permission: 'platform.diagnostics.view'`, and a backtick template-literal `path` of `/${APP_PATHS.platform.base}/${APP_PATHS.platform.systemHealth}` (same style as the `Tenants` entry), inserted **immediately before** the `'Platform overview'` entry (so the order becomes Tenants, Audit Logs, Billing, System health, Platform overview). Then update the index-based assertions in `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts` (~lines 94-98): `options[3]` now expects `'System health'` and `options[4]` expects `'Platform overview'`.

- [ ] T050 [P] [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.spec.ts` — renders four dependency cards from a stubbed store, shows the queue `pendingCount` / `oldestPendingAgeSeconds` text when the detail is present, and still renders the other cards when one dependency is unhealthy with a reason.

**Checkpoint**: US1 and US2 both work independently.

---

## Phase 5: User Story 3 - Platform engineers can track AI provider errors (Priority: P3)

**Goal**: `GET /platform/ai-providers` returns per-provider metrics, category breakdown, verdicts, and a capped recent-failures list; the AI Provider Status page renders them.

**Independent Test**: Seed AI call records with mixed success/failure categories across providers, confirm per-provider rates, category breakdown, verdicts, and the recent-failures list are correct for the selected window.

### Backend — US3

- [ ] T051 [US3] In `backend/crates/modules/platform/src/model.rs`, add the provider DTOs:
  - `pub const PROVIDERS: [&str; 3] = ["openai", "anthropic", "gemini"];`
  - `WindowQuery { window: Option<String> }`; `pub fn resolve_window(q: WindowQuery, now: DateTime<Utc>) -> Result<(DateTime<Utc>, DateTime<Utc>, String), String>` — accepts only `"1h"`, `"24h"`, `"7d"`, defaults to `"24h"`, any other value → `Err("Unknown window")` (FR-025).
  - `#[serde(rename_all = "snake_case")] pub enum ProviderVerdict { Healthy, Degraded, Unhealthy, InsufficientData, NoActivity }`
  - `FailuresByCategoryDto { authentication: i64, rate_limited: i64, unavailable: i64, timeout: i64, invalid_request: i64 }`
  - `ProviderMetricsDto { provider: String, verdict: ProviderVerdict, total_calls: i64, successes: i64, failures: i64, failure_rate: Option<f64>, p50_latency_ms: Option<i64>, p95_latency_ms: Option<i64>, input_tokens: i64, output_tokens: i64, failures_by_category: FailuresByCategoryDto }`
  - `ProviderFailureDto { provider: String, model: String, error_category: Option<String>, latency_ms: i64, occurred_at: DateTime<Utc> }` — no content or credential fields, ever (FR-026, SC-008).
  - `WindowDto { from: DateTime<Utc>, to: DateTime<Utc>, preset: String }`
  - `ProviderStatusDto { window: WindowDto, providers: Vec<ProviderMetricsDto>, recent_failures: Vec<ProviderFailureDto>, generated_at: DateTime<Utc> }`

- [ ] T052 [US3] In `backend/crates/modules/platform/src/health.rs`, add `pub fn provider_verdict(total_calls: i64, failures: i64) -> ProviderVerdict`, evaluated **in this order**: `total_calls == 0` → `NoActivity`; `total_calls < 20` → `InsufficientData`; `failures as f64 / total_calls as f64 >= 0.10` → `Unhealthy`; `>= 0.02` → `Degraded`; else `Healthy` (FR-022, FR-023). The ordering makes division by zero structurally unreachable. Add `#[cfg(test)]` tests pinning: 0 calls, 19 calls, 20 calls, 25/100 → unhealthy, 4/100 → degraded, 1/100 → healthy, exactly 2/100 → degraded, exactly 10/100 → unhealthy.

- [ ] T053 [US3] In `backend/crates/modules/platform/src/queries.rs`, add two functions:

  ```rust
  pub async fn provider_metrics(pool: &PgPool, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>)
      -> sqlx::Result<Vec<(String, i64, i64, i64, i64, i64, Option<i64>, Option<i64>, i64, i64, i64, i64, i64)>>
  // SELECT provider,
  //        COUNT(*)::bigint,
  //        COUNT(*) FILTER (WHERE status = 'success')::bigint,
  //        COUNT(*) FILTER (WHERE status = 'failure')::bigint,
  //        COALESCE(SUM(input_tokens), 0)::bigint,
  //        COALESCE(SUM(output_tokens), 0)::bigint,
  //        (percentile_cont(0.5)  WITHIN GROUP (ORDER BY latency_ms::float8))::bigint,
  //        (percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms::float8))::bigint,
  //        COUNT(*) FILTER (WHERE error_category = 'authentication')::bigint,
  //        COUNT(*) FILTER (WHERE error_category = 'rate_limited')::bigint,
  //        COUNT(*) FILTER (WHERE error_category = 'unavailable')::bigint,
  //        COUNT(*) FILTER (WHERE error_category = 'timeout')::bigint,
  //        COUNT(*) FILTER (WHERE error_category = 'invalid_request')::bigint
  // FROM ai_usage_records
  // WHERE created_at >= $1 AND created_at < $2
  // GROUP BY provider

  pub async fn recent_provider_failures(pool: &PgPool, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>)
      -> sqlx::Result<Vec<(String, String, Option<String>, i32, DateTime<Utc>)>>
  // SELECT provider, model, error_category, latency_ms, created_at
  // FROM ai_usage_records
  // WHERE status = 'failure' AND created_at >= $1 AND created_at < $2
  // ORDER BY created_at DESC
  // LIMIT 50
  ```

  Select no other columns — `request_content` and `response_content` must never be read (FR-026).

- [ ] T054 [US3] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_ai_provider_status(State(pool): State<PgPool>, Query(q): Query<model::WindowQuery>) -> Response`. Resolve the window (invalid → `422`), run both queries, then build the `providers` vector by iterating `model::PROVIDERS` and filling any provider absent from the query result with an all-zero row (FR-023 — all three are always present). Set `failure_rate` and both percentiles to `None` when `total_calls == 0` — never `0` (Edge Cases). Assign the verdict with `health::provider_verdict`. Add the `#[utoipa::path]` attribute: `get`, `path = "/platform/ai-providers"`, `tag = "platform-health"`, `operation_id = "get_platform_ai_provider_status"`, param `window` as `Option<String>`, responses 200 (`body = model::ProviderStatusDto`) and 422.

- [ ] T055 [US3] In `backend/crates/server/src/router.rs`, register the route inside `platform_routes(...)` with `.layer(require_permission(Permission::PlatformDiagnosticsView))`.

- [ ] T056 [US3] In `backend/crates/server/tests/openapi_coverage.rs` add `("GET", "/platform/ai-providers"),` to `EXPECTED` (unprefixed). In `backend/crates/server/tests/rbac.rs` add `"/api/v1/platform/ai-providers",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/ai-providers", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` (prefixed).

- [ ] T057 [US3] Create `backend/crates/server/tests/platform_ai_providers.rs` (same header block as `platform_analytics.rs`). Tests:
  1. 100 calls for `openai` of which 25 failed → `failure_rate == 0.25`, verdict `"unhealthy"`.
  2. 100 calls of which 4 failed → `0.04`, verdict `"degraded"`.
  3. Failures spread across categories → the five `failures_by_category` counts sum to that provider's `failures`.
  4. A provider with 3 calls → `"insufficient_data"`; a provider with 0 calls → `"no_activity"`, still present in the array, with `failure_rate: null` and both percentiles `null`.
  5. `recent_failures` is newest-first and capped at 50 when 60 failures exist.
  6. `?window=90d` → `422`; `?window=1h`/`24h`/`7d` all → `200`.
  7. The raw response body contains none of `request_content`, `response_content`, or any credential key (SC-008).

### Frontend — US3

- [ ] T058 [US3] In `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts`, add `ProviderStatusWire` / `ProviderStatus` types and `providerStatusFromWire`, mapping `total_calls` → `totalCalls`, `failure_rate` → `failureRate`, `p50_latency_ms` → `p50LatencyMs`, `p95_latency_ms` → `p95LatencyMs`, `failures_by_category` → `failuresByCategory` (keeping the five category keys), `recent_failures` → `recentFailures`, `error_category` → `errorCategory`, `latency_ms` → `latencyMs`, `occurred_at` → `occurredAt`.

- [ ] T059 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers-api.service.ts` — `getStatus(window: '1h' | '24h' | '7d'): Observable<ApiResponse<ProviderStatus>>` calling `this.api.get<ProviderStatusWire>('/platform/ai-providers', params)` with the `window` query param.

- [ ] T060 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.store.ts` — `signalStore` with state `{ status: ProviderStatus | null; window: '1h' | '24h' | '7d'; loading: boolean; error: string | null }`, a `load()` `rxMethod`, and `setWindow(w)` that patches `window` and re-loads.

- [ ] T061 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.ts` — `<app-page-container>` + `<app-page-header title="AI provider status" />`, an `<app-select-filter>` for the window (`1h` / `24h` / `7d`), one `<app-dashboard-card>` per provider showing `<app-health-indicator>` (mapping `insufficient_data` and `no_activity` to a neutral `<app-status-badge>` with those labels rather than a health colour), call counts, failure rate as a percentage (render `—` when `failureRate` is `null`), p50/p95 latency, tokens, and an `<app-breakdown-bars>` of the five failure categories. Below that, an `<app-data-table>` of `recentFailures` with columns provider, model, category, latency, UTC timestamp. Use `<app-empty-state>` when there is no activity at all.

- [ ] T062 [US3] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts` add `[APP_PATHS.platform.aiProviders]: 'platform.diagnostics.view',` to `PAGE_PERMISSIONS`. In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` add the `APP_PATHS.platform.aiProviders` route with `canMatch: [permissionGuard]` and `data: { pageTitle: 'platformAiProviders', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.aiProviders] }`. In `frontend/apps/dashboard/src/app/core/router/page-title.ts` add `'platformAiProviders'` to the union and `platformAiProviders: { title: 'AI provider status', subtitle: 'Platform administration' },` to the map.

- [ ] T063 [US3] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, add a `PLATFORM_DESTINATIONS` entry with `label: 'AI providers'`, `permission: 'platform.diagnostics.view'`, and a backtick template-literal `path` of `/${APP_PATHS.platform.base}/${APP_PATHS.platform.aiProviders}`, inserted **immediately after** the `'System health'` entry. Then update `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts`: `options[4]` now expects `'AI providers'` and `options[5]` expects `'Platform overview'`.

- [ ] T064 [P] [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.spec.ts` — renders all three providers from a stubbed store including one with `no_activity` and `failureRate: null` (asserting `—` is shown, not `0%`), renders the category breakdown, and renders the recent-failures rows.

- [ ] T065 [P] [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.store.spec.ts` — asserts `setWindow('7d')` patches state and triggers a reload, and that an API error sets `error` with `loading: false`.

**Checkpoint**: All three user stories work independently.

---

## Phase 6: Polish & Validation

- [ ] T066 [P] In `backend/crates/server/src/openapi.rs`, add two entries to the `tags` vec inside `TagDescriptionsAddon::modify` (~line 241): `("platform-analytics", "Platform-internal cross-tenant business metrics.")` and `("platform-health", "Platform-internal system health and AI provider diagnostics.")`. This list is descriptive only — no test enforces it — but every other production tag is listed.

- [ ] T067 [P] Finalize the module docs in `backend/crates/modules/platform/src/lib.rs` — fill in the Purpose / Responsibilities / Public Interfaces (the three endpoint paths) / Dependencies / Data Model (reads `tenants`, `conversations`, `ai_usage_records`, `outbox_events`; owns no tables; migration `0060` adds four indexes) / Extension Points sections.

- [ ] T068 Run the backend gate exactly as written — **not** `cargo test --workspace`, which aborts on the first failing crate:

  ```bash
  cd backend
  REQUIRE_DB_TESTS=1 DATABASE_URL=postgres://… cargo test -p authz -p platform -p server
  ```

  Without `REQUIRE_DB_TESTS=1` the DB-gated tests skip and pass vacuously.

- [ ] T069 Run the frontend gate: `cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check`.

- [ ] T070 Run `cd backend && cargo fmt` on **only the files this feature touched**. `cargo fmt --check` is red repo-wide at HEAD for unrelated reasons — do not treat that as a regression from this feature.

- [ ] T071 Walk `specs/032-platform-analytics-health/quickstart.md` Scenarios 1-6 manually and record the result. Scenario 2 step 3 (direct API calls returning `403` for a tenant user) and Scenario 5 step 6 (raw-JSON leak check) are the ones that actually prove SC-002 and SC-008.

- [ ] T072 Performance spot-check (SC-003): with ~1,000 tenants and ~1,000,000 `ai_usage_records` rows seeded, run `EXPLAIN (ANALYZE, BUFFERS)` on the provider-metrics query and confirm it uses `ai_usage_records_created_idx` rather than a sequential scan, and that each page's first figures appear within 2 s. If the budget is missed, **escalate** per research R4 (approximate percentiles or a narrower maximum window) — do **not** add a cache; caching was explicitly rejected in clarification Q2.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001-T003)**: no dependencies.
- **Foundational (T004-T011)**: depends on Setup. **Blocks all user stories** — `Permission::PlatformAnalyticsView` must exist before any route registration compiles.
- **US1 (T012-T028)**: depends on Foundational only.
- **US2 (T029-T050)**: depends on Foundational only. Independent of US1.
- **US3 (T051-T065)**: depends on Foundational only. Independent of US1 and US2.
- **Polish (T066-T072)**: depends on whichever stories were built.

### Cross-story file contention (do not run these in parallel across stories)

`platform/src/model.rs`, `platform/src/queries.rs`, `platform/src/routes.rs`, `server/src/router.rs`, `server/tests/rbac.rs`, `server/tests/openapi_coverage.rs`, `frontend/core/api/platform-analytics.models.ts`, `frontend/core/authz/permissions.ts`, `frontend/features/platform/platform.routes.ts`, `frontend/layout/topbar/platform-nav.component.ts` are each touched by more than one story. If stories are built by different people, serialize edits to these ten files.

### Within a story

Model → queries → handler → route registration → inventory files → integration tests → frontend models → api service → store → component → route/nav wiring → specs.

### Parallel Opportunities

- Foundational: T009 and T010 are frontend-only and run in parallel with the Rust tasks T004-T008.
- US1: T019 runs alongside the backend tasks; T027 and T028 run together once the store and component exist.
- US2: T042 and T043 (the `health-indicator` component and its spec) are independent of everything else in the story and can start immediately after Foundational.
- US3: T064 and T065 run together.

---

## Parallel Example: Foundational Phase

```bash
# Rust inventory edits (sequential — same crate):
Task: "T005 permission.rs variant + ALL"
Task: "T006 permission.rs catalog test array"
Task: "T007 matrix.rs five platform arrays"

# Simultaneously, frontend (different files, no overlap):
Task: "T009 permissions.ts Permission union"
Task: "T010 app-paths.ts systemHealth + aiProviders"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 Setup (T001-T003)
2. Phase 2 Foundational (T004-T011) — do not skip; everything else fails to compile without it
3. Phase 3 US1 (T012-T028)
4. **STOP and VALIDATE**: quickstart Scenarios 1 and 2
5. Ship — the Platform Overview placeholder is now a real dashboard

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. + US1 → validate Scenarios 1-2 → ship (MVP)
3. + US2 → validate Scenarios 3-4 → ship
4. + US3 → validate Scenarios 5-6 → ship
5. Polish (T066-T072)

---

## Notes

- Every platform handler takes **no** `TenantContext` and every query omits `tenant_id` — that is deliberate (FR-005), not an isolation bug.
- All three endpoints return the bare DTO via `Json(dto)`. Do not add a `{"data": ...}` wrapper; the frontend `ApiService` adds its own `data` field client-side.
- Adding a permission or a route reddens exact-inventory suites by design. The tasks that update them (T006, T007, T008, T016, T017, T040, T056, T026, T049, T063) are mandatory, not cleanup.
- Commit after each task or logical group.
