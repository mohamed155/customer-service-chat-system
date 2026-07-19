# Tasks: Analytics Foundation

**Input**: Design documents from `/specs/025-analytics-foundation/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/analytics-api.md](contracts/analytics-api.md)

**Tests**: Included. Constitution Principle VII (Test-First & Regression Discipline) requires them for shipped functionality.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US4)
- Every task states its exact file path and is self-contained — do not rely on reading other tasks.

## How to work these tasks

1. Do them in ID order unless you are running `[P]` tasks together.
2. Each task lists **Do**, and where useful **Copy the pattern from** (an existing file to imitate) and **Done when** (how to check).
3. Never invent SQL, column names, or API shapes — the exact strings you need are in the task.
4. After each backend task, run `cd backend && cargo check`. After each frontend task, run `cd frontend && pnpm ng build dashboard`.

## Reference: facts you will need repeatedly

**Existing DB columns (do not guess):**

- `conversations(id, tenant_id, customer_id, channel, status, last_activity_at, created_at, updated_at, deleted_at, assigned_membership_id, escalated_at, ai_handling)` — `status` ∈ `open|pending|resolved|closed`; `channel` ∈ `email|phone|web_chat|whatsapp|telegram|widget`
- `messages(id, tenant_id, conversation_id, kind, sender_membership_id, logged_by_membership_id, body, seq, created_at)` — `kind` ∈ `customer|reply|note|ai|system`
- `escalations(id, tenant_id, conversation_id, reason, status, escalated_at, ...)`
- `conversation_feedback(id, tenant_id, conversation_id, channel, rating, comment, submitted_at, created_at)`
- `ai_usage_records(id, tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at)` — **no conversation_id column**
- `ai_generations(id, tenant_id, conversation_id, trigger_message_id, response_message_id, usage_record_id, outcome, attempts, latency_ms, created_at)`

**Metric definitions (from spec.md Assumptions + Clarifications):**

- Conversation is **handed off** if an `escalations` row exists for it. Never use `conversations.escalated_at` — it is cleared when the escalation closes.
- Conversation is **concluded** if `status IN ('resolved','closed')`.
- Conversation is **AI-resolved** if concluded AND no `escalations` row exists.
- Conversations are attributed to the date range by `created_at`; feedback by `submitted_at`; tokens by `ai_usage_records.created_at`.
- Soft-deleted conversations (`deleted_at IS NOT NULL`) are excluded from every metric.
- Token totals count **all** usage rows regardless of `status`; `input_tokens`/`output_tokens` are nullable and must be `COALESCE`d to 0.

**Date binding convention (used by every query):**

- `$from_ts` = `from` date at `00:00:00Z` (inclusive lower bound)
- `$to_ts` = (`to` date + 1 day) at `00:00:00Z` (**exclusive** upper bound — always use `< $to_ts`, never `<=`)
- `$to_date` = the `to` date itself (inclusive) — **only** for `generate_series`, which includes its end value. Using `$to_ts` there would emit one extra bogus day.

---

## Phase 1: Setup

**Purpose**: Make the placeholder `analytics` crate buildable and add the index migration.

- [X] T001 Add dependencies to `backend/crates/modules/analytics/Cargo.toml`. The file currently has only `[package]`. Append a `[dependencies]` section copied verbatim from `backend/crates/modules/feedback/Cargo.toml` but **without** the `widgets` line. Final dependency list: `axum.workspace = true`, `chrono = { workspace = true, features = ["serde"] }`, `kernel = { path = "../../shared/kernel" }`, `serde.workspace = true`, `sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }`, `tenancy = { path = "../tenancy" }`, `tracing.workspace = true`, `utoipa.workspace = true`, `uuid.workspace = true`. **Done when** `cd backend && cargo check -p analytics` succeeds.

- [X] T002 Register the analytics crate with the server in `backend/crates/server/Cargo.toml`. In the `[dependencies]` section add the line `analytics = { path = "../modules/analytics" }` immediately after the existing `authz = { path = "../modules/authz" }` line. **Done when** `cd backend && cargo check -p server` succeeds.

- [X] T003 [P] Create the migration file `backend/migrations/0052_analytics_indexes.sql` with exactly this content (indexes only — this feature adds no tables):

  ```sql
  -- Migration 0052: indexes for analytics aggregation queries (spec 025).
  -- No new tables: analytics reads existing conversation/message/feedback/usage data.

  -- Drives every conversation-cohort scan (volume, rates, breakdown). Existing
  -- conversation indexes lead on status/customer/last_activity_at, none on created_at.
  CREATE INDEX conversations_tenant_created_idx
      ON conversations (tenant_id, created_at)
      WHERE deleted_at IS NULL;

  -- Reverse join for attributing ai_usage_records to a conversation channel.
  -- Existing ai_generations index leads on conversation_id, not usage_record_id.
  CREATE INDEX ai_generations_tenant_usage_record_idx
      ON ai_generations (tenant_id, usage_record_id)
      WHERE usage_record_id IS NOT NULL;
  ```

  **Done when** the file exists and `psql`-applying the migration set succeeds (or `cargo test` with `DATABASE_URL` set runs migrations without error).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared module skeleton, shared query-parameter handling, RBAC correction, the canonical test seed, and shared frontend plumbing. **No user story can start until this phase is done.**

- [X] T004 Remove analytics access from the Viewer role in `backend/crates/modules/authz/src/matrix.rs`. In the `const TENANT_VIEWER: &[Permission]` array (around line 48) delete the single line `Permission::AnalyticsView,`. Change nothing else — `TENANT_ADMIN`, `TENANT_MANAGER`, `Permission::TENANT` (used by Owner), and every `STAFF_PRODUCTION_*` array keep their existing entries. Reason: spec clarification restricts analytics to Owner/Admin/Manager. **Done when** `cd backend && cargo check -p authz` succeeds and `TENANT_VIEWER` no longer contains `AnalyticsView`.

- [X] T005 Update the RBAC expectations in `backend/crates/server/tests/rbac.rs` to match T004. In `const VIEWER_PERMISSIONS: &[&str]` (around line 41) delete the line `"analytics.view",`. Do **not** change `TENANT_PERMISSIONS` or any of the `production_expected` entries (developer/sales/finance keep `"analytics.view"`). **Done when** `cd backend && cargo test --test rbac` passes (tests skip without `DATABASE_URL`; then at minimum `cargo test --test rbac --no-run` must compile).

- [X] T006 Replace the placeholder `backend/crates/modules/analytics/src/lib.rs` (currently one doc comment line) with a module doc block plus module declarations. Follow the exact doc-section style of `backend/crates/modules/feedback/src/lib.rs`: `# Analytics Module` heading then `## Purpose`, `## Responsibilities`, `## Public Interfaces`, `## Dependencies`, `## Data Model`, `## Extension Points` sections. Public Interfaces section must list `GET /tenant/analytics/summary` and `GET /tenant/analytics/timeseries`. Data Model section must state that this module owns no tables and reads `conversations`, `messages`, `escalations`, `conversation_feedback`, `ai_usage_records`, `ai_generations`. End the file with exactly:

  ```rust
  pub mod model;
  pub mod queries;
  pub mod routes;
  ```

  **Done when** the file ends with those three `pub mod` lines. (It will not compile until T007–T009 create those files — that is expected.)

- [X] T007 Create `backend/crates/modules/analytics/src/model.rs` with the shared query and range types. Write exactly these items (imports: `chrono::{DateTime, NaiveDate, Utc}`, `serde::{Deserialize, Serialize}`, `utoipa::ToSchema`):

  ```rust
  /// Channel values permitted by the conversations_channel_check constraint.
  pub const ALLOWED_CHANNELS: [&str; 6] =
      ["email", "phone", "web_chat", "whatsapp", "telegram", "widget"];

  pub const MAX_RANGE_DAYS: i64 = 366;
  pub const DEFAULT_RANGE_DAYS: i64 = 30;

  /// Raw query string parameters for both analytics endpoints.
  #[derive(Debug, Deserialize, ToSchema)]
  pub struct AnalyticsQuery {
      pub from: Option<String>,
      pub to: Option<String>,
      pub channel: Option<String>,
  }

  /// Validated, resolved query parameters.
  #[derive(Debug, Clone)]
  pub struct ResolvedQuery {
      pub from_date: NaiveDate,
      pub to_date: NaiveDate,
      pub from_ts: DateTime<Utc>,
      /// Exclusive upper bound: to_date + 1 day at 00:00:00Z.
      pub to_ts: DateTime<Utc>,
      pub channel: Option<String>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
  pub struct DateRangeDto {
      pub from: NaiveDate,
      pub to: NaiveDate,
  }
  ```

  Then implement `pub fn resolve_query(query: AnalyticsQuery, today: NaiveDate) -> Result<ResolvedQuery, String>` with these rules, returning the error message string on failure:
  - `to` absent → `today`. `from` absent → `to_date - (DEFAULT_RANGE_DAYS - 1)` days (a 30-day inclusive window ending at `to`).
  - Parse dates with `NaiveDate::parse_from_str(value, "%Y-%m-%d")`; on parse failure return `Err("Invalid date format, expected YYYY-MM-DD".into())`.
  - If `from_date > to_date` return `Err("from must be on or before to".into())`.
  - If `(to_date - from_date).num_days() + 1 > MAX_RANGE_DAYS` return `Err("Date range must not exceed 366 days".into())`.
  - If `channel` is `Some(c)` and `!ALLOWED_CHANNELS.contains(&c.as_str())` return `Err("Unknown channel".into())`.
  - Build `from_ts` as `from_date.and_hms_opt(0,0,0)` interpreted as UTC, and `to_ts` as `(to_date + chrono::Duration::days(1)).and_hms_opt(0,0,0)` as UTC.

  Add `#[cfg(test)] mod tests` covering: default range is 30 inclusive days ending today; explicit valid range round-trips; `from > to` errors; 367-day range errors; bad channel errors; bad date string errors. **Done when** `cd backend && cargo test -p analytics` passes.

- [X] T008 Create `backend/crates/modules/analytics/src/queries.rs` containing only the file header and imports for now: `use chrono::{DateTime, NaiveDate, Utc};`, `use sqlx::PgPool;`, `use uuid::Uuid;`. Add a doc comment at the top stating: every query in this file must filter `tenant_id = $1` and must exclude `deleted_at IS NOT NULL` conversations. Later tasks append functions to this file. **Done when** the file exists and `cargo check -p analytics` succeeds.

