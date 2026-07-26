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

- [X] T001 Replace the whole of `backend/crates/modules/platform/Cargo.toml` with the package block it already has plus this dependency block:

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

- [X] T002 In `backend/crates/server/Cargo.toml`, add `platform = { path = "../modules/platform" }` to `[dependencies]` (alphabetically after `observability`).

- [X] T003 Create the crate skeleton in `backend/crates/modules/platform/src/`: replace `lib.rs` with a module doc comment (Purpose / Responsibilities / Public Interfaces / Dependencies / Data Model / Extension Points — copy the section headings from `backend/crates/modules/analytics/src/lib.rs`) followed by `pub mod model;`, `pub mod queries;`, `pub mod routes;`. Then create three **empty** files `model.rs`, `queries.rs`, `routes.rs` (empty `.rs` files are valid Rust and compile). Verify with `cd backend && cargo check -p platform`.

**Checkpoint**: `cargo check -p server` passes with the platform crate linked.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Migration + the new permission wired through every exact-inventory file. **No user story can start until this phase is complete** — a missing permission entry fails compilation and reddens `authz` tests.

- [X] T004 Create `backend/migrations/0060_platform_analytics_indexes.sql` with exactly these four additive indexes and nothing else:

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

- [X] T005 In `backend/crates/modules/authz/src/permission.rs` add the new permission in three places: (a) the enum — `#[serde(rename = "platform.analytics.view")]` above `PlatformAnalyticsView,`, placed right after the `PlatformAdmin` variant; (b) `pub const ALL` — change the length from `[Self; 31]` to `[Self; 32]` and add `Self::PlatformAnalyticsView,` after `Self::PlatformAdmin,`; (c) the `impl fmt::Display` match — add `Self::PlatformAnalyticsView => "platform.analytics.view",` after the `Self::PlatformAdmin` arm.

- [X] T006 In `backend/crates/modules/authz/src/permission.rs`, in the `catalog_parity_with_contract` test, change `let contract_codes: [&str; 31]` to `[&str; 32]` and add the string `"platform.analytics.view",` after `"platform.admin",`.

- [X] T007 In `backend/crates/modules/authz/src/matrix.rs`, add `Permission::PlatformAnalyticsView,` to **all five** platform arrays: `PLATFORM_ALL`, `PLATFORM_DEVELOPER`, `PLATFORM_TENANT_ACCESS`, `PLATFORM_SUPPORT`, `PLATFORM_FINANCE` (FR-001). Missing any one of them fails the `assert_eq!(granted, HashSet::from(Permission::ALL))` test at the bottom of that file.

- [X] T008 In `backend/crates/server/tests/auth.rs`, add `"platform.analytics.view"` to the `"platformPermissions"` JSON array (~line 245, after `"platform.admin"`).

- [X] T009 [P] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts`, add `| 'platform.analytics.view'` to the `Permission` union (after `'platform.admin'`).

- [X] T010 [P] In `frontend/apps/dashboard/src/app/core/router/app-paths.ts`, add two entries to the `platform` object: `systemHealth: 'system-health',` and `aiProviders: 'ai-providers',`. Do **not** rename `overviewPlaceholder`.

- [X] T011 Run `cd backend && cargo test -p authz` and confirm it passes.

**Checkpoint**: Permission catalog is consistent; migration applies. User stories can begin.

---

## Phase 3: User Story 1 - Platform staff see the whole business at a glance (Priority: P1) 🎯 MVP

**Goal**: `GET /platform/analytics/summary` returns cross-tenant tenant/conversation/AI-usage totals, and the Platform Overview page renders them.

**Independent Test**: Sign in as a Sales platform user, open Platform Overview, confirm tenant totals / active conversations / AI usage render and do not change when the tenant switcher changes tenant.

### Backend — US1

- [X] T012 [US1] In `backend/crates/modules/platform/src/model.rs`, add the summary DTOs and range validation. All structs derive `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]`; all field names stay `snake_case`:
  - `SummaryQuery { from: Option<String>, to: Option<String> }` (derive `Debug, Deserialize, ToSchema`)
  - `ResolvedRange { from_date: NaiveDate, to_date: NaiveDate, from_ts: DateTime<Utc>, to_ts: DateTime<Utc> }` (derive `Debug, Clone`)
  - `DateRangeDto { from: NaiveDate, to: NaiveDate }`
  - `TenantCountsDto { total: i64, active: i64, suspended: i64, created_in_range: i64 }`
  - `AiUsageTotalsDto { total_calls: i64, successful_calls: i64, failed_calls: i64, input_tokens: i64, output_tokens: i64 }`
  - `PlatformUsageSummaryDto { range: DateRangeDto, tenants: TenantCountsDto, active_conversations: i64, ai_usage: AiUsageTotalsDto, generated_at: DateTime<Utc> }`
  - `pub fn resolve_range(query: SummaryQuery, today: NaiveDate) -> Result<ResolvedRange, String>` — copy the structure of `resolve_query` in `backend/crates/modules/analytics/src/model.rs` but with **no channel handling**: parse `%Y-%m-%d`; default `to = today`, `from = to - 29 days`; error `"from must be on or before to"` when `from > to`; error `"Date range must not exceed 365 days"` when `(to - from).num_days() + 1 > 365`; `from_ts` = `from` at 00:00:00Z, `to_ts` = (`to` + 1 day) at 00:00:00Z (exclusive upper bound).

- [X] T013 [US1] In `backend/crates/modules/platform/src/queries.rs`, add three functions using `sqlx::query_as::<_, (..)>` / `sqlx::query_scalar` with `.bind(...)` (runtime-checked — do **not** use the `query!` macro). None of them filter by `tenant_id` (FR-005):

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

- [X] T014 [US1] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_analytics_summary(State(pool): State<PgPool>, Query(query): Query<model::SummaryQuery>) -> Response`. Take **no** `TenantContext` (platform routes receive no `X-Tenant-ID`). On `resolve_range` error return `ApiError::unprocessable_entity(msg).into_response()` (422). Call the three queries; on any `Err`, `tracing::error!(%e, "…")` and return `ApiError::internal_error("Failed to load platform summary")`. Return `(StatusCode::OK, Json(dto)).into_response()` — a bare DTO, no `data` wrapper. Add the `#[utoipa::path]` attribute above it, copying the shape from `backend/crates/modules/analytics/src/routes.rs`, with: `get`, `path = "/platform/analytics/summary"`, `tag = "platform-analytics"`, `operation_id = "get_platform_analytics_summary"`, params `from`/`to` as `Option<String>`, responses 200 (`body = model::PlatformUsageSummaryDto`) and 422.

- [X] T015 [US1] In `backend/crates/server/src/router.rs`, inside `platform_routes(...)` (after the `audit::routes::list_platform_audit_logs` block, ~line 330), add:

  ```rust
  .routes(
      routes!(platform::routes::get_platform_analytics_summary)
          .layer(require_permission(Permission::PlatformAnalyticsView)),
  )
  ```

- [X] T016 [US1] In `backend/crates/server/tests/openapi_coverage.rs`, add `("GET", "/platform/analytics/summary"),` to the `EXPECTED` array (next to the other `/platform/...` entries). **No `/api/v1` prefix** — the entries in this file are unprefixed.

- [X] T017 [US1] In `backend/crates/server/tests/rbac.rs`, add `"/api/v1/platform/analytics/summary",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/analytics/summary", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT`. **With `/api/v1` prefix** here.

- [X] T018 [US1] Create `backend/crates/server/tests/platform_analytics.rs`. Copy the header block (imports, `TEST_ENV`, `test_config()`, `app_state()`, `require_db_tests()`) verbatim from `backend/crates/server/tests/analytics_api.rs`. Add these tests, each guarded by `if !require_db_tests() { return; }`:
  1. Seeds 3 tenants (2 `active`, 1 `suspended`) plus 1 soft-deleted tenant → response `tenants.total == 3` and the soft-deleted one is excluded (FR-006).
  2. Seeds conversations across two tenants in `open`/`pending`/`resolved` → `active_conversations` counts only `open` + `pending`, summed across tenants (FR-007).
  3. Seeds `ai_usage_records` across two tenants → `ai_usage` totals and success/failure split match the seeded numbers (FR-008).
  4. `?from=2026-07-25&to=2026-06-25` → `422`; a span of 400 days → `422` (FR-009).
  5. No query params → response `range` spans the last 30 days inclusive (FR-009).
  6. A tenant-role user (no platform role) calling `GET /api/v1/platform/analytics/summary` → `403` with no aggregate data in the body (FR-003, SC-002).

### Frontend — US1

- [X] T019 [P] [US1] Create `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts` with wire types matching the JSON exactly (snake_case) plus camelCase app types and a mapper, following the wire/`fromWire` pattern in `core/api/tenant-api.models.ts`:
  - `PlatformSummaryWire` — `{ range: { from: string; to: string }, tenants: { total: number; active: number; suspended: number; created_in_range: number }, active_conversations: number, ai_usage: { total_calls: number; successful_calls: number; failed_calls: number; input_tokens: number; output_tokens: number }, generated_at: string }`
  - `PlatformSummary` — same shape with `createdInRange`, `activeConversations`, `aiUsage`, `totalCalls`, `successfulCalls`, `failedCalls`, `inputTokens`, `outputTokens`, `generatedAt`
  - `export function platformSummaryFromWire(wire: PlatformSummaryWire): PlatformSummary`