- [X] T009 Create `backend/crates/modules/analytics/src/routes.rs` containing only imports and no handlers yet: `use axum::extract::{Query, State};`, `use axum::http::StatusCode;`, `use axum::response::{IntoResponse, Response};`, `use axum::Json;`, `use kernel::ApiError;`, `use sqlx::PgPool;`, `use crate::model;`, `use crate::queries;`. Later tasks add the two handlers. Suppress unused-import warnings only if the compiler complains. **Done when** `cd backend && cargo check -p analytics` succeeds.

- [X] T010 Create the integration-test harness file `backend/crates/server/tests/analytics_api.rs`. Copy these helpers **verbatim** from `backend/crates/server/tests/feedback_api.rs` along with their `use` statements (they live between lines 1 and 235 of that file):
  - `TEST_ENV`, `test_config()`, `app_state()`, `require_db_tests()`, `get_pool()` — these make the suite skip when `DATABASE_URL` is unset, exactly like the existing suites
  - `async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response` (line 95) and `async fn body_json(response: Response) -> serde_json::Value` (line 102)
  - `fn authenticated_request(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body>` (line 205) — this is the helper every analytics test uses to make a tenant-scoped authenticated GET; it sets the `X-Dev-User-Id` and `X-Tenant-ID` headers. **Do not use `authed_get`/`Bearer` tokens — those are for widget session routes, not tenant routes.**
  - `async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid` (line 215)

  Then add **one new helper** that `feedback_api.rs` does not have (its `seed_admin` hardcodes the `admin` role, but analytics tests must vary the role):

  ```rust
  /// Seed a user with an active tenant membership in the given role.
  /// `role` must be one of: owner, admin, manager, agent, viewer.
  async fn seed_member(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str, role: &str) -> Uuid {
      let user_id = seed_user(pool, email).await;
      sqlx::query(
          "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
           VALUES ($1, $2, $3, 'active')",
      )
      .bind(tenant_id)
      .bind(user_id)
      .bind(role)
      .execute(pool)
      .await
      .unwrap();
      user_id
  }
  ```

  Then add a `seed_canonical_dataset(pool: &sqlx::PgPool) -> (Uuid, Uuid)` async function that returns `(tenant_a_id, tenant_b_id)` and inserts **exactly** the dataset below with `sqlx::query(...)` calls. Generate fresh UUIDs for tenants/customers/conversations; insert tenants and customers first (`customers` requires `tenant_id` and `display_name`). Use unique email addresses per test to avoid collisions on the users table.

  **Tenant A conversations** (all `channel='widget'` unless stated; `customer_id` = tenant A's customer):
  | Ref | created_at | channel | status | deleted_at | escalation row? |
  |-----|-----------|---------|--------|-----------|-----------------|
  | C1 | 2026-03-10T09:00:00Z | widget | closed | NULL | no |
  | C2 | 2026-03-10T10:00:00Z | widget | resolved | NULL | no |
  | C3 | 2026-03-11T09:00:00Z | widget | closed | NULL | **yes** |
  | C4 | 2026-03-11T10:00:00Z | email | open | NULL | no |
  | C5 | 2026-03-12T09:00:00Z | widget | closed | **2026-03-12T12:00:00Z** | no |

  For C3 insert one `escalations` row with `tenant_id`, `conversation_id`, `reason='test'`, `status='closed'`, `closed_at=now()`, and every assignment column left NULL. This is valid: `escalations_consistency_check` in migration 0037 ends with a bare `OR (status = 'closed')` branch that imposes no assignment requirements. Handoff detection is pure row-existence, so the escalation's status never affects a metric.

  **Tenant A messages** (all replies use `kind='ai'` so no membership rows are needed; `body='x'`):
  | conversation | kind | created_at |
  |--------------|------|-----------|
  | C1 | customer | 2026-03-10T09:00:00Z |
  | C1 | ai | 2026-03-10T09:00:20Z |
  | C2 | customer | 2026-03-10T10:00:00Z |
  | C2 | ai | 2026-03-10T10:01:00Z |
  | C2 | customer | 2026-03-10T10:02:00Z |
  | C2 | ai | 2026-03-10T10:02:30Z |
  | C3 | customer | 2026-03-11T09:00:00Z |
  | C3 | ai | 2026-03-11T09:00:40Z |
  | C4 | customer | 2026-03-11T10:00:00Z |

  **Tenant A feedback** (`conversation_feedback`): C1 rating 5 submitted 2026-03-10T12:00:00Z channel widget; C2 rating 3 submitted 2026-03-11T12:00:00Z channel widget; C3 rating 4 submitted **2026-03-16T12:00:00Z** channel widget (deliberately outside the test range).

  **Tenant A usage** (`ai_usage_records`, `provider='openai'`, `model='gpt-4o'`, `status='success'`, `streamed=false`, `latency_ms=10`): U1 created 2026-03-10T09:00:10Z input 100 output 50; U2 created 2026-03-11T10:00:10Z input 200 output NULL; U3 created 2026-03-11T11:00:00Z input 10 output 5.

  **Tenant A generations** (`ai_generations`, `outcome='success'`, `latency_ms=10`, `trigger_message_id` = that conversation's customer message id): G1 → conversation C1, `usage_record_id`=U1; G2 → conversation C4, `usage_record_id`=U2. **U3 gets no generation row** (it is the unattributed-token case).

  **Tenant B** (isolation control): one customer, one conversation created 2026-03-10T09:00:00Z channel widget status closed (no escalation), one feedback rating 1 submitted 2026-03-10T12:00:00Z, one usage record created 2026-03-10T09:00:00Z input 999 output 0.

  **Done when** the file compiles (`cd backend && cargo test --test analytics_api --no-run`) and `seed_canonical_dataset` runs without a DB constraint error when `DATABASE_URL` is set.

- [X] T011 [P] Add validated chart series colors to `frontend/apps/dashboard/src/app/design-system/tokens/tokens.css` (this file holds theme-independent values; these two colors are deliberately identical in light and dark because both passed accessibility validation against both surfaces). Add to the existing `:root` block:

  ```css
  /* Chart series colors (spec 025). Fixed order: series 1 then series 2 — never
     reassigned by rank, so a filter that drops a series never repaints the others.
     Validated for colorblind separation and >=3:1 contrast against both the light
     (#ffffff) and dark (#13161d) card surfaces. */
  --app-chart-1: #0284c7;
  --app-chart-2: #d97706;
  ```

  Do not touch `frontend/apps/dashboard/src/app/design-system/theme/themes.css`. **Done when** the tokens exist and `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T012 [P] Add the analytics API models to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`. Append (do not modify existing exports) snake_case wire interfaces exactly matching `specs/025-analytics-foundation/contracts/analytics-api.md`, camelCase domain interfaces, and two mapper functions. Follow the naming/mapper style already used by `feedbackSummaryFromWire` in this same file. Required exports:

  ```ts
  export interface AnalyticsSummaryWire {
    range: { from: string; to: string };
    channel: string | null;
    conversation_volume: number;
    concluded_count: number;
    ai_resolution_rate: number | null;
    handoff_rate: number | null;
    avg_first_response_seconds: number | null;
    avg_response_seconds: number | null;
    satisfaction_avg: number | null;
    satisfaction_count: number;
    total_tokens: number;
    unattributed_tokens: number;
    channels: { channel: string; conversation_count: number; share: number }[];
  }
  export interface AnalyticsSummary { /* same fields, camelCase: range {from,to}, channel, conversationVolume, concludedCount, aiResolutionRate, handoffRate, avgFirstResponseSeconds, avgResponseSeconds, satisfactionAvg, satisfactionCount, totalTokens, unattributedTokens, channels: AnalyticsChannelShare[] */ }
  export interface AnalyticsChannelShare { channel: string; conversationCount: number; share: number }
  export interface AnalyticsTimeseriesWire {
    range: { from: string; to: string };
    channel: string | null;
    days: {
      date: string; conversation_volume: number; ai_resolved: number;
      handed_off: number; satisfaction_avg: number | null;
      satisfaction_count: number; total_tokens: number;
    }[];
  }
  export interface AnalyticsTimeseriesDay { date: string; conversationVolume: number; aiResolved: number; handedOff: number; satisfactionAvg: number | null; satisfactionCount: number; totalTokens: number }
  export interface AnalyticsTimeseries { range: { from: string; to: string }; channel: string | null; days: AnalyticsTimeseriesDay[] }
  export function analyticsSummaryFromWire(wire: AnalyticsSummaryWire): AnalyticsSummary
  export function analyticsTimeseriesFromWire(wire: AnalyticsTimeseriesWire): AnalyticsTimeseries
  ```

  Preserve `null` as `null` in mappers — never convert a null rate to `0`, because null means "no data" and renders differently. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T013 Create `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics-api.service.ts`. Copy the injection/`HttpParams` style from `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts`. Content: an `@Injectable({ providedIn: 'root' })` class `AnalyticsApiService` injecting `ApiService` from `../../../core/api/api.service`, with two methods:

  ```ts
  getSummary(query: { from?: string; to?: string; channel?: string | null }): Observable<ApiResponse<AnalyticsSummary>>
  getTimeseries(query: { from?: string; to?: string; channel?: string | null }): Observable<ApiResponse<AnalyticsTimeseries>>
  ```

  Each builds `HttpParams` setting `from`, `to`, and `channel` **only when the value is a non-empty string** (skip `undefined`/`null`), calls `this.api.get<...Wire>('/tenant/analytics/summary', params)` (or `/tenant/analytics/timeseries`), and pipes `map(({ data, ...rest }) => ({ ...rest, data: analyticsSummaryFromWire(data) }))` (respectively `analyticsTimeseriesFromWire`). Use RxJS operators only — no `async`/`await`, no Promises (constitution: RxJS-first). **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

**Checkpoint**: Module skeleton, RBAC fix, seed harness, and shared frontend plumbing are ready — user stories can begin.

---

## Phase 3: User Story 1 - View key metrics at a glance (Priority: P1) 🎯 MVP

**Goal**: A tenant Admin opens Analytics and sees real headline metric cards (volume, AI resolution rate, handoff rate, first response time, satisfaction, token usage) for a default 30-day window, scoped to their tenant.

**Independent Test**: With the canonical seed loaded, `GET /api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12` as tenant A returns volume 4, resolution rate ≈0.667, handoff rate 0.25; the same call as tenant B returns volume 1; the dashboard page renders those numbers as cards.

### Tests for User Story 1 (write first, expect them to fail)

- [X] T014 [P] [US1] In `backend/crates/server/tests/analytics_api.rs`, add `summary_returns_expected_metrics_for_seeded_tenant`. Start with the standard preamble used by every test in this file: `let Some(pool) = get_pool().await else { return };` then `db::run_migrations(&pool).await.unwrap();` then `let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;`. Create the caller with `let user = seed_member(&pool, tenant_a, "admin@analytics-t014.test", "admin").await;` and issue the request with `send(pool.clone(), authenticated_request("/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12", user, tenant_a)).await`. Read the body with `body_json(response).await`. Assert status 200 and these **exact** values: `conversation_volume == 4`, `concluded_count == 3`, `ai_resolution_rate` ≈ `0.6666` (assert `(v - 2.0/3.0).abs() < 0.001`), `handoff_rate == 0.25`, `avg_first_response_seconds == 40.0` (C1 20s + C2 60s + C3 40s over 3 conversations), `avg_response_seconds == 37.5` (pairs 20, 60, 30, 40), `satisfaction_avg == 4.0`, `satisfaction_count == 2` (the rating submitted 2026-03-16 is outside the range), `total_tokens == 365` (150 + 200 + 15), `unattributed_tokens == 15` (U3 has no generation row).

- [X] T015 [P] [US1] In `backend/crates/server/tests/analytics_api.rs`, add `summary_is_tenant_isolated`. Using the same canonical seed, request the same date range as a tenant **B** admin and assert `conversation_volume == 1`, `satisfaction_count == 1`, `satisfaction_avg == 1.0`, `total_tokens == 999` — proving none of tenant A's rows leak. Then re-request as tenant A and assert `conversation_volume == 4` is unchanged.

- [X] T016 [P] [US1] In `backend/crates/server/tests/analytics_api.rs`, add `summary_enforces_rbac`. Seed the canonical dataset, then for each of the five tenant roles create a separate user with `seed_member(&pool, tenant_a, "<role>@analytics-t016.test", "<role>").await` and request `/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12` via `authenticated_request(uri, user, tenant_a)`. Assert: `owner` → 200, `admin` → 200, `manager` → 200, `agent` → 403, `viewer` → 403. (Viewer must be 403 because T004 removed `analytics.view` from the Viewer matrix — if Viewer returns 200, T004 was not applied.) Finally assert that a request built **without** the `X-Dev-User-Id` header returns 401: build it with `Request::builder().uri(uri).method(Method::GET).header("X-Tenant-ID", tenant_a.to_string()).body(Body::empty()).unwrap()`.

- [X] T017 [P] [US1] In `backend/crates/server/tests/analytics_api.rs`, add `summary_empty_range_returns_zeroes_not_error`. Request `from=2026-01-01&to=2026-01-07` (no seeded activity) as tenant A and assert status 200 with `conversation_volume == 0`, `concluded_count == 0`, and `ai_resolution_rate`, `handoff_rate`, `avg_first_response_seconds`, `satisfaction_avg` all **`null`** (JSON null, not 0) — empty denominators are a no-data state per FR-012.

### Implementation for User Story 1

- [X] T018 [US1] Append the summary response DTOs to `backend/crates/modules/analytics/src/model.rs`. Add, each deriving `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]`:

  ```rust
  pub struct ChannelBreakdownItem {
      pub channel: String,
      pub conversation_count: i64,
      pub share: f64,
  }

  pub struct AnalyticsSummaryDto {
      pub range: DateRangeDto,
      pub channel: Option<String>,
      pub conversation_volume: i64,
      pub concluded_count: i64,
      pub ai_resolution_rate: Option<f64>,
      pub handoff_rate: Option<f64>,
      pub avg_first_response_seconds: Option<f64>,
      pub avg_response_seconds: Option<f64>,
      pub satisfaction_avg: Option<f64>,
      pub satisfaction_count: i64,
      pub total_tokens: i64,
      pub unattributed_tokens: i64,
      pub channels: Vec<ChannelBreakdownItem>,
  }

  pub struct AnalyticsSummaryResponse { pub data: AnalyticsSummaryDto }
  ```

  Use snake_case field names with **no** `#[serde(rename_all)]` attribute — the contract's JSON is snake_case. **Done when** `cargo check -p analytics` succeeds.

- [X] T019 [US1] Append `conversation_counts` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn conversation_counts(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: Option<&str>) -> sqlx::Result<(i64, i64, i64, i64)>` returning `(volume, concluded, ai_resolved, handed_off)`. Use `sqlx::query_as::<_, (i64, i64, i64, i64)>` with this exact SQL, binding `$1`=tenant_id, `$2`=from_ts, `$3`=to_ts, `$4`=channel:

  ```sql
  SELECT
      COUNT(*)::bigint,
      COUNT(*) FILTER (WHERE c.status IN ('resolved','closed'))::bigint,
      COUNT(*) FILTER (WHERE c.status IN ('resolved','closed') AND NOT esc.escalated)::bigint,
      COUNT(*) FILTER (WHERE esc.escalated)::bigint
  FROM conversations c
  CROSS JOIN LATERAL (
      SELECT EXISTS (
          SELECT 1 FROM escalations es
          WHERE es.tenant_id = c.tenant_id AND es.conversation_id = c.id
      ) AS escalated
  ) esc
  WHERE c.tenant_id = $1
    AND c.deleted_at IS NULL
    AND c.created_at >= $2
    AND c.created_at < $3
    AND ($4::text IS NULL OR c.channel = $4)
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T020 [US1] Append `avg_first_response_seconds` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn avg_first_response_seconds(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: Option<&str>) -> sqlx::Result<Option<f64>>`. Use `sqlx::query_scalar::<_, Option<f64>>(...).fetch_one(pool)` with this exact SQL (same `$1..$4` binding order as T019):

  ```sql
  WITH cohort AS (
      SELECT c.id FROM conversations c
      WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
        AND c.created_at >= $2 AND c.created_at < $3
        AND ($4::text IS NULL OR c.channel = $4)
  ),
  first_customer AS (
      SELECT m.conversation_id, MIN(m.created_at) AS asked_at
      FROM messages m JOIN cohort ON cohort.id = m.conversation_id
      WHERE m.tenant_id = $1 AND m.kind = 'customer'
      GROUP BY m.conversation_id
  ),
  first_reply AS (
      SELECT fc.conversation_id, MIN(m.created_at) AS replied_at
      FROM first_customer fc
      JOIN messages m ON m.conversation_id = fc.conversation_id AND m.tenant_id = $1
      WHERE m.kind IN ('reply','ai') AND m.created_at > fc.asked_at
      GROUP BY fc.conversation_id
  )
  SELECT AVG(EXTRACT(EPOCH FROM (fr.replied_at - fc.asked_at)))::float8
  FROM first_customer fc
  JOIN first_reply fr ON fr.conversation_id = fc.conversation_id
  ```

  Conversations with no reply contribute nothing (they are absent from `first_reply`); when no conversation has a reply the query yields `NULL`, which must surface as `None`. **Done when** `cargo check -p analytics` succeeds.

- [X] T021 [US1] Append `avg_response_seconds` (the secondary all-pairs metric) to `backend/crates/modules/analytics/src/queries.rs`. Signature mirrors T020: `pub async fn avg_response_seconds(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: Option<&str>) -> sqlx::Result<Option<f64>>`. Exact SQL (`$1..$4` as before):

  ```sql
  WITH cohort AS (
      SELECT c.id FROM conversations c
      WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
        AND c.created_at >= $2 AND c.created_at < $3
        AND ($4::text IS NULL OR c.channel = $4)
  )
  SELECT AVG(EXTRACT(EPOCH FROM (r.replied_at - m.created_at)))::float8
  FROM messages m
  JOIN cohort ON cohort.id = m.conversation_id
  CROSS JOIN LATERAL (
      SELECT MIN(m2.created_at) AS replied_at
      FROM messages m2
      WHERE m2.tenant_id = m.tenant_id
        AND m2.conversation_id = m.conversation_id
        AND m2.kind IN ('reply','ai')
        AND m2.created_at > m.created_at
  ) r
  WHERE m.tenant_id = $1 AND m.kind = 'customer' AND r.replied_at IS NOT NULL
  ```

  This pairs **every** customer message with the first reply after it (so two customer messages before one reply both count). **Done when** `cargo check -p analytics` succeeds.

- [X] T022 [US1] Append `satisfaction` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn satisfaction(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: Option<&str>) -> sqlx::Result<(Option<f64>, i64)>`. Exact SQL — note it filters on `submitted_at`, not conversation creation date:

  ```sql
  SELECT AVG(f.rating)::float8, COUNT(*)::bigint
  FROM conversation_feedback f
  WHERE f.tenant_id = $1
    AND f.submitted_at >= $2
    AND f.submitted_at < $3
    AND ($4::text IS NULL OR f.channel = $4)
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T023 [US1] Append `token_totals` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn token_totals(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>) -> sqlx::Result<(i64, i64)>` returning `(total_tokens, unattributed_tokens)`. This is the **no-channel-filter** path; the channel-filtered variant is added later by US4. All rows count regardless of `status`; both token columns are nullable. Exact SQL (the `LEFT JOIN LATERAL ... LIMIT 1` prevents double-counting a usage record that has more than one generation row):

  ```sql
  SELECT
      COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint,
      COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0))
               FILTER (WHERE g.id IS NULL), 0)::bigint
  FROM ai_usage_records u
  LEFT JOIN LATERAL (
      SELECT gg.id FROM ai_generations gg
      WHERE gg.tenant_id = u.tenant_id AND gg.usage_record_id = u.id
      LIMIT 1
  ) g ON TRUE
  WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T024 [US1] Add the summary handler to `backend/crates/modules/analytics/src/routes.rs`. Copy the handler shape (State extractor, `tenancy::TenantContext`, `ApiError::internal_error` on DB failure, `tracing::error!`) from `backend/crates/modules/feedback/src/tenant_routes.rs`. Write:

  ```rust
  #[utoipa::path(
      get,
      path = "/tenant/analytics/summary",
      tag = "analytics",
      operation_id = "get_analytics_summary",
      summary = "Tenant analytics headline metrics",
      params(
          ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
          ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
          ("channel" = Option<String>, Query, description = "Optional channel filter"),
      ),
      responses(
          (status = 200, description = "Analytics summary.", body = model::AnalyticsSummaryDto),
          (status = 422, description = "Invalid query parameters."),
      ),
  )]
  pub async fn get_analytics_summary(
      State(pool): State<PgPool>,
      ctx: tenancy::TenantContext,
      Query(query): Query<model::AnalyticsQuery>,
  ) -> Response
  ```

  Body: call `model::resolve_query(query, chrono::Utc::now().date_naive())`; on `Err(message)` return `ApiError::validation_error(message).into_response()` with HTTP 422 (use whichever `ApiError` constructor in `backend/crates/shared/kernel` produces 422 — check the enum and use the existing 422 variant rather than inventing one). On success call, in order: `queries::conversation_counts`, `queries::avg_first_response_seconds`, `queries::avg_response_seconds`, `queries::satisfaction`, `queries::token_totals`. Compute in Rust (never divide in SQL): `ai_resolution_rate = if concluded == 0 { None } else { Some(ai_resolved as f64 / concluded as f64) }` and `handoff_rate = if volume == 0 { None } else { Some(handed_off as f64 / volume as f64) }`. Set `channels: Vec::new()` for now (US4 fills it). Round `satisfaction_avg` to one decimal the same way `feedback/src/tenant_routes.rs` does: `v.map(|v| (v * 10.0).round() / 10.0)`. Return `(StatusCode::OK, Json(dto)).into_response()`. **Done when** `cargo check -p analytics` succeeds.

- [X] T025 [US1] Wire the summary route in `backend/crates/server/src/router.rs`. Inside `fn tenant_routes(...)`, immediately after the existing block that registers `feedback::tenant_routes::get_feedback_summary` (search for the comment `// T040: Feedback summary`), add:

  ```rust
  // Analytics summary (spec 025)
  .routes(
      routes!(analytics::routes::get_analytics_summary)
          .layer(require_permission(Permission::AnalyticsView)),
  )
  ```

  **Done when** `cd backend && cargo check -p server` succeeds.