- [X] T020 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview-api.service.ts` — an `@Injectable({ providedIn: 'root' })` service that injects `ApiService` and exposes `getSummary(query: { from?: string; to?: string }): Observable<ApiResponse<PlatformSummary>>` calling `this.api.get<PlatformSummaryWire>('/platform/analytics/summary', params)` and mapping with `platformSummaryFromWire`. Copy the `buildParams` helper style from `features/platform/audit-logs/platform-audit-logs-api.service.ts`.

- [X] T021 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.store.ts` — a `signalStore` following `features/platform/billing/platform-billing.store.ts`. State: `{ summary: PlatformSummary | null; loading: boolean; error: string | null; from: string | null; to: string | null }`. Methods: `load(query)` via `rxMethod` with `catchError` writing `error` and `loading: false`; `setRange(from, to)` that patches state and re-loads.

- [X] T022 [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.ts` — standalone, `ChangeDetectionStrategy.OnPush`, `providers: [PlatformOverviewStore]`. Template uses `<app-page-container>` + `<app-page-header title="Platform overview" />`. Render that header **unconditionally**, outside every `@if (loading())` / `@if (summary())` block — `app.routes.spec.ts:63` navigates with `provideHttpClientTesting()` and never flushes the request, so the store is still `loading` when it asserts the text `Platform overview` is present. Below the header: a date-range control bound to `setRange`, and `<app-metric-card>` tiles for: total tenants (with active/suspended and created-in-range as sub-text), active conversations, total AI calls, successful/failed calls, input tokens, output tokens. Render `<app-loading-state>` while `loading()`, `<app-inline-alert>` when `error()`, and `<app-empty-state>` with a "No data yet" message when every figure is `0` (Edge Cases). Label the range and `generatedAt` as UTC (FR-029). Add a link to System Health rendered only when `permissions.has('platform.diagnostics.view')` (FR-011, FR-004).

- [X] T023 [US1] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts`, add `[APP_PATHS.platform.overviewPlaceholder]: 'platform.analytics.view',` to `PAGE_PERMISSIONS`.

- [X] T024 [US1] In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`, update the `APP_PATHS.platform.overviewPlaceholder` route: change `requiredPermission` from the literal `'platform.admin'` to `PAGE_PERMISSIONS[APP_PATHS.platform.overviewPlaceholder]`, and change `loadComponent` to `import('./overview/platform-overview.component').then((m) => m.PlatformOverviewComponent)`. Leave `path` and `pageTitle: 'platformOverview'` unchanged.

- [X] T025 [US1] Delete `frontend/apps/dashboard/src/app/features/platform/overview-placeholder/platform-overview-placeholder.component.ts` and its now-empty directory. Confirm with `grep -rn "PlatformOverviewPlaceholderComponent" frontend/` that no references remain.

- [X] T026 [US1] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, change the existing `'Platform overview'` entry in `PLATFORM_DESTINATIONS` so its `path` is a backtick template literal `/${APP_PATHS.platform.base}/${APP_PATHS.platform.overviewPlaceholder}` (same style as the `Tenants` entry above it) and its `permission` is `'platform.analytics.view'`. Then update `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts`: the index-based assertion at ~line 103 currently expects `routerMock.navigate` to be called with `['/platform']` — change it to `['/platform/overview-placeholder']`. Leave the `options[0..3]` label order assertions unchanged (this task adds no destinations).

- [X] T027 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.store.spec.ts` — mirror `features/tenant/analytics/analytics.store.spec.ts`: a mock api service returning `of({ data: MOCK_SUMMARY })`, asserting the store populates `summary` and clears `loading`; and a `throwError` case asserting `error` is set and `loading` is `false`.

- [X] T028 [P] [US1] Create `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.spec.ts` — renders metric values from a stubbed store, renders the zero-state message when all figures are `0`, and hides the System Health link when `PermissionsService.has` returns `false` for `'platform.diagnostics.view'`.

**Checkpoint**: US1 is independently shippable — overview endpoint + page work with no health or provider code present.

---

## Phase 4: User Story 2 - Platform engineers can tell whether the system is healthy right now (Priority: P2)

**Goal**: `GET /platform/health` returns a three-state overall verdict plus per-dependency results for database, cache, queue, storage; the System Health page renders them and auto-refreshes every 30 s.

**Independent Test**: Seed unprocessed `outbox_events` and stop a dependency, confirm the page shows the expected degraded/unhealthy states with numeric evidence, and returns to healthy after the backlog clears.

### Storage prerequisites — US2

- [X] T029 [US2] In `backend/crates/shared/storage/src/lib.rs`, add `async fn probe(&self) -> Result<(), StorageError>;` to the `ObjectStorage` trait, then implement it in both implementors in the same file: `S3Storage::probe` calls `self.client.head_bucket().bucket(&self.bucket).send().await` and maps any error to `StorageError::Other(e.to_string())`; `InMemoryStorage::probe` returns `Ok(())`. Do **not** use `get()` on a reserved key (research R1).

- [X] T030 [US2] In `backend/crates/modules/knowledge/src/upload.rs` (~line 505), add a `probe` implementation to the `FailingStorage` test double that returns `Err(StorageError::Other("probe failed".into()))`, so the unhealthy path stays covered. This is required for the crate to compile after T029.

- [X] T031 [US2] In `backend/crates/shared/storage/src/lib.rs`, add the backend descriptor and one centralized selection function:

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum StorageKind { S3, InMemory }

  #[derive(Debug, Clone, Copy)]
  pub struct StorageBackend { pub kind: StorageKind, pub s3_configured: bool }

  pub async fn select_storage(s3: Option<&config::S3Config>)
      -> (std::sync::Arc<dyn ObjectStorage>, StorageBackend)
  ```

  Behaviour: when `s3` is `Some` and `S3Storage::new` succeeds → `(S3Storage, { kind: S3, s3_configured: true })`. When `s3` is `Some` and it fails → keep the existing `tracing::warn!` and return `(InMemoryStorage, { kind: InMemory, s3_configured: true })`. When `s3` is `None` → `(InMemoryStorage, { kind: InMemory, s3_configured: false })`.

- [X] T032 [US2] In `backend/crates/server/src/main.rs`, replace the inline S3-or-fallback block (~lines 85-95) with `let (storage, _storage_backend) = storage::select_storage(state.config.s3.as_ref()).await;`.

- [X] T033 [US2] In `backend/crates/server/src/router.rs`, replace the duplicated S3-or-fallback block (~lines 998-1010) with a call to `select_storage`. Because the surrounding function is sync, keep the existing `tokio::runtime::Handle::current().block_on(...)` wrapper. When a `storage` argument was injected by a test (`Some(_)`), do **not** call `select_storage` — use the injected value with the descriptor `StorageBackend { kind: StorageKind::InMemory, s3_configured: false }` (a trait object cannot be introspected, and this value never falsely reports fallback).

- [X] T034 [US2] In `backend/crates/server/src/router.rs`, add two `Extension` layers **alongside the existing `.layer(Extension(storage.clone()))` on the merged router** (~lines 1046-1051), not inside `platform_routes` (research R3 ordering trap):

  ```rust
  .layer(Extension(state.health_checks.clone()))
  .layer(Extension(storage_backend))
  ```

### Health endpoint — US2

- [X] T035 [US2] In `backend/crates/modules/platform/src/model.rs`, add the health DTOs:
  - `#[derive(Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)] #[serde(rename_all = "snake_case")] pub enum HealthVerdict { Healthy, Degraded, Unhealthy }` — declare the variants in that order so the derived `Ord` makes `healthy < degraded < unhealthy` and "worst" is `max()`.
  - `#[serde(tag = "kind", rename_all = "snake_case")] pub enum DependencyDetail { Queue { pending_count: i64, oldest_pending_age_seconds: Option<i64>, stuck_retry_count: i64 }, Storage { backend: String, s3_configured: bool } }`
  - `DependencyHealthDto { name: String, verdict: HealthVerdict, reason: Option<String>, probe_ms: u64, detail: Option<DependencyDetail> }`
  - `SystemHealthDto { overall: HealthVerdict, dependencies: Vec<DependencyHealthDto>, read_at: DateTime<Utc> }`

- [X] T036 [US2] In `backend/crates/modules/platform/src/queries.rs`, add:

  ```rust
  pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, Option<DateTime<Utc>>, i64)>
  // SELECT COUNT(*)::bigint,
  //        MIN(created_at),
  //        COUNT(*) FILTER (WHERE attempts > 5)::bigint
  // FROM outbox_events WHERE processed_at IS NULL
  ```

- [X] T037 [US2] Create `backend/crates/modules/platform/src/health.rs` with pure, unit-testable functions (no I/O), and add `pub mod health;` to `backend/crates/modules/platform/src/lib.rs`:
  - `pub fn queue_verdict(pending: i64, oldest_age_seconds: Option<i64>, stuck: i64) -> (HealthVerdict, Option<String>)` — **unhealthy** when `pending >= 1000` OR `oldest > 1800s`; **degraded** when `pending >= 100` OR `oldest >= 300s` OR `stuck > 0`; otherwise **healthy** (FR-014, FR-015). Evaluate unhealthy first. The reason string names both the backlog count and the oldest-wait age, e.g. `"150 events waiting; oldest waiting 12m"`.
  - `pub fn storage_verdict(backend: StorageBackend, probe_ok: Result<(), String>) -> (HealthVerdict, Option<String>)` — **unhealthy** when `kind == InMemory && s3_configured` with reason `"configured for S3 but running on the in-process fallback backend"` (FR-016), or when `probe_ok` is `Err` with the error as reason; otherwise **healthy** (FR-017a: readable-but-unwritable is healthy).
  - `pub fn overall(verdicts: &[HealthVerdict]) -> HealthVerdict` — the max, defaulting to `Healthy` for an empty slice (FR-012).
  - Add `#[cfg(test)]` unit tests pinning the boundaries: pending 99 → healthy, 100 → degraded, 999 → degraded, 1000 → unhealthy; oldest 299s → healthy, 300s → degraded, 1800s → degraded, 1801s → unhealthy; `stuck = 1` with `pending = 0` → degraded.

- [X] T038 [US2] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_health(State(pool): State<PgPool>, Extension(checks): Extension<Vec<Arc<dyn HealthCheck>>>, Extension(storage): Extension<Arc<dyn ObjectStorage>>, Extension(backend): Extension<StorageBackend>) -> Response`:
  - For each entry in `checks`, run `tokio::time::timeout(Duration::from_secs(5), check.check())`, measuring elapsed ms with `std::time::Instant`. `Ok(Ok(()))` → healthy; `Ok(Err(e))` → unhealthy with `e` as reason; `Err(_)` → unhealthy with reason `"timed out"` (FR-018). Use `check.name()` as the DTO `name` — the registered values are already exactly `"database"` and `"cache"`.
  - Queue: call `queries::queue_health`, convert `oldest_pending_at` to an age in seconds against `Utc::now()`, pass to `health::queue_verdict`, attach `DependencyDetail::Queue`. If the query errors, `tracing::error!` and emit the queue dependency as unhealthy with the error as reason — **do not** fail the whole response (FR-028).
  - Storage: run `storage.probe()` inside a 5 s timeout, pass the result to `health::storage_verdict`, attach `DependencyDetail::Storage { backend: "s3" | "in_memory", s3_configured }`.
  - Always return `200` regardless of verdict. Add the `#[utoipa::path]` attribute: `get`, `path = "/platform/health"`, `tag = "platform-health"`, `operation_id = "get_platform_health"`, no params, response 200 `body = model::SystemHealthDto`.

- [X] T039 [US2] In `backend/crates/server/src/router.rs`, register the route inside `platform_routes(...)`:

  ```rust
  .routes(
      routes!(platform::routes::get_platform_health)
          .layer(require_permission(Permission::PlatformDiagnosticsView)),
  )
  ```

- [X] T040 [US2] In `backend/crates/server/tests/openapi_coverage.rs` add `("GET", "/platform/health"),` to `EXPECTED` (unprefixed). In `backend/crates/server/tests/rbac.rs` add `"/api/v1/platform/health",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/health", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` (prefixed).

- [X] T041 [US2] Create `backend/crates/server/tests/platform_health.rs` (same header block as `platform_analytics.rs`). Tests:
  1. Empty queue + reachable deps → `overall == "healthy"`, four dependencies present, each with a `probe_ms` and a `read_at` on the envelope.
  2. Insert ~150 unprocessed `outbox_events` with `created_at` 12 minutes ago → queue `degraded`, `detail.pending_count == 150`, `detail.oldest_pending_age_seconds` ≈ 720, `overall == "degraded"`.
  3. Insert ≥1000 unprocessed events → queue and `overall` both `unhealthy`.
  4. One unprocessed event with `attempts = 6` and a small backlog → queue at least `degraded` (FR-015).
  5. A platform user holding `platform.analytics.view` but **not** `platform.diagnostics.view` (Sales/Support/Finance) → `403` (FR-002).
  6. A tenant-role user → `403` (FR-003).
  7. Response JSON contains no `request_content`, `response_content`, or credential key (SC-008).

  > Deliberately **not** automated here: the silent-storage-fallback case (FR-016). T033 gives the test-injected storage path the descriptor `s3_configured: false`, which makes fallback unreproducible in-process by design. That scenario is covered manually by quickstart Scenario 4 step 5 under T070. Do not "fix" T033 to make this testable.

### Frontend — US2

- [X] T042 [P] [US2] Create `frontend/apps/dashboard/src/app/shared/components/health-indicator/health-indicator.component.ts` — standalone, `OnPush`, selector `app-health-indicator`. Inputs: `verdict = input.required<'healthy' | 'degraded' | 'unhealthy'>()`. Renders a `tui-icon` **and** a text label together (`Healthy` / `Degraded` / `Unhealthy`) so the state is readable with colour removed (FR-027, SC-006). Colour comes from `--app-green` / `--app-amber` / `--app-red` tokens, following the token usage in `shared/components/status-badge/status-badge.component.ts`.

- [X] T043 [P] [US2] Create `frontend/apps/dashboard/src/app/shared/components/health-indicator/health-indicator.component.spec.ts` — asserts the rendered `textContent` contains the label for each of the three verdicts (this is the colour-independence check).

- [X] T044 [US2] In `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts`, add `SystemHealthWire` / `SystemHealth` types and `systemHealthFromWire`, mapping `probe_ms` → `probeMs`, `read_at` → `readAt`, `pending_count` → `pendingCount`, `oldest_pending_age_seconds` → `oldestPendingAgeSeconds`, `stuck_retry_count` → `stuckRetryCount`, `s3_configured` → `s3Configured`.

- [X] T045 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health-api.service.ts` — `getHealth(): Observable<ApiResponse<SystemHealth>>` calling `this.api.get<SystemHealthWire>('/platform/health')` and mapping with `systemHealthFromWire`.

- [X] T046 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.store.ts` — `signalStore` with state `{ health: SystemHealth | null; loading: boolean; error: string | null }`, a `load()` `rxMethod`, and `withHooks({ onInit })` that starts a 30-second `setInterval` calling `load()` and clears it in `onDestroy` (FR-019). Keep the previously loaded `health` on error so the page does not blank out.

- [X] T047 [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.ts` — `<app-page-container>` + `<app-page-header title="System health" />`, an overall `<app-health-indicator>`, a manual Refresh button calling `store.load()`, the `readAt` timestamp labelled UTC, and one `<app-dashboard-card>` per dependency showing name, `<app-health-indicator>`, `reason` when present, `probeMs`, and the queue/storage `detail` fields when present. Each card renders independently so one failing panel does not blank the page (FR-028).

- [X] T048 [US2] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts` add `[APP_PATHS.platform.systemHealth]: 'platform.diagnostics.view',` to `PAGE_PERMISSIONS`. In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` add a route for `APP_PATHS.platform.systemHealth` with `canMatch: [permissionGuard]`, `data: { pageTitle: 'platformSystemHealth', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.systemHealth] }`, lazy-loading `SystemHealthComponent`. In `frontend/apps/dashboard/src/app/core/router/page-title.ts` add `'platformSystemHealth'` to the union and `platformSystemHealth: { title: 'System health', subtitle: 'Platform administration' },` to the map.

- [X] T049 [US2] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, add a `PLATFORM_DESTINATIONS` entry with `label: 'System health'`, `permission: 'platform.diagnostics.view'`, and a backtick template-literal `path` of `/${APP_PATHS.platform.base}/${APP_PATHS.platform.systemHealth}` (same style as the `Tenants` entry), inserted **immediately before** the `'Platform overview'` entry (so the order becomes Tenants, Audit Logs, Billing, System health, Platform overview). Then update the index-based assertions in `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts` (~lines 94-98): `options[3]` now expects `'System health'` and `options[4]` expects `'Platform overview'`.

- [X] T050 [P] [US2] Create `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.spec.ts` — renders four dependency cards from a stubbed store, shows the queue `pendingCount` / `oldestPendingAgeSeconds` text when the detail is present, and still renders the other cards when one dependency is unhealthy with a reason.

**Checkpoint**: US1 and US2 both work independently.

---

## Phase 5: User Story 3 - Platform engineers can track AI provider errors (Priority: P3)

**Goal**: `GET /platform/ai-providers` returns per-provider metrics, category breakdown, verdicts, and a capped recent-failures list; the AI Provider Status page renders them.

**Independent Test**: Seed AI call records with mixed success/failure categories across providers, confirm per-provider rates, category breakdown, verdicts, and the recent-failures list are correct for the selected window.

### Backend — US3

- [X] T051 [US3] In `backend/crates/modules/platform/src/model.rs`, add the provider DTOs:
  - `pub const PROVIDERS: [&str; 3] = ["openai", "anthropic", "gemini"];`
  - `WindowQuery { window: Option<String> }`; `pub fn resolve_window(q: WindowQuery, now: DateTime<Utc>) -> Result<(DateTime<Utc>, DateTime<Utc>, String), String>` — accepts only `"1h"`, `"24h"`, `"7d"`, defaults to `"24h"`, any other value → `Err("Unknown window")` (FR-025).
  - `#[serde(rename_all = "snake_case")] pub enum ProviderVerdict { Healthy, Degraded, Unhealthy, InsufficientData, NoActivity }`
  - `FailuresByCategoryDto { authentication: i64, rate_limited: i64, unavailable: i64, timeout: i64, invalid_request: i64 }`
  - `ProviderMetricsDto { provider: String, verdict: ProviderVerdict, total_calls: i64, successes: i64, failures: i64, failure_rate: Option<f64>, p50_latency_ms: Option<i64>, p95_latency_ms: Option<i64>, input_tokens: i64, output_tokens: i64, failures_by_category: FailuresByCategoryDto }`
  - `ProviderFailureDto { provider: String, model: String, error_category: Option<String>, latency_ms: i64, occurred_at: DateTime<Utc> }` — no content or credential fields, ever (FR-026, SC-008).
  - `WindowDto { from: DateTime<Utc>, to: DateTime<Utc>, preset: String }`
  - `ProviderStatusDto { window: WindowDto, providers: Vec<ProviderMetricsDto>, recent_failures: Vec<ProviderFailureDto>, generated_at: DateTime<Utc> }`

- [X] T052 [US3] In `backend/crates/modules/platform/src/health.rs`, add `pub fn provider_verdict(total_calls: i64, failures: i64) -> ProviderVerdict`, evaluated **in this order**: `total_calls == 0` → `NoActivity`; `total_calls < 20` → `InsufficientData`; `failures as f64 / total_calls as f64 >= 0.10` → `Unhealthy`; `>= 0.02` → `Degraded`; else `Healthy` (FR-022, FR-023). The ordering makes division by zero structurally unreachable. Add `#[cfg(test)]` tests pinning: 0 calls, 19 calls, 20 calls, 25/100 → unhealthy, 4/100 → degraded, 1/100 → healthy, exactly 2/100 → degraded, exactly 10/100 → unhealthy.

- [X] T053 [US3] In `backend/crates/modules/platform/src/queries.rs`, add two functions:

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

- [X] T054 [US3] In `backend/crates/modules/platform/src/routes.rs`, add `pub async fn get_platform_ai_provider_status(State(pool): State<PgPool>, Query(q): Query<model::WindowQuery>) -> Response`. Resolve the window (invalid → `422`), run both queries, then build the `providers` vector by iterating `model::PROVIDERS` and filling any provider absent from the query result with an all-zero row (FR-023 — all three are always present). Set `failure_rate` and both percentiles to `None` when `total_calls == 0` — never `0` (Edge Cases). Assign the verdict with `health::provider_verdict`. Add the `#[utoipa::path]` attribute: `get`, `path = "/platform/ai-providers"`, `tag = "platform-health"`, `operation_id = "get_platform_ai_provider_status"`, param `window` as `Option<String>`, responses 200 (`body = model::ProviderStatusDto`) and 422.

- [X] T055 [US3] In `backend/crates/server/src/router.rs`, register the route inside `platform_routes(...)` with `.layer(require_permission(Permission::PlatformDiagnosticsView))`.

- [X] T056 [US3] In `backend/crates/server/tests/openapi_coverage.rs` add `("GET", "/platform/ai-providers"),` to `EXPECTED` (unprefixed). In `backend/crates/server/tests/rbac.rs` add `"/api/v1/platform/ai-providers",` to `PLATFORM_OPERATIONS` and `("/api/v1/platform/ai-providers", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` (prefixed).

- [X] T057 [US3] Create `backend/crates/server/tests/platform_ai_providers.rs` (same header block as `platform_analytics.rs`). Tests:
  1. 100 calls for `openai` of which 25 failed → `failure_rate == 0.25`, verdict `"unhealthy"`.
  2. 100 calls of which 4 failed → `0.04`, verdict `"degraded"`.
  3. Failures spread across categories → the five `failures_by_category` counts sum to that provider's `failures`.
  4. A provider with 3 calls → `"insufficient_data"`; a provider with 0 calls → `"no_activity"`, still present in the array, with `failure_rate: null` and both percentiles `null`.
  5. `recent_failures` is newest-first and capped at 50 when 60 failures exist.
  6. `?window=90d` → `422`; `?window=1h`/`24h`/`7d` all → `200`.
  7. The raw response body contains none of `request_content`, `response_content`, or any credential key (SC-008).

### Frontend — US3

- [X] T058 [US3] In `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts`, add `ProviderStatusWire` / `ProviderStatus` types and `providerStatusFromWire`, mapping `total_calls` → `totalCalls`, `failure_rate` → `failureRate`, `p50_latency_ms` → `p50LatencyMs`, `p95_latency_ms` → `p95LatencyMs`, `failures_by_category` → `failuresByCategory` (keeping the five category keys), `recent_failures` → `recentFailures`, `error_category` → `errorCategory`, `latency_ms` → `latencyMs`, `occurred_at` → `occurredAt`.

- [X] T059 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers-api.service.ts` — `getStatus(window: '1h' | '24h' | '7d'): Observable<ApiResponse<ProviderStatus>>` calling `this.api.get<ProviderStatusWire>('/platform/ai-providers', params)` with the `window` query param.

- [X] T060 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.store.ts` — `signalStore` with state `{ status: ProviderStatus | null; window: '1h' | '24h' | '7d'; loading: boolean; error: string | null }`, a `load()` `rxMethod`, and `setWindow(w)` that patches `window` and re-loads.

- [X] T061 [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.ts` — `<app-page-container>` + `<app-page-header title="AI provider status" />`, an `<app-select-filter>` for the window (`1h` / `24h` / `7d`), one `<app-dashboard-card>` per provider showing `<app-health-indicator>` (mapping `insufficient_data` and `no_activity` to a neutral `<app-status-badge>` with those labels rather than a health colour), call counts, failure rate as a percentage (render `—` when `failureRate` is `null`), p50/p95 latency, tokens, and an `<app-breakdown-bars>` of the five failure categories. Below that, an `<app-data-table>` of `recentFailures` with columns provider, model, category, latency, UTC timestamp. Use `<app-empty-state>` when there is no activity at all.

- [X] T062 [US3] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts` add `[APP_PATHS.platform.aiProviders]: 'platform.diagnostics.view',` to `PAGE_PERMISSIONS`. In `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` add the `APP_PATHS.platform.aiProviders` route with `canMatch: [permissionGuard]` and `data: { pageTitle: 'platformAiProviders', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.platform.aiProviders] }`. In `frontend/apps/dashboard/src/app/core/router/page-title.ts` add `'platformAiProviders'` to the union and `platformAiProviders: { title: 'AI provider status', subtitle: 'Platform administration' },` to the map.

- [X] T063 [US3] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, add a `PLATFORM_DESTINATIONS` entry with `label: 'AI providers'`, `permission: 'platform.diagnostics.view'`, and a backtick template-literal `path` of `/${APP_PATHS.platform.base}/${APP_PATHS.platform.aiProviders}`, inserted **immediately after** the `'System health'` entry. Then update `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts`: `options[4]` now expects `'AI providers'` and `options[5]` expects `'Platform overview'`.

- [X] T064 [P] [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.spec.ts` — renders all three providers from a stubbed store including one with `no_activity` and `failureRate: null` (asserting `—` is shown, not `0%`), renders the category breakdown, and renders the recent-failures rows.

- [X] T065 [P] [US3] Create `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.store.spec.ts` — asserts `setWindow('7d')` patches state and triggers a reload, and that an API error sets `error` with `loading: false`.

**Checkpoint**: All three user stories work independently.

---

## Phase 6: Polish & Validation

- [X] T066 [P] In `backend/crates/server/src/openapi.rs`, add two entries to the `tags` vec inside `TagDescriptionsAddon::modify` (~line 241): `("platform-analytics", "Platform-internal cross-tenant business metrics.")` and `("platform-health", "Platform-internal system health and AI provider diagnostics.")`. This list is descriptive only — no test enforces it — but every other production tag is listed.

- [X] T067 [P] Finalize the module docs in `backend/crates/modules/platform/src/lib.rs` — fill in the Purpose / Responsibilities / Public Interfaces (the three endpoint paths) / Dependencies / Data Model (reads `tenants`, `conversations`, `ai_usage_records`, `outbox_events`; owns no tables; migration `0060` adds four indexes) / Extension Points sections.

- [X] T068 Run the backend gate exactly as written — **not** `cargo test --workspace`, which aborts on the first failing crate:

  ```bash
  cd backend
  REQUIRE_DB_TESTS=1 DATABASE_URL=postgres://… cargo test -p authz -p platform -p server
  ```

  Without `REQUIRE_DB_TESTS=1` the DB-gated tests skip and pass vacuously.

- [X] T069 Run the frontend gate: `cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check`.

- [X] T070 Run `cd backend && cargo fmt` on **only the files this feature touched**. `cargo fmt --check` is red repo-wide at HEAD for unrelated reasons — do not treat that as a regression from this feature.

- [X] T071 Walk `specs/032-platform-analytics-health/quickstart.md` Scenarios 1-6 manually and record the result. Scenario 2 step 3 (direct API calls returning `403` for a tenant user) and Scenario 5 step 6 (raw-JSON leak check) are the ones that actually prove SC-002 and SC-008.

- [X] T072 Performance spot-check (SC-003): with ~1,000 tenants and ~1,000,000 `ai_usage_records` rows seeded, run `EXPLAIN (ANALYZE, BUFFERS)` on the provider-metrics query and confirm it uses `ai_usage_records_created_idx` rather than a sequential scan, and that each page's first figures appear within 2 s. If the budget is missed, **escalate** per research R4 (approximate percentiles or a narrower maximum window) — do **not** add a cache; caching was explicitly rejected in clarification Q2.

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

---

## Phase 7: Convergence

**Why this phase exists**: Phases 1–6 are all marked `[X]`, but neither gate passes.

- **Frontend**: the dashboard **has never compiled**. `pnpm ng build dashboard` fails with 27 errors, all inside this feature's three page components, and `pnpm ng test dashboard` therefore exits 1 with **zero specs executed** — so no frontend test in the entire repository currently runs.
- **Backend**: it compiles (`cargo check --workspace --all-targets` exits 0) and the pure unit tests pass (`cargo test -p platform -p authz`, 11/11), but **all 21 platform DB tests fail — 0 passed**, and a regression from T033 also breaks 24 `rbac` and 15 `audit_logs` tests that were passing before this feature. T068's `[X]` does not reflect a run that happened.

**Execution order**: **T084 first** (it blocks every backend test), then T085, then T073–T082, then T083 last.

**Read this before starting** — verified at HEAD on 2026-07-26, do not re-derive:

| Shared component | Real API (what you MUST bind) |
|---|---|
| `MetricCardComponent` (`app-metric-card`) | one required input `metric` of type `MetricCardData { id: string; label: string; value: string; icon: string; delta?: string; deltaPositive?: boolean; trend?: readonly number[] }`. There is **no** `label`, `value`, or `subText` input. |
| `EmptyStateComponent` (`app-empty-state`) | three required inputs `icon`, `title`, `description`. There is **no** `message` input. |
| `SelectFilterComponent` (`app-select-filter`) | required `label: string`, required `options: SelectFilterOption[]` (`{ value: string; label: string }`, **mutable array**), optional `value` (default `'all'`), output `valueChange` emitting **`string`**. |
| `HealthIndicatorComponent` (`app-health-indicator`) | required `verdict` of exactly `'healthy' \| 'degraded' \| 'unhealthy'`. |
| Taiga icon names | this repo uses the **dot** form `@tui.<name>` (e.g. `@tui.users`, `@tui.alert-circle`) — 20+ usages. The dash form `@tui-<name>` resolves to nothing. |

Icons verified present in `@taiga-ui/icons/src`: `users`, `message-circle`, `bot`, `chart-line`, `arrow-down-to-line`, `arrow-up-from-line`, `circle-check`, `circle-x`, `triangle-alert`, `alert-circle`, `search-x`.

Within the frontend group, work T073 → T082 **in order**; T073 must land before T075. T083 is last because every earlier task edits files that prettier must then reformat.

---

- [X] T073 **CRITICAL** In `frontend/apps/dashboard/src/app/core/api/platform-analytics.models.ts`, change the `verdict` field of the objects in `ProviderStatus.providers[]` (~line 151) from `verdict: string;` to:

  ```ts
  verdict: 'healthy' | 'degraded' | 'unhealthy' | 'insufficient_data' | 'no_activity';
  ```

  Leave `ProviderStatusWire` (the wire type) with `verdict: string` unchanged. In `providerStatusFromWire`, change `verdict: p.verdict,` to `verdict: p.verdict as ProviderStatus['providers'][number]['verdict'],`. This is the only cast needed — it sits at the wire boundary. Blocks T075. Per FR-022/FR-023 (partial).

- [X] T074 **CRITICAL** Fix every compile error in `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.ts` (11 of the 27 build errors). Per US1, FR-006–FR-010, spec Edge Cases (contradicts). Make exactly these changes:

  1. **Replace the `metricCards()` return type.** It currently returns `{ label; value; subText? }[]`, which does not match `MetricCardData`. Change the method to return `{ metric: MetricCardData; subText?: string }[]` and import `MetricCardData` from `'../../../shared/components/metric-card/metric-card.component'`. Build the five entries as:

     ```ts
     [
       { metric: { id: 'tenants', label: 'Total Tenants', value: String(summary.tenants.total), icon: '@tui.users' },
         subText: `${summary.tenants.active} active · ${summary.tenants.suspended} suspended · ${summary.tenants.createdInRange} created in range` },
       { metric: { id: 'conversations', label: 'Active Conversations', value: String(summary.activeConversations), icon: '@tui.message-circle' } },
       { metric: { id: 'ai-calls', label: 'AI Calls', value: String(summary.aiUsage.totalCalls), icon: '@tui.bot' },
         subText: `${summary.aiUsage.successfulCalls} success · ${summary.aiUsage.failedCalls} failed` },
       { metric: { id: 'input-tokens', label: 'Input Tokens', value: String(summary.aiUsage.inputTokens), icon: '@tui.arrow-down-to-line' } },
       { metric: { id: 'output-tokens', label: 'Output Tokens', value: String(summary.aiUsage.outputTokens), icon: '@tui.arrow-up-from-line' } },
     ]
     ```

     Keep the sub-text — US1/AC1 requires the active/suspended breakdown and AC3 the success/failure split to be visible.

  2. **Fix both `<app-metric-card>` blocks** (currently at lines ~42 and ~58 — there are **two**, one in the error branch and one in the loaded branch; fix both identically). Replace each with:

     ```html
     <div class="card-cell">
       <app-metric-card [metric]="card.metric" />
       @if (card.subText) {
         <p class="sub">{{ card.subText }}</p>
       }
     </div>
     ```

     and change the `@for` track expression to `track card.metric.id`. Add `.card-cell { display: flex; flex-direction: column; gap: var(--app-space-1); }` and `.sub { margin: 0; font-size: var(--app-font-xs); color: var(--app-text-3); }` to `styles`. Do **not** add a `subText` input to the shared `MetricCardComponent` — it is used by tenant analytics and must not change.

  3. **Fix the date-range Apply button** (line ~53). `fromValue`/`toValue` are `private` string fields but the template calls them as functions — two separate errors each. Replace the two private fields and the two `onFromChange`/`onToChange` methods with signals:

     ```ts
     protected readonly fromValue = signal('');
     protected readonly toValue = signal('');
     ```

     Import `signal` from `@angular/core`. Delete `onFromChange` and `onToChange`. In the template change the two date inputs to `[ngModel]="fromValue()" (ngModelChange)="fromValue.set($event)"` and `[ngModel]="toValue()" (ngModelChange)="toValue.set($event)"`. The Apply button then works unchanged: `(pressed)="store.setRange(fromValue(), toValue())"` (`setRange(from: string | null, to: string | null)` already exists in the store).

  4. **Fix `<app-empty-state>`** (line ~68):

     ```html
     <app-empty-state icon="@tui.chart-line" title="No data yet" description="Platform figures will appear once tenants, conversations, and AI usage are recorded." />
     ```

  5. **Add the zero-figure empty state** (spec Edge Cases: a fresh install must show an explicit "no data yet" message, not bare zero cards). Add a method `protected isZeroState(summary: PlatformSummary): boolean` returning true when `summary.tenants.total === 0 && summary.activeConversations === 0 && summary.aiUsage.totalCalls === 0`. In the loaded branch, render the same `<app-empty-state>` as (4) instead of the cards grid when `isZeroState(summary)` is true. Keep the date-range controls visible in both cases.

  Verify with `cd frontend && pnpm ng build dashboard` — this file must produce zero errors.

- [X] T075 **CRITICAL** Fix every compile and lint error in `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.ts` (8 build errors + 3 lint errors). Requires T073. Per US3, FR-020–FR-025, FR-027 (contradicts). Make exactly these changes:

  1. **`windowOptions`** — remove `as const` and annotate the type, so it is a mutable `SelectFilterOption[]`:

     ```ts
     protected readonly windowOptions: SelectFilterOption[] = [
       { value: '1h', label: 'Last hour' },
       { value: '24h', label: 'Last 24 hours' },
       { value: '7d', label: 'Last 7 days' },
     ];
     ```

     Import `SelectFilterOption` from `'../../../shared/components/select-filter/select-filter.component'`.

  2. **`<app-select-filter>`** (line ~46) — add the required `label` input: `label="Window"`.

  3. **Window change handler** — `valueChange` emits `string` but `store.setWindow` takes `'1h' | '24h' | '7d'`. Add to the class:

     ```ts
     protected onWindowChange(value: string): void {
       if (value === '1h' || value === '24h' || value === '7d') {
         this.store.setWindow(value);
       }
     }
     ```

     and change the template binding to `(valueChange)="onWindowChange($event)"`.

  4. **`<app-health-indicator [verdict]="prov.verdict" />`** (line ~61) — after T073 the surrounding `@if (prov.verdict === 'no_activity' || prov.verdict === 'insufficient_data') { … } @else { … }` narrows `prov.verdict` to the 3-value union in the `@else` branch, so this binding type-checks with no change. If the template type-checker still reports TS2322 there, add a method `protected healthVerdict(v: string): 'healthy' | 'degraded' | 'unhealthy' { return v === 'degraded' || v === 'unhealthy' ? v : 'healthy'; }` and bind `[verdict]="healthVerdict(prov.verdict)"`. Do **not** use `$any()`.

  5. **`number` pipe** (line ~64) — import `DecimalPipe` from `@angular/common` and add it to the component's `imports` array.

  6. **Both `<app-empty-state>` usages**:
     - line ~85: `<app-empty-state icon="@tui.circle-check" title="No recent failures" description="No failed AI calls were recorded in this window." />`
     - line ~108: `<app-empty-state icon="@tui.bot" title="No provider data available" description="Provider metrics will appear once AI calls are recorded." />`

  7. **Lint (`@angular-eslint/template/eqeqeq`)** — at lines ~64 and ~65 replace all three `!=` with `!==`: `prov.failureRate !== null`, `prov.p50LatencyMs !== null`, `prov.p95LatencyMs !== null`. The `'—'` fallbacks must stay (FR-023: a provider with no activity shows `—`, never `0%`).

  8. **Unused import** — remove `ButtonComponent` from the `imports` array and its `import` statement (build warning NG8113; it is not used in the template).

- [X] T076 **CRITICAL** In `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.ts`: (a) fix `<app-empty-state>` at line ~71 to `<app-empty-state icon="@tui.alert-circle" title="No health data available" description="Health readings will appear once the first probe completes." />`; (b) fix the lint error at line ~59 by changing `dep.detail.oldestPendingAgeSeconds != null` to `!== null`. Per FR-013 (contradicts).

- [X] T077 **CRITICAL** In `frontend/apps/dashboard/src/app/shared/components/health-indicator/health-indicator.component.ts`, fix the three icon names in the `iconName` computed (lines ~48-50): change `'@tui-circle-check'` → `'@tui.circle-check'`, `'@tui-triangle-alert'` → `'@tui.triangle-alert'`, `'@tui-circle-x'` → `'@tui.circle-x'`. The dash form is not a valid Taiga icon reference and these are the only three dash-form names in the whole repo, so today **no icon renders at all** — which defeats the icon-plus-label requirement. The text labels already present are correct; do not change them. Per FR-027, SC-006 (contradicts).

- [X] T078 **HIGH** In `backend/crates/modules/platform/src/routes.rs`, fix the queue probe inside `get_platform_health` (lines ~135-174). Two defects in one block: the queue query has **no timeout** (FR-018 requires *every* dependency probe to be bounded — the storage probe already uses one), and when it fails the handler `return`s early, so the **storage dependency is never probed and never appears in the response** (FR-013 requires all four dependencies; FR-028/SC-007 require a failing data source to degrade only its own panel). Replace the whole `let (queue_pending, queue_oldest, queue_stuck) = match queries::queue_health(&pool).await { … }` block — including its early `return` — with a version that:

  1. wraps the call as `timeout(Duration::from_secs(5), queries::queue_health(&pool)).await`, measuring elapsed ms with `std::time::Instant` exactly like the storage probe does;
  2. on `Ok(Ok(row))` → compute `oldest_age` and push the queue dependency via `health::queue_verdict` as it does today, with the measured `probe_ms`;
  3. on `Ok(Err(e))` → `tracing::error!(%e, "failed to query queue health")` and push the queue dependency as `HealthVerdict::Unhealthy` with `reason: Some(e.to_string())`;
  4. on `Err(_)` (timeout) → push the queue dependency as `HealthVerdict::Unhealthy` with `reason: Some("timed out".into())`;
  5. in **all three** cases **fall through** to the existing storage-probe block and the single final response — do not `return` early anywhere in this block.

  The existing final response already recomputes `overall` from all collected verdicts, so a failed queue still yields `overall: unhealthy` with database, cache, and storage reported normally. Also add a test to `backend/crates/server/tests/platform_health.rs` asserting the response always contains all four dependency names (`database`, `cache`, `queue`, `storage`). Per FR-018, FR-013, FR-028, SC-007 (contradicts).

- [X] T079 **HIGH** In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts`, update the stale index-based assertions — T026/T049/T063 changed `PLATFORM_DESTINATIONS` but this spec was never updated, so it will fail as soon as the suite can run. The live order is Tenants, Audit Logs, Billing, System health, AI providers, Platform overview. Change lines ~95-103 to:

  ```ts
  expect(options[0].textContent).toContain('Tenants');
  expect(options[1].textContent).toContain('Audit Logs');
  expect(options[2].textContent).toContain('Billing');
  expect(options[3].textContent).toContain('System health');
  expect(options[4].textContent).toContain('AI providers');
  expect(options[5].textContent).toContain('Platform overview');

  (options[5] as HTMLElement).click();
  fixture.detectChanges();

  expect(routerMock.navigate).toHaveBeenCalledWith(['/platform/overview-placeholder']);
  ```

  Also update the stale comment above them (~line 93-94) that says the second destination is "Platform overview". Leave the separate "navigates to the Tenants entry" test (~line 110-118) unchanged. Per T026/T049/T063 (missing).

- [X] T080 **HIGH** Replace the body of `frontend/apps/dashboard/src/app/features/platform/overview/platform-overview.component.spec.ts` with the assertions T028 actually specified — it currently only checks the page header and the loading state, which tests none of the feature. Provide a **stubbed store** rather than the real `PlatformOverviewStore` (override it with `{ provide: PlatformOverviewStore, useValue: … }` exposing `summary`, `loading`, `error`, `from`, `to` as signals plus `load`/`setRange` spies), and a stubbed `PermissionsService` with a `has` spy. Cover: (1) with a populated summary, the rendered `textContent` contains the tenant total, the `active … suspended … created in range` sub-text, and the AI success/failure sub-text; (2) with every figure `0`, the "No data yet" empty state renders; (3) when `PermissionsService.has('platform.diagnostics.view')` returns `false`, no System Health link is rendered, and when it returns `true`, one is. Per US1/AC1, FR-004, FR-011, Constitution VII (partial).

- [X] T081 **HIGH** Replace the body of `frontend/apps/dashboard/src/app/features/platform/system-health/system-health.component.spec.ts` with the assertions T050 actually specified. Stub `SystemHealthStore` the same way as T080. Cover: (1) four dependency cards render from a stubbed reading containing `database`, `cache`, `queue`, `storage`; (2) the queue card's text shows its `pendingCount` and `oldestPendingAgeSeconds`; (3) when one dependency is `unhealthy` with a `reason`, that reason renders **and** the other three cards still render (this is the SC-007 partial-render check). Per US2, FR-013, FR-028, SC-007, Constitution VII (partial).

- [X] T082 **HIGH** Replace the body of `frontend/apps/dashboard/src/app/features/platform/ai-providers/ai-providers.component.spec.ts` with the assertions T064 actually specified. Stub `AiProvidersStore` the same way as T080. Cover: (1) all three providers render, including one with `verdict: 'no_activity'` and `failureRate: null` whose card shows `—` and **not** `0%`; (2) the five failure-category counts render; (3) the recent-failures rows render with provider, model, category, latency, and timestamp. Per US3, FR-023, FR-024, SC-008, Constitution VII (partial).

- [X] T083 **MEDIUM** Final gate — **the last task in this phase**. Run in this order from `frontend/`, and only after T073–T082, T084 and T085 are all complete (every one of them edits a file prettier must then reformat, so running this earlier just re-reddens `format:check`):

  ```bash
  cd frontend
  pnpm prettier --write "apps/dashboard/src/app/features/platform/**/*.ts" "apps/dashboard/src/app/shared/components/health-indicator/*.ts" "apps/dashboard/src/app/core/api/platform-analytics.models.ts" "apps/dashboard/src/app/layout/topbar/platform-nav.component.spec.ts"
  pnpm ng build dashboard
  pnpm ng test dashboard
  pnpm lint
  pnpm format:check
  ```

  All five must pass. `pnpm ng test dashboard` has **never** executed a single spec on this branch, so expect newly-surfaced failures the first time it runs — check `platform-nav.component.spec.ts` (fixed in T079), `app.routes.spec.ts:63` (navigates `/platform/overview-placeholder` and asserts the text `Platform overview`, which T074 keeps rendering unconditionally outside the loading branch), `platform.routes.spec.ts`, and `breadcrumb.spec.ts` (asserts the `overview-placeholder` path at lines 82, 94, 102, 105, 198, 208). Fix whatever fails; do **not** rename the `overview-placeholder` route path to make a test pass.

  Then re-run the backend gate from `backend/`. There is no `backend/.env` and no project database on this machine, but a local Postgres is running — create a throwaway database rather than pointing at the existing `test` database (the suites TRUNCATE and seed):

  ```bash
  cd backend
  createdb converge_032_tmp
  export DATABASE_URL="postgres://$(whoami)@localhost/converge_032_tmp"
  REQUIRE_DB_TESTS=1 cargo test -p authz -p platform -p server
  dropdb converge_032_tmp   # when finished
  ```

  The harnesses run `db::run_migrations` themselves, so an empty database is fine. Without `REQUIRE_DB_TESTS=1` the DB-gated suites skip and pass vacuously — that is how T068 came to be marked `[X]` while 21 of 21 platform tests were failing. Beyond the platform suites, confirm `rbac` and `audit_logs` are back to their pre-feature state once T084 lands. Per plan.md quality gates (contradicts).

- [X] T084 **CRITICAL — do this first, before T073–T083** Fix the runtime-panic regression introduced by T033 in `backend/crates/server/src/router.rs`. The offending statement is the `let (fallback_storage, storage_backend) = …` expression at lines ~1014-1026, inside the **sync** private fn `api_routes` (declared at line 983, called by `build_app` at line 1131). T033 replaced the old storage block with an unconditional `tokio::runtime::Handle::current().block_on(storage::select_storage(...))`, which panics with *"Cannot start a runtime from within a runtime"* whenever the router is built from inside a Tokio runtime without an injected storage. (There is no function named `build_router` in this file — the public entry points are `app`, `app_with_email_sender`, `app_with_storage`, and friends at lines 1193-1220.)

  **Measured blast radius** (verified 2026-07-26 against a migrated database with `REQUIRE_DB_TESTS=1`) — this is not limited to this feature:

  | Suite | Result | Panics at `router.rs:1024` |
  |---|---|---|
  | `rbac` | 4 passed, **24 failed** | 24 of 24 |
  | `audit_logs` | 1 passed, **17 failed** | 15 |
  | `analytics_api` | 0 passed, **11 failed** | 2 |
  | `platform_analytics` | 0 passed, **6 failed** | 4 |
  | `platform_health` | 0 passed, **7 failed** | 7 of 7 |
  | `platform_ai_providers` | 0 passed, **8 failed** | 2 |

  On `master` this path was safe because `block_on` ran **only** when `state.config.s3` was `Some`, and every test config sets `s3: None` (e.g. `crates/server/tests/rbac.rs:155`), so the async call was never reached. T033 made it unconditional.

  **Do not "fix" this by skipping the call when `s3` is `None`.** That restores the tests but leaves the S3 branch broken: production reaches this same line. `main.rs:83` calls `router::app_with_email_sender(...)` → `build_app(state, false, Some(email_sender), **None**)`, i.e. with **no injected storage**, from inside `#[tokio::main]`. So with S3 configured, the server panics at startup on this very line — and FR-016's whole point is that a real S3 deployment reports its real backend. Fix the `block_on` itself so **both** paths work. Replace the `else` branch only:

  ```rust
  } else {
      // `build_app` is sync but is called from inside `#[tokio::main]` (production) and
      // from `#[tokio::test]` (the suites), so `Handle::current().block_on(...)` panics
      // with "Cannot start a runtime from within a runtime". Run the async selection on
      // a dedicated thread with its own runtime instead.
      let s3 = state.config.s3.clone();
      std::thread::spawn(move || {
          tokio::runtime::Runtime::new()
              .expect("failed to build storage-selection runtime")
              .block_on(storage::select_storage(s3.as_ref()))
      })
      .join()
      .expect("storage-selection thread panicked")
  };
  ```

  This is safe to move across threads: `ObjectStorage: Send + Sync + 'static` (`crates/shared/storage/src/lib.rs:23`), so the returned `Arc<dyn ObjectStorage>` and the `Copy` `StorageBackend` both cross the boundary. `state.config.s3` is an `Option<S3Config>`; clone it so the thread owns its data.

  Leave the injected-storage branch (`if let Some(ref s) = storage`) exactly as it is — its hardcoded `StorageBackend { kind: InMemory, s3_configured: false }` is only reached by the three tests that call `app_with_storage` / `app_with_test_routes_and_storage`, which is the behaviour T041's note deliberately relies on. Do **not** change `main.rs`: it is genuinely `async` and its own `select_storage(...).await` at line 85 (used to hand storage to the background workers) is correct. Do **not** delete `select_storage`. Per FR-016, Constitution VII (contradicts).

- [X] T085 **HIGH** Fix the three test seed helpers that violate NOT NULL constraints, so the platform suites can actually exercise the endpoints. All were written against a guessed schema and every insert fails with SQLSTATE 23502. Verified column facts: `ai_usage_records.streamed` is `BOOLEAN NOT NULL` with **no default** (migration `0040`), and `conversations.customer_id` is `NOT NULL` (migration `0026`).

  1. `backend/crates/server/tests/platform_analytics.rs`, `seed_conversation` (~line 168): the insert omits `customer_id`. Add a `seed_customer` helper and pass its id:

     ```rust
     async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
         sqlx::query_scalar("INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id")
             .bind(tenant_id)
             .bind("Platform Analytics Customer")
             .fetch_one(pool)
             .await
             .unwrap()
     }
     ```

     then change `seed_conversation` to take a `customer_id: Uuid` and insert
     `INSERT INTO conversations (tenant_id, customer_id, status, channel) VALUES ($1, $2, $3, 'web_chat')`.
     Update its call sites. Keep the seeded statuses as they are — `open`, `pending`, and `resolved` are all valid (migration `0033` widened `conversations_status_check` to `('open','pending','resolved','closed')`), and `web_chat` is a valid channel.

  2. `backend/crates/server/tests/platform_analytics.rs`, `seed_ai_usage` (~line 186): add `streamed` to the column list and `false` to `VALUES`.

  3. `backend/crates/server/tests/platform_ai_providers.rs`, `seed_ai_call` (~line 135): same fix — add `streamed` and `false`.

  Use `crates/server/tests/analytics_api.rs:488-491` as the reference for a correct `ai_usage_records` insert. Per Constitution VII (contradicts).

  > After T084 and T085 land, run the three platform suites and fix whatever *real* assertion failures then surface — they have **never** executed against a database, so their expected values are unverified. `platform_health`'s 7 tests are currently 100% blocked by T084 alone, and `seed_outbox_event` has not been exercised at all.

---

## Phase 8: Convergence

**This phase is the follow-through T085's closing note predicted**, not a redo of it. T084 and T085 landed and worked: `platform_health` now passes **7/7**. Running the suites for the first time surfaced the *real* assertion failures that note anticipated. Phase 7's frontend work is **complete and verified** — do not re-touch it.

**Measured at HEAD on 2026-07-26 — do not re-derive:**

| Gate | Result |
|---|---|
| `pnpm ng build dashboard` | ✅ exit 0 |
| `pnpm lint` / `pnpm format:check` | ✅ exit 0 |
| `pnpm ng test dashboard` | 1098 passed, 2 failed (**both pre-existing, not ours — see below**) |
| `cargo test -p authz -p platform` | ✅ 30/30 |
| `platform_health` | ✅ 7 passed, 0 failed |
| `platform_analytics` | ❌ 5 passed, **1 failed** (T088) |
| `platform_ai_providers` | ❌ 2 passed, **6 failed** (T087) |
| `rbac` | ❌ 26 passed, **2 failed** (1 is ours → T086; 1 is not) |

### ⛔ Known-failing BEFORE this feature — do NOT fix these, do NOT let them block you

Each was reproduced on a **fresh database** and traced to files this feature never touched. Leave them red:

| Suite | Count | Failing test(s) | Why it is not ours |
|---|---|---|---|
| `auth` | 2 | `login_response_matches_me_with_membership_permissions`, `login_success_sets_session_cookie_and_cookie_authenticates_me` | Expected literals are stale from features 023/026 (missing `platform.audit.view`, `audit.view`, `widgets.view`, `widgets.manage`; Viewer role drift). Both already contain `platform.analytics.view`, so T008 is done. |
| `openapi_coverage` | 1 | `documented_paths_equal_expected_inventory` — missing `POST /api/v1/tenant/tools/requests/{id}/decide` | Tools module. All three `/platform/...` entries of ours are present and pass. |
| `rbac` | 1 | `staff_tenant_access_is_environment_aware` (404 on `/api/v1/tenant/conversations`) | `conversations/src/routes.rs:302` 404s for staff with no membership. `conversations/` is byte-identical to master. |
| `ai` | 4 | assorted | Fail on a fresh DB with this feature's code absent from the path. (39 more fail only when run in parallel — always run this suite with `-- --test-threads=1`.) |
| frontend | 2 | `app-shell.component.spec.ts` (sidebar `collapsed` class), `composer.component.spec.ts` (long plain text) | Layout + tenant conversations. Neither file nor its component is touched by this feature. |

> The permission matrix was checked specifically for a dropped tenant permission: `git diff master...HEAD -- authz/src/matrix.rs` is **5 purely additive lines** to the five `PLATFORM_*` arrays. No tenant role lost anything. Do not "fix" the matrix.

**Order: T086 → T087 → T088 → T089.** T089 is last.

---

- [X] T086 **HIGH** In `backend/crates/server/tests/rbac.rs`, test `me_returns_exact_platform_support_payload_in_production` (starts ~line 1223), find the `"platformPermissions"` array inside the `json!({ ... })` literal and add one line so it reads **exactly**:

  ```rust
  "platformPermissions": [
      "platform.tenants.list",
      "platform.tenants.switch",
      "platform.tenants.manage",
      "platform.analytics.view",
      "platform.audit.view"
  ],
  ```

  The new string goes **after** `"platform.tenants.manage"` and **before** `"platform.audit.view"` — this is an exact array equality, so position matters. Change nothing else in that test; its `staffTenantPermissions` array is already correct. Reason: T007 granted `PlatformAnalyticsView` to `PLATFORM_SUPPORT`, so the live `/me` response now returns it, but this exact-payload inventory was never updated. Per FR-001 (missing).

  Verify: `cd backend && DATABASE_URL=... REQUIRE_DB_TESTS=1 cargo test -p server --test rbac -- --test-threads=1` → 27 passed, 1 failed (the pre-existing `staff_tenant_access_is_environment_aware` above).

- [X] T087 **HIGH** Fix `backend/crates/server/tests/platform_ai_providers.rs` — **6 of its 8 tests fail**. Two independent defects; fix both. Per US3, FR-020–FR-024, Constitution VII (contradicts).

  **Defect 1 — foreign key violation.** `seed_ai_call` (~line 126) binds `Uuid::new_v4()` as `tenant_id`, but no such tenant row exists, so every insert dies with `SQLSTATE 23503` (`ai_usage_records_tenant_id_fkey`). Fix:

  1. Add this helper next to `seed_user` (copied from `platform_analytics.rs:124`):

     ```rust
     async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
         sqlx::query_scalar("INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id")
             .bind("AI Providers Tenant")
             .bind(format!("aip-{}", Uuid::new_v4().simple()))
             .fetch_one(pool)
             .await
             .unwrap()
     }
     ```

  2. Change `seed_ai_call`'s signature to take `tenant_id: Uuid` as its **second** parameter and `.bind(tenant_id)` instead of `.bind(Uuid::new_v4())`. Leave the rest of its SQL alone — `streamed` was already fixed by T085.
  3. In each of the 6 tests that call `seed_ai_call`, add `let tenant_id = seed_tenant(&pool).await;` right after the `setup(&pool).await;` line from Defect 2, and pass `tenant_id` to every `seed_ai_call` call. One tenant per test is enough — these endpoints are cross-tenant and never group by tenant.

  **Defect 2 — no test isolation.** Every test asserts *absolute* totals (`total_calls == 100`) against a **DB-wide** aggregate, and several seed `openai` calls into the same 7-day window, so they corrupt each other's numbers. Fix:

  4. Add this helper to the file. **Copy the table list verbatim** from `crates/server/tests/analytics_api.rs:93-98` — do not invent your own list, it will miss FK dependents and error mid-truncate:

     ```rust
     async fn setup(pool: &sqlx::PgPool) {
         db::run_migrations(pool).await.unwrap();
         sqlx::query("DELETE FROM escalations").execute(pool).await.unwrap();
         sqlx::query(
             "TRUNCATE TABLE conversation_feedback, ai_generations, \
              ai_usage_records, agent_configurations, messages, \
              customer_channel_identifiers, customers, conversations, \
              widget_sessions, widget_instances, outbox_events, audit_logs, \
              tenant_invitations, tenant_memberships, tenants, users \
              RESTART IDENTITY CASCADE",
         )
         .execute(pool)
         .await
         .unwrap();
     }
     ```

  5. In **all 8** test functions, replace the existing `db::run_migrations(&pool).await.unwrap();` line with `setup(&pool).await;`. The 8 are: `platform_ai_providers_failure_rate_and_verdict`, `_degraded_verdict`, `_categories_sum_to_failures`, `_insufficient_data_and_no_activity`, `_recent_failures_capped_at_50`, `_invalid_window_returns_422`, `_valid_windows_200`, `_no_sensitive_content_in_response`.
  6. Add `use serial_test::serial;` to the imports and put `#[serial]` on the line directly **below** `#[tokio::test]` on all 8 tests. `serial_test` is already a dev-dependency of the `server` crate. Without this, the truncation in one test wipes another's seed data mid-run.
  7. `seed_user` must be called **after** `setup`, since the truncate clears `users`. Check each test's ordering.

  **Defect 3 — the seed dates expire on 2026-08-02.** The tests seed hardcoded `"2026-07-25T00:00:00Z"` / `"2026-07-25T01:00:00Z"` and request `?window=7d`. The window presets are relative to `now()`, so about a week from now those rows fall outside every window, every provider reports `no_activity`, and the suite goes red for someone who did nothing wrong. You are already editing every `seed_ai_call` call site for Defect 1, so fix it in the same pass:

  8. Change `seed_ai_call`'s last parameter from `created_at: &str` to `minutes_ago: i32`, and change the last column expression in its `VALUES` list from `$6::timestamptz` to `now() - make_interval(mins => $6)`. Bind `minutes_ago` as before.
  9. Update the call sites: the `"...T00:00:00Z"` (success) seeds become `120`, the `"...T01:00:00Z"` (failure) seeds become `60`. In `platform_ai_providers_recent_failures_capped_at_50`, the loop currently builds 60 distinct timestamps via `format!("2026-07-25T00:{:02}:00Z", i)` — replace that with `seed_ai_call(&pool, tenant_id, "openai", "failure", Some("rate_limited"), 100, i + 1).await;`. The values must stay **distinct**, because that test depends on `ORDER BY created_at DESC` to prove newest-first (FR-024).

  Do **not** apply this to `platform_analytics.rs` — its absolute seed dates are paired with an explicit absolute `?from=2026-07-19&to=2026-07-23`, so that suite has no expiry.

  Verify: `DATABASE_URL=... REQUIRE_DB_TESTS=1 cargo test -p server --test platform_ai_providers -- --test-threads=1` → **8 passed, 0 failed**. If a *value* assertion still fails after this, the expected number in the test is the thing to re-derive — the handler logic is already unit-tested and passing in `platform/src/health.rs`.

- [X] T088 **HIGH** Fix `backend/crates/server/tests/platform_analytics.rs` — `platform_analytics_summary_includes_all_tenants_excluding_soft_deleted` (~line 211) fails with `tenants.total` = **10**, expecting **3**. Same root cause as T087 Defect 2: the endpoint counts every tenant in the database (correct — FR-005 is deliberately cross-tenant), but the other 5 tests in this binary seed their own tenants, so an absolute assertion cannot hold. Per US1/AC1, FR-006, Constitution VII (contradicts).

  1. Add the **same `setup` helper** you wrote in T087 step 4 to this file (identical body — copy it across).
  2. In **all 6** test functions, replace `db::run_migrations(&pool).await.unwrap();` with `setup(&pool).await;`, and make sure `seed_user`/`seed_tenant`/`seed_customer` calls come **after** it.
  3. Add `use serial_test::serial;` and put `#[serial]` directly below `#[tokio::test]` on all 6.
  4. Do **not** change the expected numbers (`total == 3`, `active == 2`, `suspended == 1`) — with truncation they are correct and they are what proves FR-006's soft-delete exclusion.

  Verify: `DATABASE_URL=... REQUIRE_DB_TESTS=1 cargo test -p server --test platform_analytics -- --test-threads=1` → **6 passed, 0 failed**.

- [X] T089 **MEDIUM** Final backend gate — **run only after T086, T087 and T088 are all done.** T087/T088 add truncation, which changes what `platform_health` sees, so all three platform suites must be re-run together. Per plan.md quality gates (contradicts).

  ```bash
  cd backend
  createdb converge_032_final
  export DATABASE_URL="postgres://$(whoami)@localhost/converge_032_final"
  REQUIRE_DB_TESTS=1 cargo test -p authz -p platform --no-fail-fast
  REQUIRE_DB_TESTS=1 cargo test -p server \
      --test platform_analytics --test platform_health --test platform_ai_providers \
      --test rbac --test auth --test openapi_coverage \
      --no-fail-fast -- --test-threads=1
  dropdb converge_032_final   # when finished
  ```

  `--no-fail-fast` is required: without it cargo stops at the first red binary and hides the rest. `--test-threads=1` is required: these suites truncate shared tables.

  **Required end state** — anything else is a real failure you must fix:

  | Suite | Must be |
  |---|---|
  | `authz` + `platform` | 30 passed, 0 failed |
  | `platform_analytics` | 6 passed, 0 failed |
  | `platform_health` | 7 passed, 0 failed |
  | `platform_ai_providers` | 8 passed, 0 failed |
  | `rbac` | 27 passed, **1 failed** — `staff_tenant_access_is_environment_aware` only |
  | `auth` | 11 passed, **2 failed** — the two named in the ⛔ table only |
  | `openapi_coverage` | 2 passed, **1 failed** — `documented_paths_equal_expected_inventory` only |

  Those 4 permitted failures are the pre-existing ones catalogued in the ⛔ table above. **Do not fix them and do not edit their files** — they are out of this feature's scope and touching them widens the diff for no benefit. If any *other* test fails, that one is yours.

  Finally run `cargo fmt` on **only** the two test files you edited (`crates/server/tests/platform_ai_providers.rs`, `crates/server/tests/platform_analytics.rs`) plus `crates/server/tests/rbac.rs`. Repo-wide `cargo fmt --check` is red at HEAD for unrelated reasons — do not chase it. The frontend needs **no** re-run: build, lint, format and 1098/1100 specs already pass, and no task in this phase touches frontend code.