- [X] T026 [US1] Register the analytics schemas in `backend/crates/server/src/openapi.rs`. In the `components(schemas(...))` list, after the block of `feedback::model::*` entries (around line 149), add:

  ```rust
  // Analytics
  analytics::model::DateRangeDto,
  analytics::model::ChannelBreakdownItem,
  analytics::model::AnalyticsSummaryDto,
  ```

  **Done when** `cd backend && cargo test --test openapi_valid --test openapi_contract --test openapi_coverage` passes (or compiles if no `DATABASE_URL`).

- [X] T027 [US1] Create the store `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.ts`. Copy the NgRx SignalStore structure (`signalStore`, `withState`, `withMethods`, `withHooks`, `rxMethod`, `patchState`, tenant-change effect) from `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.store.ts`. State interface:

  ```ts
  interface AnalyticsState {
    readonly from: string;   // 'YYYY-MM-DD'
    readonly to: string;     // 'YYYY-MM-DD'
    readonly channel: string | null;
    readonly summary: AnalyticsSummary | null;
    readonly timeseries: AnalyticsTimeseries | null;
    readonly loading: boolean;
    readonly error: string | null;
  }
  ```

  Initial `from`/`to` = a 30-day inclusive window ending today, computed with plain `Date` arithmetic and formatted `YYYY-MM-DD`. Expose an `rxMethod` `loadSummary` that sets `loading: true, error: null`, calls `AnalyticsApiService.getSummary({ from, to, channel })`, and on success patches `summary` and `loading: false`; on `catchError` patches `error` to the caught message and `loading: false`. Expose a public method `load(): void` that triggers it. Add a `withHooks` `onInit` that calls `load()` when the active tenant id is set (copy the `selectActiveTenant` effect pattern from `conversations.store.ts`). Leave `timeseries` always `null` here — US3 adds its loader. RxJS operators only, no Promises. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T028 [US1] Make delta and trend optional on the shared metric card so it can display real metrics that have no comparison period. Edit `frontend/apps/dashboard/src/app/shared/components/metric-card/metric-card.component.ts`:
  1. Export a new interface in this file: `export interface MetricCardData { id: string; label: string; value: string; icon: string; delta?: string; deltaPositive?: boolean; trend?: readonly number[]; }`.
  2. Change the input to `readonly metric = input.required<MetricCardData>();` (the existing `MetricFixture` objects used by the overview page satisfy this shape, so `overview.component.ts` keeps working unchanged — do not edit it).
  3. Wrap the delta span in `@if (metric().delta) { ... }` and the `<app-sparkline>` in `@if (metric().trend?.length) { ... }`, passing `[points]="metric().trend ?? []"`.
  4. Keep `deltaClass()` and `sparkColor()` working when `deltaPositive` is undefined (treat undefined as not-positive).

  **Done when** `cd frontend && pnpm ng build dashboard && pnpm ng test dashboard` both succeed (the existing overview tests must still pass).

- [X] T029 [US1] Rewrite `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.ts` to render real metric cards. Remove the `RoutedPageStore`/`PAGE_ROUTE` providers, the `routed-page.store` import, and all fixture-driven markup (the `topArticles` table and the fixture `charts` loop go away). Keep using `PageContainerComponent`, `PageHeaderComponent`, `LoadingStateComponent`, `EmptyStateComponent`, `MetricCardComponent`, `ToolbarComponent`. Provide `AnalyticsStore` in the component `providers` array and inject it. Render: loading state while `store.loading()`; the existing error empty-state when `store.error()`; otherwise a `<section class="metrics">` with one `<app-metric-card>` per metric built by a `computed()` that maps `store.summary()` into `MetricCardData[]`:
  - Conversations — `value` = `conversationVolume` as a plain integer string, `icon` `'@tui.message-square'`
  - AI resolution rate — `aiResolutionRate` formatted as a percentage with one decimal (e.g. `66.7%`), `icon` `'@tui.bot'`
  - Human handoffs — `handoffRate` as a percentage with one decimal, `icon` `'@tui.user-round'`
  - Avg first response — `avgFirstResponseSeconds` formatted as `Xs` under 60s else `Xm Ys`, `icon` `'@tui.timer'`
  - Avg response (all messages) — `avgResponseSeconds` with the same `Xs` / `Xm Ys` formatting, `icon` `'@tui.clock'`. This is the secondary all-pairs metric required by FR-005; it must be visible in the UI, not just returned by the API.
  - Satisfaction — `satisfactionAvg` as `4.0 / 5` plus `satisfactionCount` ratings, `icon` `'@tui.star'`
  - Tokens used — `totalTokens` with thousands separators via `toLocaleString()`, `icon` `'@tui.zap'`

  That is **seven** cards in total.

  **Any `null` metric must render the literal string `—` (em dash), never `0` or `0%`** — null means no data (FR-012). Do not set `delta` or `trend` on these cards. When `store.summary()` is null and not loading, show the existing `app-empty-state` with icon `'@tui.chart-line'`. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T030 [US1] Replace `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.spec.ts` (it currently mocks `RoutedPageDataService`, which the component no longer uses). Keep the existing harness style: `provideZonelessChangeDetection()`, `provideTaiga()`, `vi.fn()` mocks, `TestBed.configureTestingModule`. Provide a stub `AnalyticsApiService` whose `getSummary` returns `of({ data: <AnalyticsSummary> })`. Assert: (a) **seven** `app-metric-card` elements render for a summary with all values present, and both response-time cards (first response and all-messages) are among them; (b) a summary with `aiResolutionRate: null` and `satisfactionAvg: null` renders `—` for those cards and does not render `0%`; (c) when `getSummary` returns `throwError(() => new Error('boom'))` the error empty-state renders. **Done when** `cd frontend && pnpm ng test dashboard` passes.

- [X] T031 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.spec.ts`. Using `TestBed` with `provideZonelessChangeDetection()` and a stubbed `AnalyticsApiService`, assert: (a) the initial `from`/`to` span exactly 30 inclusive days ending today; (b) `load()` calls `getSummary` with the current `from`, `to`, and `channel` and patches `summary`; (c) an API error patches `error` and leaves `loading` false. **Done when** `cd frontend && pnpm ng test dashboard` passes.

**Checkpoint**: US1 complete — real headline metrics render, tenant-scoped and RBAC-guarded. This is the MVP.

---

## Phase 4: User Story 2 - Filter analytics by date range (Priority: P2)

**Goal**: The admin narrows the window with presets (7/30/90 days) or a custom range, and every metric recomputes.

**Independent Test**: With the canonical seed, `?from=2026-03-10&to=2026-03-10` returns volume 2 while `?from=2026-03-10&to=2026-03-12` returns volume 4; the page's date controls drive the same change.

### Tests for User Story 2 (write first)

- [X] T032 [P] [US2] In `backend/crates/server/tests/analytics_api.rs`, add `summary_respects_date_range`. With the canonical seed and tenant A assert: `?from=2026-03-10&to=2026-03-10` → `conversation_volume == 2`, `satisfaction_count == 1`, `satisfaction_avg == 5.0`, `total_tokens == 150`; `?from=2026-03-11&to=2026-03-11` → `conversation_volume == 2`, `total_tokens == 215`; `?from=2026-03-10&to=2026-03-12` → `conversation_volume == 4`. This proves both bounds are inclusive of whole UTC days.

- [X] T033 [P] [US2] In `backend/crates/server/tests/analytics_api.rs`, add `summary_rejects_invalid_ranges`. As a tenant-A admin assert HTTP **422** for each of: `?from=2026-03-12&to=2026-03-10` (from after to), `?from=2025-01-01&to=2026-12-31` (longer than 366 days), `?from=notadate&to=2026-03-10` (unparseable), `?channel=carrier-pigeon` (unknown channel). Also assert that omitting `from` and `to` entirely returns 200 (the default 30-day window).

### Implementation for User Story 2

- [X] T034 [US2] Verify and, if needed, correct the 422 path in `backend/crates/modules/analytics/src/routes.rs`. The handler added in T024 must map every `Err` from `model::resolve_query` to an HTTP 422 response carrying the error message in the standard error envelope. Open `backend/crates/shared/kernel` and use the existing validation/unprocessable constructor on `ApiError` (do not add a new variant, and do not return 400). **Done when** `cd backend && cargo test --test analytics_api summary_rejects_invalid_ranges` passes with a live `DATABASE_URL`.

- [X] T035 [US2] Add date-range controls to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.ts`. Inside the existing `<app-toolbar>`, add an `<app-select-filter>` (import from `../../../shared/components/select-filter/select-filter.component`) with `label="Date range"` and options `[{value:'7',label:'Last 7 days'},{value:'30',label:'Last 30 days'},{value:'90',label:'Last 90 days'},{value:'custom',label:'Custom range'}]`. On `(valueChange)` call a new store method `setPreset(days: '7'|'30'|'90'|'custom')`. When `custom` is selected, reveal two native `<input type="date">` controls bound to `store.from()` and `store.to()`, each with an `aria-label` (`Start date` / `End date`), whose `change` handlers call `store.setCustomRange(from, to)`. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T036 [US2] Add the range methods to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.ts`. Add to the `withMethods` return object:
  - `setPreset(days: '7'|'30'|'90'|'custom'): void` — for a numeric preset, patch `from` to `today - (n - 1)` days and `to` to today (both `YYYY-MM-DD`), then trigger the same load used by `load()`. For `'custom'`, leave the current dates untouched and do not reload (the user picks dates next).
  - `setCustomRange(from: string, to: string): void` — patch both dates then reload. If `from > to` (plain string comparison works for `YYYY-MM-DD`), patch `error` to `'Start date must be on or before end date'` and skip the request.
  Also add a `preset` field to the state (initial value `'30'`) so the select reflects the active choice. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T037 [US2] Extend `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.spec.ts` with date-filter cases: (a) `setPreset('7')` produces a 7-day inclusive window ending today and calls `getSummary` again; (b) `setCustomRange('2026-03-10','2026-03-12')` sends exactly those dates; (c) `setCustomRange('2026-03-12','2026-03-10')` sets `error` and issues **no** API call; (d) **a single `setPreset` call issues exactly one `getSummary` call and one `getTimeseries` call** — assert the call counts on the stubs. Case (d) guards SC-006 (filters update everything within 2 s) by proving a filter change cannot fan out into duplicate or repeated requests. **Done when** `cd frontend && pnpm ng test dashboard` passes.

**Checkpoint**: US1 + US2 work — metrics respond to date filtering, invalid ranges are rejected with 422.

---

## Phase 5: User Story 3 - See trends over time in charts (Priority: P3)

**Goal**: Daily time-series charts for volume, AI-resolved vs handed-off, satisfaction, and token usage.

**Independent Test**: `GET /tenant/analytics/timeseries?from=2026-03-10&to=2026-03-12` returns exactly 3 day entries with the seeded per-day values and a zero-filled third day; the page renders four chart cards.

### Tests for User Story 3 (write first)

- [X] T038 [P] [US3] In `backend/crates/server/tests/analytics_api.rs`, add `timeseries_returns_one_zero_filled_entry_per_day`. With the canonical seed request `/api/v1/tenant/analytics/timeseries?from=2026-03-10&to=2026-03-12` as a tenant-A admin. Assert `days.len() == 3` **exactly** (a 4th entry means the `generate_series` upper bound is wrong) and that entries are ascending by date with these values:
  - `2026-03-10`: `conversation_volume == 2`, `ai_resolved == 2`, `handed_off == 0`, `satisfaction_avg == 5.0`, `satisfaction_count == 1`, `total_tokens == 150`
  - `2026-03-11`: `conversation_volume == 2`, `ai_resolved == 0`, `handed_off == 1`, `satisfaction_avg == 3.0`, `satisfaction_count == 1`, `total_tokens == 215`
  - `2026-03-12`: `conversation_volume == 0`, `ai_resolved == 0`, `handed_off == 0`, `satisfaction_avg` is **null**, `satisfaction_count == 0`, `total_tokens == 0` (the only conversation that day is soft-deleted)

- [X] T039 [P] [US3] In `backend/crates/server/tests/analytics_api.rs`, add `timeseries_day_count_matches_range_length`. For ranges of 1 day (`from == to`), 7 days, and 31 days assert `days.len()` equals exactly `to - from + 1` (1, 7, 31) and that `days[0].date == from` and the last entry's date `== to`.

### Implementation for User Story 3

- [X] T040 [US3] Append the timeseries DTOs to `backend/crates/modules/analytics/src/model.rs`, deriving `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]` and using snake_case field names with no rename attribute:

  ```rust
  pub struct TimeseriesDay {
      pub date: NaiveDate,
      pub conversation_volume: i64,
      pub ai_resolved: i64,
      pub handed_off: i64,
      pub satisfaction_avg: Option<f64>,
      pub satisfaction_count: i64,
      pub total_tokens: i64,
  }

  pub struct AnalyticsTimeseriesDto {
      pub range: DateRangeDto,
      pub channel: Option<String>,
      pub days: Vec<TimeseriesDay>,
  }
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T041 [US3] Append `daily_series` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn daily_series(pool: &PgPool, tenant_id: Uuid, from_date: NaiveDate, to_date: NaiveDate, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: Option<&str>) -> sqlx::Result<Vec<(NaiveDate, i64, i64, i64, Option<f64>, i64, i64)>>`. Bind `$1`=tenant_id, `$2`=from_ts, `$3`=to_ts, `$4`=channel, `$5`=from_date, `$6`=to_date. **`generate_series` includes its end value, so it takes the inclusive `$6` date — never the exclusive `$3` timestamp**, otherwise the response gains a bogus trailing day. Exact SQL:

  ```sql
  WITH days AS (
      SELECT generate_series($5::date, $6::date, interval '1 day')::date AS day
  ),
  conv AS (
      SELECT (c.created_at AT TIME ZONE 'UTC')::date AS day,
             COUNT(*)::bigint AS volume,
             COUNT(*) FILTER (WHERE c.status IN ('resolved','closed') AND NOT esc.escalated)::bigint AS ai_resolved,
             COUNT(*) FILTER (WHERE esc.escalated)::bigint AS handed_off
      FROM conversations c
      CROSS JOIN LATERAL (
          SELECT EXISTS (
              SELECT 1 FROM escalations es
              WHERE es.tenant_id = c.tenant_id AND es.conversation_id = c.id
          ) AS escalated
      ) esc
      WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
        AND c.created_at >= $2 AND c.created_at < $3
        AND ($4::text IS NULL OR c.channel = $4)
      GROUP BY 1
  ),
  fb AS (
      SELECT (f.submitted_at AT TIME ZONE 'UTC')::date AS day,
             AVG(f.rating)::float8 AS avg_rating,
             COUNT(*)::bigint AS rating_count
      FROM conversation_feedback f
      WHERE f.tenant_id = $1 AND f.submitted_at >= $2 AND f.submitted_at < $3
        AND ($4::text IS NULL OR f.channel = $4)
      GROUP BY 1
  ),
  tok AS (
      SELECT (u.created_at AT TIME ZONE 'UTC')::date AS day,
             COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint AS tokens
      FROM ai_usage_records u
      WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
      GROUP BY 1
  )
  SELECT d.day,
         COALESCE(conv.volume, 0)::bigint,
         COALESCE(conv.ai_resolved, 0)::bigint,
         COALESCE(conv.handed_off, 0)::bigint,
         fb.avg_rating,
         COALESCE(fb.rating_count, 0)::bigint,
         COALESCE(tok.tokens, 0)::bigint
  FROM days d
  LEFT JOIN conv ON conv.day = d.day
  LEFT JOIN fb ON fb.day = d.day
  LEFT JOIN tok ON tok.day = d.day
  ORDER BY d.day
  ```

  `fb.avg_rating` stays nullable on purpose — a day with no ratings must report `null`, not `0`. **Done when** `cargo check -p analytics` succeeds.

- [X] T042 [US3] Add the timeseries handler to `backend/crates/modules/analytics/src/routes.rs`, mirroring the summary handler written earlier in this same file (same `State` + `tenancy::TenantContext` + `Query<model::AnalyticsQuery>` extractors, same `resolve_query` → 422 error path, same `ApiError::internal_error` on DB failure):

  ```rust
  #[utoipa::path(
      get,
      path = "/tenant/analytics/timeseries",
      tag = "analytics",
      operation_id = "get_analytics_timeseries",
      summary = "Tenant analytics daily time series",
      params(
          ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
          ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
          ("channel" = Option<String>, Query, description = "Optional channel filter"),
      ),
      responses(
          (status = 200, description = "Daily analytics series.", body = model::AnalyticsTimeseriesDto),
          (status = 422, description = "Invalid query parameters."),
      ),
  )]
  pub async fn get_analytics_timeseries(...) -> Response
  ```

  Body: resolve the query, call `queries::daily_series`, map each returned tuple into a `model::TimeseriesDay`, and return `(StatusCode::OK, Json(dto))`. Round `satisfaction_avg` per day with `v.map(|v| (v * 10.0).round() / 10.0)`. **Done when** `cargo check -p analytics` succeeds.

- [X] T043 [US3] Wire the timeseries route and schemas. In `backend/crates/server/src/router.rs`, directly below the analytics summary block added earlier, add:

  ```rust
  .routes(
      routes!(analytics::routes::get_analytics_timeseries)
          .layer(require_permission(Permission::AnalyticsView)),
  )
  ```

  In `backend/crates/server/src/openapi.rs`, add `analytics::model::TimeseriesDay,` and `analytics::model::AnalyticsTimeseriesDto,` to the `components(schemas(...))` list beside the other analytics entries. **Done when** `cd backend && cargo check -p server` and `cargo test --test openapi_valid` succeed.

- [X] T044 [US3] Create the shared chart component `frontend/apps/dashboard/src/app/shared/components/trend-chart/trend-chart.component.ts`. It is a standalone, `OnPush`, signal-based component rendering hand-built inline SVG — **do not add a charting library** (project rule: charts are inline SVG). Public API:

  ```ts
  export interface TrendSeries { id: string; label: string; color: 'chart-1' | 'chart-2'; points: readonly (number | null)[] }
  // inputs: series = input.required<readonly TrendSeries[]>();
  //         labels = input.required<readonly string[]>();   // one x label per point, e.g. '2026-03-10'
  //         valueLabel = input('Value');                    // used in the accessible table header
  ```

  Rendering rules (transcribe exactly, they are the accessibility contract):
  1. `<svg viewBox="0 0 100 40" preserveAspectRatio="none" role="img" [attr.aria-label]="...">` containing one `<polyline>` per series. Compute each point's `x = (index / Math.max(points.length - 1, 1)) * 100` and `y = 36 - ((value - min) / span) * 32`, where `min`/`max` are computed across **all** series together so they share one scale (never two y-scales). **Compute the span exactly as `const span = max - min || 1;`** — copy this guard from `frontend/apps/dashboard/src/app/shared/components/sparkline/sparkline.component.ts`. Without it a flat series (e.g. a long range of all-zero token days, which is common in real data) divides by zero and emits `NaN` into the `points` attribute, silently breaking the chart. Skip `null` values when computing min/max, and break the polyline by omitting those points. Use the same `.toFixed(2)` point formatting as the sparkline component.
  2. Stroke each polyline with `var(--app-chart-1)` or `var(--app-chart-2)` per the series' `color`, `stroke-width: 2`, `fill: none`, `vector-effect: non-scaling-stroke`.
  3. Overlay one transparent `<rect>` per x position (`width = 100 / points.length`, full height, `fill="transparent"`), each containing a `<title>` child whose text is the x label followed by each series label and value (e.g. `2026-03-10 — Conversations: 12`). This gives native hover tooltips with no mouse-position math.
  4. **Always render a legend** when `series().length > 1`: a `<ul>` of items, each an 8px `<span>` swatch in the series color plus the series label in `var(--app-text-2)` — never put the series color on the text itself.
  5. Always render a visually-hidden `<table>` (class `sr-only`, positioned off-screen via CSS) with a row per label and a column per series, so the data is available to screen readers and satisfies the table-view requirement.

  **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T045 [P] [US3] Create `frontend/apps/dashboard/src/app/shared/components/trend-chart/trend-chart.component.spec.ts`. With `provideZonelessChangeDetection()`, assert: (a) one `<polyline>` renders per series; (b) with two series a legend renders with two items; (c) with one series no legend renders; (d) the visually-hidden `<table>` has one body row per label; (e) `null` points do not produce `NaN` in any `points` attribute; (f) **a flat series where every value is identical (e.g. `[0, 0, 0, 0]`) produces no `NaN`** and renders a valid polyline — this is the divide-by-zero case the `|| 1` span guard prevents. **Done when** `cd frontend && pnpm ng test dashboard` passes.

- [X] T046 [US3] Add timeseries loading to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.ts`. Add an `rxMethod` `loadTimeseries` calling `AnalyticsApiService.getTimeseries({ from, to, channel })` and patching the `timeseries` state field (leave the existing `summary` loading logic untouched; a timeseries failure must set `error` but must not blank out an already-loaded `summary`). Call it everywhere `loadSummary` is triggered — that is, from `load()`, `setPreset`, and `setCustomRange` — so charts and cards always reflect the same filters. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T047 [US3] Add the chart cards to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.ts`. Below the metrics section add a `<section class="charts">` with four `<app-dashboard-card>`s, each containing an `<app-section-header>` and one `<app-trend-chart>` (import `TrendChartComponent` from `../../../shared/components/trend-chart/trend-chart.component`). Build the inputs with `computed()` from `store.timeseries()`, using `days.map(d => d.date)` as `labels`:
  1. **Conversation volume** — one series, `color: 'chart-1'`, points `d.conversationVolume`
  2. **AI resolved vs human handoff** — two series: `{ id:'ai', label:'AI resolved', color:'chart-1', points: d.aiResolved }` and `{ id:'handoff', label:'Human handoff', color:'chart-2', points: d.handedOff }` (this is the chart that gets the legend)
  3. **Satisfaction trend** — one series, `color: 'chart-1'`, points `d.satisfactionAvg` (keep `null` days as `null` — do not coerce to 0, a day with no ratings is a gap, not a zero score)
  4. **Token usage** — one series, `color: 'chart-1'`, points `d.totalTokens`

  When `store.timeseries()` is null, render nothing in this section rather than an empty chart. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T048 [US3] Extend `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.spec.ts` with chart cases: stub `getTimeseries` to return a 3-day series and assert four `app-trend-chart` elements render; assert that a `satisfactionAvg: null` day does not render as a zero point (check the chart's hidden table cell shows `—` or is empty, not `0`). **Done when** `cd frontend && pnpm ng test dashboard` passes.

**Checkpoint**: US1–US3 work — cards, date filters, and four trend charts.

---

## Phase 6: User Story 4 - Break down and filter by channel (Priority: P4)

**Goal**: A per-channel conversation breakdown, plus a channel filter that recomputes every metric and chart.

**Independent Test**: With the canonical seed over 2026-03-10..12, the breakdown shows widget 3 / email 1; `?channel=widget` returns volume 3 and `?channel=email` returns volume 1 with token total 200.

### Tests for User Story 4 (write first)

- [X] T049 [P] [US4] In `backend/crates/server/tests/analytics_api.rs`, add `summary_returns_channel_breakdown`. For tenant A over `from=2026-03-10&to=2026-03-12` with **no** channel filter, assert `channels` contains exactly two entries: `widget` with `conversation_count == 3` and `share` ≈ `0.75`, and `email` with `conversation_count == 1` and `share` ≈ `0.25` (the soft-deleted C5 is excluded). Assert entries are ordered by descending count. Then add a second assertion block for the **single-channel tenant** case: request the same range as tenant **B** (whose only conversation is on `widget`) and assert `channels` has exactly one entry, `widget`, with `conversation_count == 1` and `share == 1.0` — a lone channel must report 100% without a divide-by-zero or rounding artifact.

- [X] T050 [P] [US4] In `backend/crates/server/tests/analytics_api.rs`, add `summary_respects_channel_filter`. For tenant A over `from=2026-03-10&to=2026-03-12` assert: `?channel=widget` → `conversation_volume == 3`, `handoff_rate` ≈ `0.333`, `total_tokens == 150`, `unattributed_tokens == 0`; `?channel=email` → `conversation_volume == 1`, `total_tokens == 200`. Also assert that with a channel filter applied the `channels` breakdown array **still** lists both channels (the breakdown always describes the whole range, per the contract).

### Implementation for User Story 4

- [X] T051 [US4] Append `channel_breakdown` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn channel_breakdown(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>) -> sqlx::Result<Vec<(String, i64)>>`. It deliberately takes **no** channel parameter — the breakdown always covers every channel in the range. Exact SQL:

  ```sql
  SELECT c.channel, COUNT(*)::bigint
  FROM conversations c
  WHERE c.tenant_id = $1 AND c.deleted_at IS NULL
    AND c.created_at >= $2 AND c.created_at < $3
  GROUP BY c.channel
  ORDER BY COUNT(*) DESC, c.channel ASC
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T052 [US4] Append `token_totals_for_channel` to `backend/crates/modules/analytics/src/queries.rs`. Signature: `pub async fn token_totals_for_channel(pool: &PgPool, tenant_id: Uuid, from_ts: DateTime<Utc>, to_ts: DateTime<Utc>, channel: &str) -> sqlx::Result<i64>`. Usage rows carry no channel, so attribution goes through `ai_generations` to `conversations`. Exact SQL (`$4` = channel):

  ```sql
  SELECT COALESCE(SUM(COALESCE(u.input_tokens,0) + COALESCE(u.output_tokens,0)), 0)::bigint
  FROM ai_usage_records u
  WHERE u.tenant_id = $1 AND u.created_at >= $2 AND u.created_at < $3
    AND EXISTS (
        SELECT 1 FROM ai_generations g
        JOIN conversations c ON c.tenant_id = g.tenant_id AND c.id = g.conversation_id
        WHERE g.tenant_id = u.tenant_id AND g.usage_record_id = u.id
          AND c.channel = $4 AND c.deleted_at IS NULL
    )
  ```

  **Done when** `cargo check -p analytics` succeeds.

- [X] T053 [US4] Update the summary handler in `backend/crates/modules/analytics/src/routes.rs` to populate channels and channel-aware tokens. Two changes only:
  1. Call `queries::channel_breakdown(...)` (no channel argument) and map the rows into `Vec<ChannelBreakdownItem>`, computing `share` in Rust as `count as f64 / volume_all_channels as f64`, using `0.0` when the total is 0. The total for `share` is the **sum of the breakdown counts**, not the possibly channel-filtered `conversation_volume`.
  2. For tokens, branch: when `resolved.channel` is `Some(c)` call `queries::token_totals_for_channel(..., c)` and set `unattributed_tokens: 0`; otherwise keep the existing `queries::token_totals` call returning both figures.

  **Done when** `cd backend && cargo test --test analytics_api` passes with a live `DATABASE_URL`.

- [X] T054 [US4] Create the breakdown component `frontend/apps/dashboard/src/app/shared/components/breakdown-bars/breakdown-bars.component.ts` — standalone, `OnPush`, signal inputs. API: `items = input.required<readonly { label: string; count: number; share: number }[]>()`. Render a `<ul>` where each `<li>` is a row containing the label, the count, the share as a percentage with one decimal, and a horizontal bar (`<div>` track with an inner fill whose `width` is `share * 100` percent, background `var(--app-chart-1)`, `border-radius: var(--app-radius-sm)`). Do **not** use a pie or donut. Label and value text use `var(--app-text)` / `var(--app-text-2)`, never the bar color. Give the `<ul>` an `aria-label` naming the breakdown. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T055 [US4] Add the channel filter and breakdown card to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.ts`. In the toolbar add a second `<app-select-filter>` with `label="Channel"` and options `[{value:'all',label:'All channels'},{value:'widget',label:'Website widget'},{value:'email',label:'Email'},{value:'phone',label:'Phone'},{value:'web_chat',label:'Web chat'},{value:'whatsapp',label:'WhatsApp'},{value:'telegram',label:'Telegram'}]` — these values must match the DB channel vocabulary exactly. On `(valueChange)` call a new store method `setChannel(value)`. Below the charts add a `<app-dashboard-card>` titled "Channel breakdown" containing `<app-breakdown-bars>` fed by a `computed()` mapping `store.summary()?.channels` into `{ label, count, share }` (map raw channel values to the same human labels used in the select). **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T056 [US4] Add `setChannel` to `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.ts`. Signature `setChannel(value: string): void`: patch `channel` to `null` when the value is `'all'`, otherwise to the value, then trigger both the summary and timeseries loads (the same ones `load()` triggers) so cards, charts, and breakdown stay consistent. **Done when** `cd frontend && pnpm ng build dashboard` succeeds.

- [X] T057 [US4] Extend the frontend specs for channel behavior. In `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.spec.ts` assert `setChannel('widget')` sends `channel: 'widget'` to both `getSummary` and `getTimeseries`, that it issues **exactly one call to each** (the SC-006 guard, matching T037 case (d)), and that `setChannel('all')` sends `channel: null`. In `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.component.spec.ts` assert the breakdown card renders one row per entry in `summary.channels`. **Done when** `cd frontend && pnpm ng test dashboard` passes.

**Checkpoint**: All four user stories complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T058 [P] Remove the now-dead analytics fixture path in `frontend/apps/dashboard/src/app/features/tenant/routed-page-data.service.ts`. The analytics page no longer uses `RoutedPageDataService`. Delete the `analytics` entries from the `PagePayload` union (line ~86) and the three `case 'analytics':` branches (~147, ~209, ~265), plus the now-unused import from `../../shared/fixtures/analytics.fixtures` (line ~10) **only if** no other case still references those fixtures. Then delete any exports in `frontend/apps/dashboard/src/app/shared/fixtures/analytics.fixtures.ts` that nothing imports — but keep `CHANNEL_BREAKDOWN` if `frontend/apps/dashboard/src/app/shared/fixtures/fixtures.spec.ts` still imports it, and keep `OVERVIEW_METRICS` (the overview page uses it). Verify with a repo-wide search for each symbol before deleting it. **Done when** `cd frontend && pnpm ng build dashboard && pnpm ng test dashboard` both pass.

- [X] T059 [P] Update the module documentation block at the top of `backend/crates/modules/analytics/src/lib.rs` so its `## Public Interfaces` and `## Data Model` sections match what was actually built (both endpoints, all query functions, the index-only migration 0052, and the note that `ai_usage_records` is channel-attributed through `ai_generations`). **Done when** the doc block names both endpoints and migration 0052.

- [X] T060 [P] Add the feature entry to the `## Recent Changes` list in `CLAUDE.md` at the repo root, following the existing one-entry-per-feature style: `- 025-analytics-foundation: tenant analytics (summary + daily timeseries endpoints over existing conversation/feedback/usage tables, no rollup tables; metric cards, inline-SVG trend charts, date-range and channel filters; analytics.view restricted to Owner/Admin/Manager). See specs/025-analytics-foundation/plan.md.` **Done when** the entry exists in `CLAUDE.md`.

- [X] T061 Register the two new analytics routes in the RBAC route-coverage list in `backend/crates/server/tests/rbac.rs`. Add these entries to `const TENANT_OPERATIONS: &[(&str, &str)]` (around line 50), following the existing `(uri, permission)` tuple style:

  ```rust
  ("/api/v1/tenant/analytics/summary", "analytics.view"),
  ("/api/v1/tenant/analytics/timeseries", "analytics.view"),
  ```

  This list drives `no_role_user_is_denied_by_every_protected_api_route` (around line 1071), so adding the routes extends that fail-closed guarantee to analytics. Do this **after** the routes are wired (T025 and T043) — otherwise the test hits a route that does not exist yet. **Done when** `cd backend && cargo test --test rbac` passes.

- [X] T062 Add the metric-stability regression test for FR-016 to `backend/crates/server/tests/analytics_api.rs`. Name it `past_period_metrics_do_not_drift_when_new_activity_arrives`. Seed the canonical dataset, request `/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12` as a tenant-A admin and keep the parsed body. Then insert **new** activity dated outside that range — one conversation created `2026-04-01T09:00:00Z` (channel widget, status closed) and one `ai_usage_records` row created `2026-04-01T09:00:00Z` with 500 input tokens. Re-request the identical URL and assert the second body equals the first **field for field** (`conversation_volume`, `concluded_count`, both rates, both response-time averages, satisfaction, `total_tokens`). FR-016 permits exactly one retroactive change — a late feedback rating — so do **not** insert feedback in this test. **Done when** `cd backend && cargo test --test analytics_api past_period_metrics_do_not_drift` passes.

- [X] T063 Add the SC-004 performance check to `backend/crates/server/tests/analytics_api.rs`. Name it `summary_and_timeseries_are_fast_on_a_large_tenant` and mark it `#[ignore]` with a comment explaining it is opt-in because it seeds 100k rows (run it with `cargo test --test analytics_api -- --ignored`). Seed one tenant and one customer, then bulk-insert 100,000 conversations in a **single** statement — do not loop:

  ```sql
  INSERT INTO conversations (tenant_id, customer_id, channel, status, created_at, last_activity_at)
  SELECT $1, $2, 'widget',
         CASE WHEN g % 3 = 0 THEN 'closed' ELSE 'open' END,
         TIMESTAMPTZ '2026-01-01 00:00:00Z' + (g % 90) * INTERVAL '1 day',
         TIMESTAMPTZ '2026-01-01 00:00:00Z' + (g % 90) * INTERVAL '1 day'
  FROM generate_series(1, 100000) AS g
  ```

  Then time a 90-day-range request to `/tenant/analytics/summary` and one to `/tenant/analytics/timeseries` using `std::time::Instant`. Assert each completes in under 3 seconds (SC-004's budget covers the whole dashboard render; the two API calls are its dominant cost). If either assertion fails, the live-aggregation approach chosen in research.md R1 no longer holds and rollup tables must be reconsidered — report that rather than raising the threshold. **Done when** `cargo test --test analytics_api -- --ignored` passes with a live `DATABASE_URL`.

- [X] T064 Create the end-to-end test `frontend/e2e/analytics.spec.ts`. Constitution Principle VII requires end-to-end coverage, and every comparable feature in this repo ships one. **Copy the structure of `frontend/e2e/escalation-routing.spec.ts`**: a single `await page.route('**/api/v1/**', async (route) => { ... })` handler that strips the `/api/v1` prefix from the URL path and returns JSON fixtures per path, including `/me` for the signed-in identity (give the user a tenant `admin` role so `analytics.view` is present). Serve fixtures for `/tenant/analytics/summary` and `/tenant/analytics/timeseries`, reading `from`, `to`, and `channel` off the request URL so the assertions below can distinguish calls. Then write these tests, each doing `await page.goto('/tenant/analytics')` first:
  1. **Cards render** — the seven metric cards appear with the fixture's values.
  2. **Date preset drives the request** — selecting "Last 7 days" issues a summary request whose `from`/`to` span 7 inclusive days, and the cards update.
  3. **Channel filter drives the request** — selecting the website-widget channel issues requests carrying `channel=widget`, and the channel breakdown still lists every channel.
  4. **Charts render** — four `app-trend-chart` elements are present, and the AI-resolved-vs-handoff chart shows a legend with two entries.
  5. **Empty state** — when the summary fixture returns zeros with `null` rates, the cards show `—` rather than `0%`.

  Run with `cd frontend && pnpm test:e2e analytics` (Playwright starts the dashboard dev server itself per `playwright.config.ts`). **Done when** all five tests pass.

- [X] T065 Run the full backend suite: `cd backend && cargo test`. Every test must pass — pay particular attention to `rbac.rs` (changed by T004/T005/T061) and the three `openapi_*` tests (changed by T026/T043). Fix any regression before continuing. **Done when** `cargo test` reports zero failures.

- [X] T066 Run the full frontend gate: `cd frontend && pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check`. All four must pass. **Done when** all four commands exit 0.

- [X] T067 Execute the manual smoke steps in `specs/025-analytics-foundation/quickstart.md` section 5 (sign in as Admin, check cards render real data, switch date presets, apply a channel filter, submit a widget rating and confirm satisfaction updates, then confirm a Viewer gets 403). Record any deviation as a new task rather than silently fixing scope. **Done when** every step behaves as quickstart.md describes.

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (T001–T003)**: no dependencies; T003 is `[P]` with T001–T002.
- **Foundational (T004–T013)**: needs Setup. **Blocks every user story.**
- **US1 (T014–T031)**: needs Foundational. No dependency on other stories.
- **US2 (T032–T037)**: needs Foundational; in practice runs after US1 because it edits the same handler and page.
- **US3 (T038–T048)**: needs Foundational; the new endpoint is independent of US1/US2, but T047 edits the page component US1 created.
- **US4 (T049–T057)**: needs Foundational; T053 edits the summary handler from US1 and T055 edits the page.
- **Polish (T058–T067)**: needs every story you intend to ship. T061 additionally requires both routes to be wired (T025, T043). T064 (E2E) requires the page to be complete through US4, since it asserts cards, charts, and both filters. Run the three gate tasks (T065–T067) last.

### Within each story

- Write the test tasks first and confirm they fail before implementing (Principle VII).
- Order inside a story: DTOs → queries → handler → route/OpenAPI wiring → store → component → specs.

### Parallel opportunities

- T001/T002 are sequential (both touch Cargo manifests in the same build graph); T003 is `[P]` alongside them.
- T011, T012 are `[P]` with each other (different frontend files); T013 depends on T012.
- Within each story, all test-writing tasks are `[P]` with each other — but they all append to `backend/crates/server/tests/analytics_api.rs`, so if one agent runs them, do them sequentially to avoid edit conflicts.
- T045 (`trend-chart.component.spec.ts`) is `[P]` with T046 (store) — different files.
- T058, T059, T060 are `[P]` (three different files).
- T064 (`frontend/e2e/analytics.spec.ts`) is `[P]` with T062 and T063 — different file, different toolchain.

### Sequential-file warnings (do not run these in parallel)

- `backend/crates/modules/analytics/src/queries.rs` — appended by T019–T023, T041, T051, T052.
- `backend/crates/modules/analytics/src/model.rs` — appended by T007, T018, T040.
- `backend/crates/modules/analytics/src/routes.rs` — edited by T009, T024, T042, T053.
- `backend/crates/server/tests/analytics_api.rs` — appended by T010, T014–T017, T032, T033, T038, T039, T049, T050, T062, T063.
- `frontend/.../analytics/analytics.component.ts` — edited by T029, T035, T047, T055.
- `frontend/.../analytics/analytics.store.ts` — edited by T027, T036, T046, T056.

---

## Implementation Strategy

### MVP first (US1 only)

1. Phase 1 Setup (T001–T003)
2. Phase 2 Foundational (T004–T013) — blocking
3. Phase 3 US1 (T014–T031)
4. **Stop and validate**: `cargo test --test analytics_api` and `pnpm ng test dashboard` pass; the Analytics page shows real tenant-scoped numbers.
5. This alone satisfies the spec's primary acceptance criterion ("Tenant admins can view basic analytics, tenant-scoped").

### Incremental delivery

1. Setup + Foundational → foundation ready
2. + US1 → real metric cards (MVP, demoable)
3. + US2 → date filtering (satisfies "analytics support date filtering")
4. + US3 → trend charts
5. + US4 → channel breakdown and filtering
6. Polish → full gates green

---

## Notes

- `[P]` = different files, no dependencies. When one agent works alone, prefer strict ID order — it is always safe.
- Never coerce a `null` metric to `0` anywhere in the stack: `null` means "no data" and must render as `—`.
- Every SQL statement must filter `tenant_id = $1`; every conversation query must also exclude `deleted_at IS NOT NULL`.
- The upper date bound is always **exclusive** (`< $to_ts`) except `generate_series`, which takes the inclusive `to` date.
- Commit after each task or each logical group.

---

## Phase 8: Convergence

Appended by `/speckit-converge` after verifying the codebase against spec, plan, and tasks. All 67 prior tasks are marked complete, but the checks below were run and failed — each item here is verified-not-implemented, not a restatement of existing work.

- [X] T068 CRITICAL: Fix the runtime failure that makes the analytics page render an error instead of content, per US1/AC1 + FR-015 + Constitution VII (contradicts). **Reproduce**: `cd frontend && pnpm exec playwright install chromium` (once), then `pnpm test:e2e analytics` → all 5 tests fail with `locator('app-metric-card')` resolving to 0 elements. The page renders its header and both filter dropdowns, then the content area shows the "Something went wrong" empty-state whose message is: `NG0602: toSignal() cannot be called from within a reactive context.` **Cause**: `analytics.store.ts` triggers HTTP from inside a reactive context. Its `withHooks` `onInit` runs `effect(() => { if (activeTenant()?.id) store.load(); })`, and `load()` synchronously invokes the `loadSummary`/`loadTimeseries` `rxMethod`s whose `switchMap` bodies both read signals (`store.from()`, `store.to()`, `store.channel()`) and issue the request. The request passes through `apps/dashboard/src/app/core/http/tenant-context.interceptor.ts`, which calls `toSignal(store.select(selectActiveTenant))()` — legal normally, illegal inside a reactive context — so Angular throws, `catchError` captures the NG0602 text, and the store lands in its error state. **Fix direction** (do not change the shared interceptor, which every other feature depends on): make the analytics store match the working pattern in `features/tenant/conversations/conversations.store.ts` — read `from`/`to`/`channel` *outside* the pipe and pass them in as `rxMethod` parameters, and wrap the `onInit` invocation in `untracked(() => store.load())` (import `untracked` from `@angular/core`) so the subscription is not created inside the effect's reactive context. **Done when** `pnpm test:e2e analytics` reports 5 passed, and `pnpm ng test dashboard` still passes. Note: unit tests cannot catch this — they stub `AnalyticsApiService`, so the real interceptor never runs; the e2e is the only guard.

- [X] T069 Register the two analytics operations in the OpenAPI contract inventory at `backend/crates/server/tests/openapi_coverage.rs`, per plan (API-First contract consistency) + Constitution V (missing). **Reproduce**: `cd backend && cargo test --test openapi_coverage` → `documented_paths_equal_expected_inventory` FAILS with `documented operations not in the contract inventory: [("GET", "/tenant/analytics/summary"), ("GET", "/tenant/analytics/timeseries"), …]`. The routes were wired and their schemas registered (T025/T026/T043), but this test keeps a separate hand-maintained `const EXPECTED: &[(&str, &str)]` list (starts at line 49) that was never updated. Add these two rows alongside the other tenant entries, following the existing `("GET", "/tenant/widgets")` style:

  ```rust
  ("GET", "/tenant/analytics/summary"),
  ("GET", "/tenant/analytics/timeseries"),
  ```

  **Important**: the same failure also lists three `feedback` operations (`/tenant/feedback/summary`, `/widget/v1/feedback/pending`, `POST /widget/v1/conversations/{conversationId}/feedback`). Those belong to spec 024, not this feature — do **not** delete or weaken the assertion to make the test green. Add only the two analytics rows here; the test stays red until 024 registers its own, which is correct and expected. **Done when** the failure message no longer names either analytics path.

- [X] T070 Execute the analytics integration suite against a live database, per SC-002 + SC-003 (partial). **Why**: `cargo test --test analytics_api` currently reports "11 passed" in **0.00s** because `DATABASE_URL` is unset and every test hits the `let Some(pool) = get_pool().await else { return };` early return. Not one assertion has ever run, so tenant isolation (SC-002) and exact-value correctness (SC-003) are unverified despite the green output. Start Postgres, apply migrations through `0052_analytics_indexes.sql`, then run with the guard that turns skips into failures: `cd backend && REQUIRE_DB_TESTS=1 DATABASE_URL=<url> cargo test --test analytics_api`. Fix any assertion that fails against the real schema. Then run the opt-in performance check for SC-004: `REQUIRE_DB_TESTS=1 DATABASE_URL=<url> cargo test --test analytics_api -- --ignored`. **Done when** all 11 tests report pass with a non-zero duration, and the ignored performance test passes under its 3-second budget.

- [X] T071 Add the feature entry to the `## Recent Changes` list in `CLAUDE.md` at the repo root, per plan (Documentation & Future Readiness) (missing). T060 is marked complete but `grep "025-analytics" CLAUDE.md` returns no match — the entry was never written. Follow the existing one-line-per-feature style used by the `024-customer-feedback` and `023-website-chat-widget` entries: `- 025-analytics-foundation: tenant analytics (summary + daily timeseries endpoints aggregating existing conversation/feedback/usage tables, no rollup tables; metric cards, inline-SVG trend charts, date-range and channel filters; analytics.view restricted to Owner/Admin/Manager). See specs/025-analytics-foundation/plan.md.` **Done when** the entry appears in `CLAUDE.md`.

---

## Phase 9: Convergence

Appended by `/speckit-converge` after re-verifying the codebase. Everything through Phase 8 was re-checked by running the real gates, not by reading checkboxes. **Verified working and deliberately not re-tasked**: backend module/queries/routes/migration 0052 all present; `authz` matrix and `rbac.rs` route coverage correct (Viewer has no `analytics.view`); OpenAPI schemas + inventory rows registered (T069 landed); `CLAUDE.md` entry present (T071 landed); `pnpm ng build dashboard` clean; 994 dashboard unit tests pass; eslint and prettier clean; and T068's NG0602 fix genuinely landed — the page now renders 7 metric cards and 4 charts instead of an error state. The single task below is the one verified-not-working item.

- [X] T072 Make the analytics end-to-end suite pass, per US1/AC1 + US2/AC1 + US4/AC2 + Constitution VII (partial). T064 and T068 are both marked complete, but the suite has never been green: `cd frontend && ./node_modules/.bin/playwright test analytics` reports **3 failed, 2 passed**. All three failures are defects in `frontend/e2e/analytics.spec.ts` itself, where the test drives UI affordances the components never implemented. **The components are correct and follow tasks.md — fix the test file only. Do NOT change `analytics.component.ts`, `select-filter.component.ts`, or `metric-card.component.ts`,** or you will regress T029/T035/T055 and the passing unit tests.

  Three fixes, all in `frontend/e2e/analytics.spec.ts`:

  1. **Native-select interaction (2 failures, one root cause).** Tests at lines ~234 ("Date preset drives the request") and ~271 ("Channel filter drives the request") fail with `getByRole('button', { name: /Last 30 days/i })` → `element(s) not found`, likewise `/All channels/i`. Cause: `shared/components/select-filter/select-filter.component.ts` renders a **native `<select>`** with `<option>` children (its ARIA role is `combobox`, not `button`), exactly as T035 and T055 specified — there is no button-style dropdown to click. Replace the two-step click pattern (`getByRole('button', …).click()` on the trigger, then on the option label) with native select handling, keying off the `aria-label` the component sets from its `label` input:

     ```ts
     // date preset test
     await expect(page.getByLabel('Date range')).toBeVisible();
     await page.getByLabel('Date range').selectOption('7');

     // channel test
     await expect(page.getByLabel('Channel')).toBeVisible();
     await page.getByLabel('Channel').selectOption('widget');
     ```

     Note `selectOption` takes the option **value** (`'7'`, `'widget'`), not the visible label. Keep every existing assertion about the resulting request URLs (`from`/`to` spanning ≤ 7 days; `channel=widget`) unchanged — those assertions are correct and are the point of both tests.

  2. **Conversation-volume formatting assertion (1 failure).** The "Cards render" test at line ~225 asserts `cards.nth(0)` contains `'1,240'`; the card actually renders `'1240'` (`Received string: "Conversations1240"`). The component is right: T029 specifies conversation volume as a **plain integer string** and deliberately reserves `toLocaleString()` for the tokens card. Change the assertion to `await expect(cards.nth(0)).toContainText('1240');`. (If the product genuinely wants thousands separators on volume, that is a spec change and belongs in a new spec revision — not in converge, and not by editing the component here.)

  3. **Locale hardening on the tokens assertion (same test, do it while you are in the file).** Line ~231 asserts `'5,482,210'`, which only holds because `Number.prototype.toLocaleString()` defaults to a comma-grouping locale on the current machine — it will break on a CI runner with a different default locale. Make it locale-independent, e.g. assert against `(5482210).toLocaleString()` computed in the test rather than the hardcoded literal.

  **Done when** `cd frontend && ./node_modules/.bin/playwright test analytics` reports **5 passed**, and `./node_modules/.bin/ng test dashboard` and `./node_modules/.bin/ng build dashboard` still pass. To run the suite you need the dev server on `127.0.0.1:4201`; `playwright.config.ts` starts it with `pnpm`, so if `pnpm` is unavailable start it yourself (`./node_modules/.bin/ng serve dashboard --host 127.0.0.1 --port 4201`) — `reuseExistingServer` is enabled outside CI and Playwright will attach to it.
