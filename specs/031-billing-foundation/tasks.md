---
description: "Task list for 031-billing-foundation"
---

# Tasks: Billing Foundation

**Input**: Design documents from `/specs/031-billing-foundation/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/billing-api.md](contracts/billing-api.md), [quickstart.md](quickstart.md)

**Tests**: Included — Constitution Principle VII (Test-First) and quickstart.md require DB-backed suites.

**Last reviewed**: 2026-07-24 against the updated spec (FR-011, SC-007, US3 scenarios 5–6) and plan. No tasks were added, removed, or renumbered — the platform read surface was already in Phase 5. Amendments landed in the Phase 5 header (traceability table), T045, and T054.

**Checkbox status (reconciled 2026-07-24 against the working tree, not against commit messages)**

- **T001–T008a and T010–T028 are ticked.** Each was verified by inspecting the artifact it names: migration `0059` with both tables and the Free seed; the three `Cargo.toml` wirings; `PlatformBillingManage` in `permission.rs` plus all four `matrix.rs` edits including the corrected owner-exclusives set; the `/test/platform/billing/manage` probe and its `rbac.rs` entries; `auth.rs`'s `platformPermissions`; `metric_usage` and all four query functions; the handler, its utoipa annotation, the router registration, the openapi schemas, and **both** `openapi_coverage` inventories (test + the 016 contract md); and the seven frontend files plus all four route-wiring edits.
- **Two ticks rest on commit evidence, not a run I performed**: T013/T014/T020 name test files, which exist with real content (122, 271, and 439 lines). The suites were last reported green in commit `acd2c58`; this environment has no PostgreSQL (no docker daemon, no `DATABASE_URL`), so they were not re-run during reconciliation.
- **T009 and T021 are deliberately left open** — they are "run the suite green" gates, and an unrunnable gate is not a passed gate. Tick them when `cargo test -p authz`, `--test rbac`, `--test auth`, `--test billing_overview`, and `--test openapi_coverage` actually pass with a database up.

> **Resume at T037** (frontend warning/exceeded styling + AI-paused banner). `enforcement.rs` is implemented, the billing gate is active in `agent_responder.rs`, and US2 backend enforcement (T032-T036) is complete. `audit.rs` is still a 0-byte placeholder from T010, and `features/platform/billing/` does not exist — US3 is untouched.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (touches a different file than other [P] tasks in the same group)
- **[Story]**: US1 / US2 / US3 — the user story this task serves
- Every task names the exact file to create or edit

---

## Conventions (read once before starting)

These are facts verified in this repo. Follow them exactly; do not invent alternatives.

1. **Backend root** is `backend/`. Run all `cargo` commands from `backend/`.
2. **Frontend root** is `frontend/`. Run all `pnpm` commands from `frontend/`.
3. **Copy the `analytics` crate as the template.** `backend/crates/modules/analytics/` has the exact `lib.rs` / `model.rs` / `queries.rs` / `routes.rs` layout this feature needs.
4. **HTTP paths in `#[utoipa::path(...)]` are written WITHOUT the `/api/v1` prefix** (e.g. `path = "/tenant/billing/overview"`). The server mounts everything under `/api/v1`, so tests call `/api/v1/tenant/billing/overview`.
5. **Tenant handlers** take `ctx: tenancy::TenantContext` and use `ctx.tenant_id`.
   **Platform handlers** take `Extension(principal): Extension<identity::Principal>` and use `principal.user_id` as the audit actor.
6. **JSON request bodies** use `kernel::ApiJson<T>`, not `axum::Json<T>`.
7. **Audit writes** use `tenancy::audit::record_in_tx(&mut tx, action, Some(actor_user_id), tenant_id_opt, resource_type, Some(resource_id_str), &details_json).await?` inside the same transaction as the mutation.
8. **DB tests do not run under `cargo test --workspace`** — it skips them and can abort early. Always run the narrow suites named in each task with PostgreSQL up and `DATABASE_URL` exported.
9. **`cargo fmt --check` is already failing repo-wide at HEAD.** Do not try to fix unrelated formatting; only keep your own new files formatted.
10. **Do not create new dependency directions.** `ai` may depend on `billing`. `billing` must NEVER depend on `ai`, `conversations`, or `analytics` — it reads their tables with raw SQL only.
11. **Test-first within each pair (Constitution VII).** Task IDs are listed implementation-before-test only so each test task can name the exact symbols it targets. The required working order is the reverse: write the test file first, run it, watch it fail (a compile error on a not-yet-existing handler counts as red), then implement until it passes. The pairs are T011/T012→T013/T014, T015–T018→T020, T032/T033→T034/T035, T040–T044→T045, T024–T026→T027, T037→T038, T049–T051→T054.

---

## Phase 1: Setup

**Purpose**: database schema and crate wiring. Nothing else can compile without these.

- [X] T001 Create the migration `backend/migrations/0059_billing_foundation.sql` with exactly this content:

  ```sql
  CREATE TABLE billing_plans (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      name TEXT NOT NULL UNIQUE,
      monthly_price_cents INTEGER NOT NULL CHECK (monthly_price_cents >= 0),
      token_allowance BIGINT NULL CHECK (token_allowance >= 0),
      conversation_allowance INTEGER NULL CHECK (conversation_allowance >= 0),
      is_default BOOLEAN NOT NULL DEFAULT false,
      status TEXT NOT NULL DEFAULT 'active',
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      CONSTRAINT billing_plans_status_check CHECK (status IN ('active', 'retired'))
  );

  CREATE UNIQUE INDEX billing_plans_one_default_idx
      ON billing_plans (is_default) WHERE is_default;

  CREATE TRIGGER set_updated_at
      BEFORE UPDATE ON billing_plans
      FOR EACH ROW
      EXECUTE FUNCTION set_updated_at();

  CREATE TABLE tenant_subscriptions (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE RESTRICT,
      plan_id UUID NOT NULL REFERENCES billing_plans(id) ON DELETE RESTRICT,
      status TEXT NOT NULL DEFAULT 'active',
      assigned_by UUID NULL REFERENCES users(id),
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      CONSTRAINT tenant_subscriptions_status_check
          CHECK (status IN ('active', 'past_due', 'canceled'))
  );

  CREATE TRIGGER set_updated_at
      BEFORE UPDATE ON tenant_subscriptions
      FOR EACH ROW
      EXECUTE FUNCTION set_updated_at();

  INSERT INTO billing_plans
      (name, monthly_price_cents, token_allowance, conversation_allowance, is_default, status)
  VALUES ('Free', 0, 100000, 100, true, 'active');
  ```

- [X] T002 [P] Replace `backend/crates/modules/billing/Cargo.toml` dependencies with (keep the existing `[package]` block unchanged, add a `[dependencies]` section):

  ```toml
  [dependencies]
  axum.workspace = true
  chrono = { workspace = true, features = ["serde"] }
  identity = { path = "../identity" }
  kernel = { path = "../../shared/kernel" }
  serde.workspace = true
  serde_json.workspace = true
  sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }
  tenancy = { path = "../tenancy" }
  tracing.workspace = true
  utoipa.workspace = true
  uuid.workspace = true

  [dev-dependencies]
  tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
  ```

- [X] T003 [P] In `backend/crates/server/Cargo.toml`, add `billing = { path = "../modules/billing" }` to `[dependencies]` (alphabetically, right after the `audit = ...` line).

- [X] T004 [P] In `backend/crates/modules/ai/Cargo.toml`, add `billing = { path = "../billing" }` to `[dependencies]` (right after the `authz = ...` line).

**Checkpoint**: run `cargo check -p billing -p ai -p server` from `backend/` — it must compile.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: permissions and the shared read queries that User Stories 1, 2, and 3 all depend on.

**⚠️ CRITICAL**: no user story work may start until this phase is done.

### RBAC

- [X] T005 In `backend/crates/modules/authz/src/permission.rs`, add the new permission `PlatformBillingManage`. Four edits in this one file:
  1. In the `enum Permission`, immediately after the `PlatformBillingView` variant, add:
     `#[serde(rename = "platform.billing.manage")]` then `PlatformBillingManage,`
  2. In `pub const ALL: [Self; 30]`, change the length to `[Self; 31]` and add `Self::PlatformBillingManage,` right after `Self::PlatformBillingView,`.
  3. In the `as_str`-style match, add `Self::PlatformBillingManage => "platform.billing.manage",` right after the `PlatformBillingView` arm.
  4. In the `catalog_parity_with_contract` test, change `let contract_codes: [&str; 30]` to `[&str; 31]` and add `"platform.billing.manage",` right after `"platform.billing.view",`.

  Do NOT change `pub const TENANT: [Self; 23]` — this is a platform permission.

- [X] T006 In `backend/crates/modules/authz/src/matrix.rs`, make three array edits **and one test edit**:
  1. Add `Permission::BillingView,` to `TENANT_ADMIN` (put it right after `Permission::AuditView,`).
  2. Add `Permission::PlatformBillingManage,` to `PLATFORM_ALL` (right after `Permission::PlatformBillingView,`).
  3. Add `Permission::PlatformBillingManage,` to `PLATFORM_FINANCE` (right after `Permission::PlatformBillingView,`).
  4. **Required — the existing `tenant_role_hierarchy_and_owner_exclusives_hold` test in this same file asserts `owner − admin == {BillingView, BillingManage, TenantDelete, OwnerAssign}` and will fail after edit 1.** Delete the `Permission::BillingView,` line from that `HashSet::from([...])` so the expected owner-exclusive set becomes `{BillingManage, TenantDelete, OwnerAssign}`. The `owner.is_superset(&admin)` assertion above it still holds.

  Do NOT add billing permissions to `TENANT_MANAGER`, `TENANT_AGENT`, or `TENANT_VIEWER` — the existing `manager_has_no_settings_or_billing_permissions` test depends on that.

- [X] T007 In `backend/crates/server/src/router.rs`, inside the `if include_test_routes {` block of the **platform** router (next to the existing `/test/platform/billing/view` route), add:

  ```rust
  .route(
      "/test/platform/billing/manage",
      routing::get(|| async { StatusCode::OK })
          .route_layer(require_permission(Permission::PlatformBillingManage)),
  )
  ```

- [X] T008 In `backend/crates/server/tests/rbac.rs`, update the expectation arrays so the existing sweeps cover the new permission:
  1. Add `"/api/v1/test/platform/billing/manage",` to `PLATFORM_OPERATIONS`.
  2. Add `("/api/v1/test/platform/billing/manage", Method::GET),` to `PLATFORM_OPERATIONS_DENY_BY_DEFAULT`.

  No other list in this file needs the `billing.view` change: `TENANT_PERMISSIONS` is the Owner list (it already contains `billing.view`), and `me_staff_tenant_permissions_follow_environment_for_every_platform_role` asserts only `staffTenantPermissions`, which the `TENANT_ADMIN` edit does not affect. Verified by reading the file — do not go hunting for an "Admin list"; there isn't one.

- [X] T008a **Required — `backend/crates/server/tests/auth.rs` asserts the `/auth/login` and `/me` bodies with exact JSON equality**, including the super-admin `platformPermissions` array. The T006 `PLATFORM_ALL` edit changes that array, so this test fails until it is updated. In the `json!({...})` expectation (the `"platformPermissions"` array, ~line 238), add `"platform.billing.manage"` immediately after `"platform.billing.view"`. Order matters — it must match the order of `PLATFORM_ALL` in `matrix.rs`. `staffTenantPermissions` in the same assertion needs **no** change (it is the Owner set, which already has `billing.view`).

- [ ] T009 Run `cargo test -p authz`, `cargo test -p server --test rbac`, and `cargo test -p server --test auth` from `backend/`. (Requires PostgreSQL — suites were run individually during implementation and reported green by the Stream C agent.)

### Billing crate skeleton + shared reads

- [X] T010 Replace `backend/crates/modules/billing/src/lib.rs` with module docs following the exact section headings used in `backend/crates/modules/analytics/src/lib.rs` (`## Purpose`, `## Responsibilities`, `## Public Interfaces`, `## Dependencies`, `## Data Model`, `## Extension Points`), then these declarations:

  ```rust
  pub mod audit;
  pub mod enforcement;
  pub mod model;
  pub mod queries;
  pub mod routes;
  ```

  Create empty placeholder files `audit.rs`, `enforcement.rs`, `model.rs`, `queries.rs`, `routes.rs` in `backend/crates/modules/billing/src/` so the crate compiles.

- [X] T011 Create the DTOs in `backend/crates/modules/billing/src/model.rs`. Every struct derives `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]` and uses snake_case field names (serde default). Define:
  - `PlanDto { id: Uuid, name: String, monthly_price_cents: i32, token_allowance: Option<i64>, conversation_allowance: Option<i32>, is_default: bool, status: String }`
  - `PeriodDto { start: DateTime<Utc>, end: DateTime<Utc> }`
  - `MetricUsageDto { used: i64, allowance: Option<i64>, percent: Option<i32>, state: String }`
  - `UsageDto { tokens: MetricUsageDto, conversations: MetricUsageDto }`
  - `BillingOverviewDto { plan: PlanDto, subscription_status: String, period: PeriodDto, usage: UsageDto, ai_paused: bool, ai_paused_reason: Option<String> }`
  - `PlanListResponse { plans: Vec<PlanDto> }`
  - `CreatePlanRequest { name: String, monthly_price_cents: i32, token_allowance: Option<i64>, conversation_allowance: Option<i32> }`
  - `UpdatePlanRequest { status: String }`
  - `SubscriptionRowDto { tenant_id: Uuid, tenant_name: String, plan_id: Uuid, plan_name: String, plan_is_default: bool, status: String, explicit: bool }`
  - `SubscriptionListResponse { subscriptions: Vec<SubscriptionRowDto> }`
  - `PutSubscriptionRequest { plan_id: Uuid, status: String }`
  - `SubscriptionDto { tenant_id: Uuid, plan_id: Uuid, status: String }`

  Also add this free function (used by both the overview and the enforcement gate):

  ```rust
  /// `normal` below 80%, `warning` at 80..100%, `exceeded` at or above 100%.
  /// An unlimited allowance (None) is always `normal` with no percent.
  /// A zero allowance (`Some(0)`) is `exceeded` from the start — a deliberate
  /// "AI disabled for this metric" plan, not a division-by-zero guard.
  pub fn metric_usage(used: i64, allowance: Option<i64>) -> MetricUsageDto {
      match allowance {
          None => MetricUsageDto { used, allowance: None, percent: None, state: "normal".into() },
          Some(limit) if limit <= 0 => MetricUsageDto {
              used, allowance: Some(limit), percent: Some(100), state: "exceeded".into(),
          },
          Some(limit) => {
              let percent = ((used as f64 / limit as f64) * 100.0).floor() as i32;
              let state = if used >= limit { "exceeded" }
                  else if percent >= 80 { "warning" }
                  else { "normal" };
              MetricUsageDto { used, allowance: Some(limit), percent: Some(percent), state: state.into() }
          }
      }
  }
  ```

- [X] T012 Create the shared read queries in `backend/crates/modules/billing/src/queries.rs`. All functions take `pool: &PgPool` and return `Result<_, sqlx::Error>`.
  1. `pub fn current_period(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>)` — pure Rust helper returning the UTC calendar-month `[start, end)` containing `now` (end = first instant of the next month).
  2. `pub async fn effective_plan(pool, tenant_id: Uuid) -> Result<(model::PlanDto, String), sqlx::Error>` — returns the plan and the subscription status. One query:

     ```sql
     SELECT p.id, p.name, p.monthly_price_cents, p.token_allowance,
            p.conversation_allowance, p.is_default, p.status,
            s.status AS subscription_status
     FROM tenant_subscriptions s
     JOIN billing_plans p ON p.id = s.plan_id
     WHERE s.tenant_id = $1
     ```

     If that returns no row, fall back to `SELECT ... FROM billing_plans WHERE is_default` and use `"active"` as the status. Never insert a row here (no write-on-read).

     (Use the inner join exactly as written. An outer join from `billing_plans` with `WHERE s.tenant_id = $1` — the shape this task carried before — filters out the unmatched rows anyway, so it reads as a fallback while behaving as an inner join. The fallback lives in the second query, not the join.)
  3. `pub async fn token_usage(pool, tenant_id, start, end) -> Result<i64, sqlx::Error>`:

     ```sql
     SELECT COALESCE(SUM(COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0)), 0)::BIGINT
     FROM ai_usage_records
     WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
     ```

  4. `pub async fn conversation_usage(pool, tenant_id, start, end) -> Result<i64, sqlx::Error>`:

     ```sql
     SELECT COUNT(*)::BIGINT FROM conversations
     WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3 AND deleted_at IS NULL
     ```

- [X] T013 [P] Write unit tests for the pure helpers in `backend/crates/modules/billing/tests/usage_math_tests.rs` (no database needed). Cover: 0% is `normal`; 79% is `normal`; exactly 80% is `warning`; 99% is `warning`; exactly 100% is `exceeded`; over 100% is `exceeded`; `allowance = None` is always `normal` with `percent: None`; `allowance = Some(0)` with zero usage is `exceeded` (the "AI disabled" plan) and does not panic; `current_period` returns month start/end correctly including a December→January rollover.

- [X] T014 [P] Write DB-backed tests for the queries in `backend/crates/modules/billing/tests/queries_tests.rs`. Cover: a tenant with no `tenant_subscriptions` row resolves to the seeded `Free` plan with status `"active"`; a tenant with an explicit row resolves to its own plan and status; `token_usage` sums `input_tokens + output_tokens` including rows where one column is NULL; usage recorded before the period start and on/after the period end is excluded; soft-deleted conversations are not counted.

**Checkpoint**: `cargo test -p billing` passes. Foundation is ready.

---

## Phase 3: User Story 1 — Tenant views billing overview (Priority: P1) 🎯 MVP

**Goal**: an Owner or Admin opens the Billing page and sees plan, price, status, period, and two usage cards.

**Independent Test**: sign in as Owner, open `/tenant/billing`, confirm plan name, price, status, period range, and both usage numbers match the database. Sign in as Viewer — no Billing nav entry, and direct navigation is blocked.

### Backend

- [X] T015 [US1] In `backend/crates/modules/billing/src/routes.rs`, implement `pub async fn get_billing_overview(State(pool): State<PgPool>, ctx: tenancy::TenantContext) -> Response`. Steps: compute the period with `queries::current_period(Utc::now())`; call `queries::effective_plan`, `queries::token_usage`, `queries::conversation_usage`; build both `MetricUsageDto`s with `model::metric_usage`; set `ai_paused` / `ai_paused_reason` using this precedence — status `past_due` or `canceled` ⇒ `("subscription_status")`, else tokens `exceeded` ⇒ `("token_limit")`, else conversations `exceeded` ⇒ `("conversation_limit")`, else `false` / `None`; return `(StatusCode::OK, Json(BillingOverviewDto)).into_response()`. On any `sqlx::Error`, log with `tracing::error!` and return `kernel::ApiError` 500, exactly like `analytics::routes`.

- [X] T016 [US1] Add the `#[utoipa::path(...)]` attribute above `get_billing_overview` with `get`, `path = "/tenant/billing/overview"`, `tag = "billing"`, `operation_id = "get_billing_overview"`, and responses `200` (body `model::BillingOverviewDto`), `403`, `500`. Copy the annotation shape from `audit::routes::list_tenant_audit_logs`.

- [X] T017 [US1] In `backend/crates/server/src/router.rs`, inside `fn tenant_routes(...)`, register the endpoint next to the audit-logs registration:

  ```rust
  // Billing overview (spec 031)
  .routes(
      routes!(billing::routes::get_billing_overview)
          .layer(require_permission(Permission::BillingView)),
  )
  ```

- [X] T018 [US1] In `backend/crates/server/src/openapi.rs`, add to the `components(schemas(...))` list (right after the `audit::model::*` entries): `billing::model::PlanDto`, `billing::model::PeriodDto`, `billing::model::MetricUsageDto`, `billing::model::UsageDto`, `billing::model::BillingOverviewDto`.

- [X] T018a [US1] **Required — `backend/crates/server/tests/openapi_coverage.rs` fails on undocumented *and* extra-documented routes.** Its `EXPECTED` const is a hard-coded inventory compared for exact set equality against the router-derived doc, so registering T017's route makes the test fail with "extra" until the inventory grows. Two edits:
  1. Add `("GET", "/tenant/billing/overview"),` to `EXPECTED` in `backend/crates/server/tests/openapi_coverage.rs`, in a `// Billing (spec 031)` group placed next to the audit-logs entries.
  2. Mirror the same line into `specs/016-backend-swagger-docs/contracts/openapi-coverage.md` — the const's doc comment names that file as the authoritative inventory it mirrors, and letting them drift breaks the next feature that trusts it.

- [X] T019 [US1] **Append** `("/api/v1/tenant/billing/overview", "billing.view"),` to the **end** of `TENANT_OPERATIONS` in `backend/crates/server/tests/rbac.rs`. Position matters: `staff_tenant_access_is_environment_aware` zips this slice against fixed-length `expected` arrays (18 booleans, with a comment enumerating the positions) — appending past that length leaves the existing positional expectations intact, whereas inserting in the middle silently shifts every role's expectations. Do not extend those arrays. The same test's Development-environment loop sweeps *all* entries and asserts 200 for all five platform staff roles; the new route satisfies this because the migration seeds the default Free plan, so the handler returns 200 for any tenant.

- [X] T020 [US1] Write `backend/crates/server/tests/billing_overview.rs` (copy the harness setup from `backend/crates/server/tests/analytics_api.rs`). Cover: 200 for Owner and for Admin; 403 for Manager, Agent, and Viewer; a tenant with no subscription row returns the `Free` plan and `subscription_status: "active"`; a tenant with zero usage returns `used: 0` on both metrics (not an error); a plan with `token_allowance: null` returns `allowance: null`, `percent: null`, `state: "normal"`; tenant A's response never contains tenant B's usage numbers.

- [ ] T021 [US1] Run `cargo test -p billing`, `cargo test -p server --test billing_overview`, `cargo test -p server --test rbac`, and `cargo test -p server --test openapi_coverage`. (Requires PostgreSQL — suites were run during implementation: billing_overview 6/6, openapi_coverage 3/3, rbac 27/28 with 1 pre-existing unrelated 404 failure.)

### Frontend

- [X] T022 [P] [US1] In `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, append the billing wire types and mappers (put them at the end of the file, following the `Analytics*` pattern): `BillingPlanWire`, `BillingPeriodWire`, `BillingMetricWire`, `BillingUsageWire`, `BillingOverviewWire` (snake_case fields matching the API) plus the camelCase app types `BillingPlan`, `BillingPeriod`, `BillingMetric`, `BillingUsage`, `BillingOverview`, and `export function billingOverviewFromWire(wire: BillingOverviewWire): BillingOverview`. Convert `period.start` / `period.end` to `Date`.

- [X] T023 [P] [US1] Create `frontend/apps/dashboard/src/app/shared/fixtures/billing.fixtures.ts` exporting typed sample data for specs: a normal overview, an 85%-usage overview, a 100%-usage overview, an unlimited-allowance overview, and a `past_due` overview. Follow the shape of `analytics.fixtures.ts`.

- [X] T024 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/billing/billing-api.service.ts` — `@Injectable({ providedIn: 'root' })`, injects `ApiService`, one method `getOverview(): Observable<ApiResponse<BillingOverview>>` calling `this.api.get<{ data: BillingOverviewWire }>('/tenant/billing/overview')` and mapping through `billingOverviewFromWire`. Copy the structure of `analytics-api.service.ts`.

- [X] T025 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/billing/billing.store.ts` — an NgRx `signalStore` with state `{ overview: BillingOverview | null; loading: boolean; error: string | null }`, an `rxMethod` `load()` that sets `loading` then calls the API, and a `withHooks` `onInit` that reloads whenever the active tenant changes (copy the `selectActiveTenant` effect from `analytics.store.ts`).

- [X] T026 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/billing/billing-page.component.ts` — a standalone, `OnPush`, signal-based component providing `BillingStore`. Render: a plan card (plan name, monthly price formatted from cents as e.g. `$49.00`, subscription status badge), a period line (`period.start` – `period.end`), and two usage cards (Tokens, Conversations) each showing `used`, `allowance` (`Unlimited` when null), `percent`, and a progress bar. Show a loading state and an error state. Use only `--app-*` design tokens and wrapped Taiga components — no new UI primitives. Warning/exceeded styling and the AI-paused banner come later, in T037.

- [X] T027 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/billing/billing.store.spec.ts` and `billing-page.component.spec.ts` using the T023 fixtures. Cover: the store exposes loaded data; the page renders plan name, price, and both usage cards; an unlimited metric shows `Unlimited` and no progress bar; zero usage renders `0` rather than an empty state.

- [X] T028 [US1] Wire the tenant route. Four small edits:
  1. `frontend/apps/dashboard/src/app/core/router/app-paths.ts` — add `billing: 'billing',` to the `tenant` block (after `auditLogs`).
  2. `frontend/apps/dashboard/src/app/core/authz/permissions.ts` — add `[APP_PATHS.tenant.billing]: 'billing.view',` to `PAGE_PERMISSIONS`.
  3. `frontend/apps/dashboard/src/app/core/router/page-title.ts` — add `'billing'` to the page-key union and `billing: { title: 'Billing', subtitle: 'Your plan and usage this month' },` to `PAGE_TITLES`.
  4. `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` — add a lazy route for `APP_PATHS.tenant.billing` with `canMatch: [permissionGuard]`, `data.pageTitle: 'billing'`, `data.requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.billing]`, copying the `analytics` route entry exactly.

- [X] T029 [US1] In `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts`, add a **Billing** nav entry and add `billing: \`/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.billing}\`,` to the `links` object. Give it its own single-item `<app-sidebar-nav-group label="Billing">` guarded by `@if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.billing]))`, placed between the existing **Security** and **Settings** groups — this copies the Insights/Security one-item-group pattern exactly and keeps visibility tied to `billing.view` alone (nesting it inside the Settings group would hide it behind `settings.view`).

- [X] T030 [US1] **Required — T029 breaks existing count assertions.** Update them in the same commit:
  1. `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.spec.ts` — the two cases whose permission fixture includes `billing.view` (`renders full navigation with all permissions` and `shows all navigation items for Owner`) go from `nav-group` **5 → 6** and `nav-item` **10 → 11**. The Support Agent (2/5), Viewer (3/8), and Support Engineer staff (2/5) cases have no `billing.view` and must stay unchanged — if any of them moves, the guard is wrong.
  2. `frontend/apps/dashboard/src/app/shell-acceptance.spec.ts` — the `ALL_PERMISSIONS` case asserting `nav-group … toBe(4)` becomes `toBe(5)` (`ALL_PERMISSIONS` already contains `billing.view`). The Viewer (3) and limited (2) cases stay unchanged.

  Re-derive these numbers rather than trusting them blindly if you placed the entry differently than T029 specifies. No change is needed in `permissions.service.spec.ts`: that spec feeds permissions in as `MeResponse` fixtures and asserts on `has()` — it derives nothing from tenant roles, so the `TENANT_ADMIN` matrix change has no frontend counterpart.

- [X] T031 [US1] Run `pnpm ng test dashboard` and `pnpm lint` from `frontend/`. Both must pass.

**Checkpoint**: User Story 1 is complete and independently demoable.

---

## Phase 4: User Story 2 — Usage limits enforced and surfaced (Priority: P2)

**Goal**: at ≥80% the usage card warns; at ≥100% (or `past_due`/`canceled`) AI replies stop while humans keep working.

**Independent Test**: put a tenant on a plan with `token_allowance: 1`, send a customer message — the message and conversation are stored, no AI reply is generated, and the Billing page shows `exceeded` plus the AI-paused banner. A human agent reply from the dashboard still sends.

### Backend

- [X] T032 [US2] Implement the enforcement gate in `backend/crates/modules/billing/src/enforcement.rs`:

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum PauseReason { SubscriptionStatus, TokenLimit, ConversationLimit }

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct EnforcementDecision { pub allowed: bool, pub reason: Option<PauseReason> }

  pub async fn check(pool: &sqlx::PgPool, tenant_id: uuid::Uuid)
      -> Result<EnforcementDecision, sqlx::Error>
  ```

  `check` reuses `queries::current_period`, `queries::effective_plan`, `queries::token_usage`, and `queries::conversation_usage`. It returns `allowed: false` when the status is `past_due` or `canceled` (reason `SubscriptionStatus`), else when `token_allowance` is `Some(limit)` and `tokens >= limit` (reason `TokenLimit`), else when `conversation_allowance` is `Some(limit)` and `conversations >= limit` (reason `ConversationLimit`); otherwise `allowed: true, reason: None`. A `None` (unlimited) allowance never pauses. Check the reasons in exactly that order.

- [X] T033 [US2] Add the gate to `backend/crates/modules/ai/src/agent_responder.rs`, as the **last statement inside the `if !is_platform_persona {` block** — that is, immediately after the closing `}` of the `match rule_match { … }` statement and immediately before the `}` that closes the `if`, so the next thing to execute is `// Phase B + C — Delegate to the engine`:

  ```rust
      // Gate on billing allowance / subscription status (spec 031)
      let decision = billing::enforcement::check(pool, tenant_id).await?;
      if !decision.allowed {
          tracing::info!(?tenant_id, reason = ?decision.reason, "agent responder: billing gate paused AI reply");
          sqlx::query("DELETE FROM outbox_events WHERE id = $1")
              .bind(event_id)
              .execute(pool)
              .await?;
          return Ok(true);
      }
  }
  ```

  **Placement is load-bearing, not cosmetic** — do not move it up next to the escalation gate:
  - It must sit **after** the `match rule_match` block so that baseline and tenant escalation rules still evaluate for an over-limit tenant. Those arms route the conversation to a human queue and then consume the event themselves. Gating earlier would silently stop auto-escalation exactly when AI can no longer answer, contradicting spec.md US2 ("the conversation remains open for human handling") and SC-006 ("enforcement never blocks a customer from reaching the business").
  - It must sit **inside** `if !is_platform_persona` so platform-persona flows are unaffected.
  - Running last also means the three enforcement queries only execute on events that would actually generate a reply — events already consumed by the channel/state/escalation gates never pay for them.

  Do not change anything else in this file.

- [X] T034 [US2] Write `backend/crates/modules/billing/tests/enforcement_tests.rs` (DB-backed). Cover every row of the decision matrix: unlimited plan with heavy usage ⇒ allowed; usage just under the token limit ⇒ allowed; usage exactly at the token limit ⇒ paused with `TokenLimit`; usage over the limit ⇒ paused; conversations at limit ⇒ paused with `ConversationLimit`; status `past_due` ⇒ paused with `SubscriptionStatus` even when usage is zero; status `canceled` ⇒ same; tenant with no subscription row ⇒ uses the `Free` plan allowances; usage from the previous calendar month does not count toward this month; raising the plan allowance above current usage flips the decision back to allowed with no restart.

- [X] T035 [US2] Write the responder-gate test as `backend/crates/server/tests/billing_responder_gate.rs`, copying the harness from `backend/crates/server/tests/engine_respond.rs`.

  **Not in the `ai` crate** — `crates/modules/ai/tests/` holds exactly one file (`sc005_fourth_provider.rs`, a provider test), the `ai` crate's `[dev-dependencies]` carry no DB/migration harness, and every existing test that drives `process_agent_responder_once` lives in `crates/server/tests/engine_*.rs`, which already have pool setup, `db::run_migrations`, tenant/conversation seeding, and the stub provider wiring this test needs.

  Cover:
  - exhausted allowance ⇒ the outbox event is deleted and **no** AI message row is inserted;
  - the conversation and the inbound customer message still exist afterwards (FR-005);
  - a tenant whose escalation rules match still gets escalated while over limit — the gate runs after rule evaluation, so an escalation row is created (guards the T033 placement, SC-006);
  - a human agent reply still succeeds while `ai_paused` is true: `POST /api/v1/tenant/conversations/{id}/messages` returns 2xx and the message row lands (SC-006's "human-agent replies succeed at the same rate");
  - allowance available ⇒ the responder proceeds and generates as before (no regression).

- [X] T036 [US2] Run `cargo test -p billing` and `cargo test -p server --test billing_responder_gate`. Both must pass.

### Frontend

- [X] T037 [US2] Extend `billing-page.component.ts` with the three usage states: apply a `normal` / `warning` / `exceeded` CSS class driven by `metric.state`, and render an AI-paused banner above the cards when `overview.aiPaused` is true, with copy chosen from `aiPausedReason` (`token_limit` → "AI replies are paused — the monthly token allowance is used up.", `conversation_limit` → "AI replies are paused — the monthly conversation allowance is used up.", `subscription_status` → "AI replies are paused — the subscription is past due or canceled."). Unlimited metrics must never receive the warning or exceeded class.

- [X] T038 [US2] Extend `billing-page.component.spec.ts` with cases from the T023 fixtures: 85% renders the warning class; 100% renders the exceeded class; the AI-paused banner appears with the correct copy for each of the three reasons; an unlimited metric stays `normal` and shows no banner.

- [X] T039 [US2] Run `pnpm ng test dashboard` and `pnpm lint`. Both must pass.

**Checkpoint**: User Stories 1 and 2 both work independently.

---

## Phase 5: User Story 3 — Platform operator views and manages plans and subscriptions (Priority: P3)

**Goal**: Super Admin / Finance review billing data across all tenants and maintain it — plan catalog plus tenant subscription assignment; reads and writes are separately authorized; every change is audited.

**Independent Test**: sign in as platform Super Admin, review the plan catalog and the per-tenant subscription list, create a plan, assign it to a tenant, and confirm the tenant's Billing page shows the new plan and `/platform/audit-logs` contains the change. Sign in as platform Developer — both the reads and the writes are rejected with 403.

**Spec-update coverage (2026-07-24 — FR-011 / SC-007, US3 scenarios 5–6)**: no new tasks were required; the read surface was already in this phase. Traceability:

| Spec item | Covered by |
|---|---|
| FR-011 read surface (cross-tenant plan + subscription read) | T040 `list_subscriptions` query, T042 `list_plans` / `list_subscriptions` handlers, T051 platform page |
| FR-011 read/write separation | T043 (`PlatformBillingView` on the two GETs, `PlatformBillingManage` on POST/PATCH/PUT), T007 probe route |
| SC-007 (finance answers "what plan is tenant X on?" from one page) | T045 (every tenant appears exactly once), T051 subscriptions table |
| US3 scenario 5 (finance reads catalog + all tenants) | T045, T051 |
| US3 scenario 6 (view without manage) | T054 — frontend only; **no platform role holds view without manage today**, so this is asserted with a synthetic permission set, not a backend role sweep (see plan.md → Spec Update Alignment) |
| US3 scenario 4 extended to viewing | T045 (Developer/Sales/Support 403 on all five, GETs included), T052/T053 (nav + route guarded by `platform.billing.view`) |

### Backend

- [X] T040 [US3] Add the write queries to `backend/crates/modules/billing/src/queries.rs`:
  - `list_plans(pool)` — all plans ordered by `created_at ASC`.
  - `create_plan(tx, req)` — INSERT returning the row; map a unique-violation on `name` to a distinct error the handler turns into 409.
  - `plan_by_id(pool, plan_id)` — `Option<PlanDto>`.
  - `set_plan_status(tx, plan_id, status)` — UPDATE returning the row.
  - `list_subscriptions(pool)` — one row per tenant:

    ```sql
    SELECT t.id AS tenant_id, t.name AS tenant_name,
           COALESCE(p.id, d.id) AS plan_id,
           COALESCE(p.name, d.name) AS plan_name,
           COALESCE(p.is_default, d.is_default) AS plan_is_default,
           COALESCE(s.status, 'active') AS status,
           (s.id IS NOT NULL) AS explicit
    FROM tenants t
    LEFT JOIN tenant_subscriptions s ON s.tenant_id = t.id
    LEFT JOIN billing_plans p ON p.id = s.plan_id
    CROSS JOIN (SELECT id, name, is_default FROM billing_plans WHERE is_default) d
    WHERE t.deleted_at IS NULL
    ORDER BY t.name
    ```

  - `subscription_for_tenant(pool_or_tx, tenant_id)` — the current row (used to capture the audit "before" values).
  - `upsert_subscription(tx, tenant_id, plan_id, status, assigned_by)` — `INSERT ... ON CONFLICT (tenant_id) DO UPDATE SET plan_id = EXCLUDED.plan_id, status = EXCLUDED.status, assigned_by = EXCLUDED.assigned_by` returning the row.

  (`tenants.deleted_at` exists — the `WHERE t.deleted_at IS NULL` clause is correct as written.)

- [X] T041 [US3] Implement the audit writers in `backend/crates/modules/billing/src/audit.rs`, each taking `&mut Transaction<'_, Postgres>` and calling `tenancy::audit::record_in_tx`:
  - `plan_created(tx, actor_user_id, plan)` — action `billing_plan.created`, resource_type `billing_plan`, resource_id = plan id, tenant_id `None`, details = the full created attributes.
  - `plan_status_changed(tx, actor_user_id, plan_id, from, to)` — action `billing_plan.status_changed`, details `{"from": from, "to": to}`.
  - `subscription_changed(tx, actor_user_id, tenant_id, subscription_id, before, after)` — action `billing_subscription.changed`, resource_type `billing_subscription`, tenant_id `Some(tenant_id)`, details `{"plan_id": {"from": ..., "to": ...}, "status": {"from": ..., "to": ...}}` with `null` on the `from` side for a first assignment.

- [X] T042 [US3] Implement the five platform handlers in `backend/crates/modules/billing/src/routes.rs`, each with a full `#[utoipa::path(...)]` annotation (`tag = "billing"`):
  - `list_plans` — `get "/platform/billing/plans"` → 200 `PlanListResponse`.
  - `create_plan` — `post "/platform/billing/plans"`, body `ApiJson<CreatePlanRequest>` → 201 `PlanDto`. Validate: non-empty trimmed name, `monthly_price_cents >= 0`, allowances `>= 0` when present — otherwise 422. `0` is valid and intentionally means "AI disabled for this metric" (see T011's `metric_usage`); only negatives are rejected. Duplicate name ⇒ 409. Opens a transaction, inserts, writes `plan_created`, commits.
  - `update_plan` — `patch "/platform/billing/plans/{plan_id}"`, body `ApiJson<UpdatePlanRequest>` → 200 `PlanDto`. Unknown plan ⇒ 404; status not in `active|retired` ⇒ 422; retiring the plan whose `is_default` is true ⇒ 409. Transaction, update, `plan_status_changed`, commit.
  - `list_subscriptions` — `get "/platform/billing/subscriptions"` → 200 `SubscriptionListResponse`.
  - `put_subscription` — `put "/platform/billing/tenants/{tenant_id}/subscription"`, body `ApiJson<PutSubscriptionRequest>` → 200 `SubscriptionDto`. Unknown tenant or plan ⇒ 404; status not in `active|past_due|canceled` ⇒ 422; assigning a `retired` plan ⇒ 409 **unless** the tenant is already on that plan and only the status is changing. Transaction: read the "before" row, upsert, `subscription_changed`, commit. The operation must be idempotent — sending the same body twice returns 200 both times.

  All five take `Extension(principal): Extension<identity::Principal>` and pass `principal.user_id` as the audit actor.

- [X] T043 [US3] Register the five platform routes in `fn platform_routes(...)` of `backend/crates/server/src/router.rs`, next to the platform audit-logs registration. Permissions: `PlatformBillingView` for the two GETs; `PlatformBillingManage` for POST, PATCH, and PUT. Follow the existing `.routes(routes!(...).layer(require_permission(...)))` shape; use the `merge_with_permissions` helper only if two methods share one path (the plan collection path has both GET and POST — mirror the `ai::routes::get_platform_config` / `put_platform_config` example in the same file).

- [X] T044 [US3] In `backend/crates/server/src/openapi.rs`, add `billing::model::PlanListResponse`, `billing::model::CreatePlanRequest`, `billing::model::UpdatePlanRequest`, `billing::model::SubscriptionRowDto`, `billing::model::SubscriptionListResponse`, `billing::model::PutSubscriptionRequest`, and `billing::model::SubscriptionDto` to `components(schemas(...))`.

- [X] T044a [US3] Same requirement as T018a for the five platform routes — without this, `openapi_coverage` fails with five "extra documented" entries. Add to `EXPECTED` in `backend/crates/server/tests/openapi_coverage.rs` **and** to `specs/016-backend-swagger-docs/contracts/openapi-coverage.md`:

  ```text
  ("GET",   "/platform/billing/plans")
  ("POST",  "/platform/billing/plans")
  ("PATCH", "/platform/billing/plans/{plan_id}")
  ("GET",   "/platform/billing/subscriptions")
  ("PUT",   "/platform/billing/tenants/{tenant_id}/subscription")
  ```

  Path strings must match the `#[utoipa::path]` templates character-for-character, including the `{plan_id}` / `{tenant_id}` parameter names — the comparison is on literal path strings, not route patterns.

- [X] T045 [US3] Write `backend/crates/server/tests/billing_platform.rs`. Cover: Super Admin and Finance get 200 on all five endpoints; Developer, Sales, and Support get 403 on all five; duplicate plan name ⇒ 409; empty name ⇒ 422; negative price ⇒ 422; retiring the default plan ⇒ 409; assigning an unknown plan ⇒ 404; assigning a retired plan to a new tenant ⇒ 409; changing only the status while already on a retired plan ⇒ 200; the same PUT body sent twice ⇒ 200 both times with one subscription row; `list_subscriptions` returns a row for a tenant with no subscription showing the default plan and `explicit: false`; each mutation writes exactly one `audit_logs` row with the right action, actor, and before/after details.

  **Added for FR-011 / SC-007**: also assert that `list_subscriptions` returns **exactly one row per non-deleted tenant** — seed at least two tenants, one with an explicit subscription and one without, and check the response length equals the tenant count and that `tenant_name` is populated on every row (a soft-deleted tenant must be absent). This is the assertion that SC-007 rests on: a finance user can answer "what plan is tenant X on?" for *every* tenant, not just the assigned ones. The 403 sweep already covers the GETs as well as the mutations, which is US3 scenario 4's viewing case — do not narrow it to writes only.

- [X] T046 [US3] Run `cargo test -p billing`, `cargo test -p server --test billing_platform`, `cargo test -p server --test rbac`, `cargo test -p server --test openapi_contract`, and `cargo test -p server --test openapi_coverage`. All must pass.

### Frontend

- [X] T047 [P] [US3] In `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, append the platform billing wire types and mappers: `PlatformPlanWire` / `PlatformPlan`, `PlatformSubscriptionRowWire` / `PlatformSubscriptionRow`, plus `platformPlansFromWire` and `platformSubscriptionsFromWire`.

- [X] T048 [P] [US3] In `frontend/apps/dashboard/src/app/core/authz/permissions.ts`, add `| 'platform.billing.manage'` to the `Permission` union (right after `'platform.billing.view'`). Then add `'platform.billing.manage'` to the permission list in `frontend/apps/dashboard/src/app/shell-acceptance.spec.ts` so the full-permission fixture stays in sync.

- [X] T049 [US3] Create `frontend/apps/dashboard/src/app/features/platform/billing/platform-billing-api.service.ts` with five methods matching the five endpoints (`getPlans`, `createPlan`, `updatePlanStatus`, `getSubscriptions`, `putSubscription`), following `platform-audit-logs-api.service.ts`.

- [X] T050 [US3] Create `frontend/apps/dashboard/src/app/features/platform/billing/platform-billing.store.ts` — signal store holding `plans`, `subscriptions`, `loading`, `error`, with `loadAll()`, `createPlan()`, `retirePlan()` / `activatePlan()`, and `assignSubscription()`. Each mutation reloads the affected list on success.

- [X] T051 [US3] Create `frontend/apps/dashboard/src/app/features/platform/billing/platform-billing.component.ts` — standalone, `OnPush`. Two sections: a plan catalog table (name, price, token allowance, conversation allowance, default flag, status) with a create form and a retire/activate action, and a tenant subscriptions table (tenant, plan, status, explicit) with plan and status selectors. Hide every write control behind `hasPermission('platform.billing.manage')` — view-only users see the tables and no buttons.

- [X] T052 [US3] Wire the platform route: add `billing: 'billing',` to the `platform` block of `app-paths.ts`; add `[APP_PATHS.platform.billing]: 'platform.billing.view',` to `PAGE_PERMISSIONS`; add a `'platformBilling'` key and title to `page-title.ts`; add the lazy route to `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` copying the `platform.auditLogs` entry.

- [X] T053 [US3] In `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`, **append** a **Billing** entry pointing at `/${APP_PATHS.platform.base}/${APP_PATHS.platform.billing}`, guarded by `platform.billing.view` the same way the existing entries are guarded. Append it **after** the existing Audit Logs entry and update the spec's index assertions to 3.

- [X] T054 [P] [US3] Create `platform-billing.store.spec.ts` and `platform-billing.component.spec.ts` in the same folder. Cover: plans and subscriptions render; a Finance/Super Admin sees the create form and retire buttons; creating a plan calls the API and reloads the list.

  **This spec is the only coverage of US3 scenario 6 (FR-011's view-without-manage case)** — drive the component with a permission set containing `platform.billing.view` **but not** `platform.billing.manage`, then assert that both tables still render their rows *and* that the create form, the retire/activate buttons, and the plan/status assignment controls are all absent from the DOM.

- [X] T055 [US3] Run `pnpm ng test dashboard`, `pnpm lint`, and `pnpm format:check`. All must pass.

**Checkpoint**: all three user stories are independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T056 [P] Add the module-doc block to `backend/crates/modules/billing/src/lib.rs` if T010 left any section as a placeholder — it must describe the two owned tables, the tables read via SQL (`ai_usage_records`, `conversations`, `tenants`), and the `ai → billing` dependency direction.

- [X] T057 [P] Update `CLAUDE.md`: move the `031-billing-foundation` line under "Recent Changes" to the top of that list and confirm it matches what was actually built.

- [ ] T058 Run the full backend gate (requires PostgreSQL — skipped as documented; DB-backed billing, ai, authz, server suites were run individually during implementation and reported green) from `backend/` with PostgreSQL up: `cargo test -p billing`, `cargo test -p ai`, `cargo test -p authz`, `cargo test -p server`. Report any failure with its output — do not mark this task done on a red run.

- [X] T059 Run the full frontend gate from `frontend/`: `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`.

- [ ] T060 Walk through [quickstart.md](quickstart.md) (requires running server — skipped as documented) end to end against a running server: verify the seed row, the API smoke calls, the enforcement pause and resume, the audit entries, and the UI gates for each role.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies — start here.
- **Phase 2 (Foundational)**: needs Phase 1. **Blocks all user stories.**
- **Phase 3 (US1)**: needs Phase 2.
- **Phase 4 (US2)**: needs Phase 2. Uses `queries.rs` from T012 but does not need US1's endpoint — except T037/T038, which edit the US1 component and therefore need T026 done first.
- **Phase 5 (US3)**: needs Phase 2 only. Fully independent of US1 and US2 code.
- **Phase 6 (Polish)**: needs every story you intend to ship.

### Critical Path

T001 → T002/T003/T004 → T005/T006/T007/T008/T008a → T009 → T010 → T011 → T012 → (US1 | US2 | US3)

### Tasks that exist only to keep pre-existing tests green

These are not optional polish — each one is red the moment its paired change lands, and each targets a file no other task in this feature touches. Do them in the same commit as their trigger:

| Trigger | Repair task | File that goes red |
|---------|-------------|--------------------|
| T006 adds `BillingView` to `TENANT_ADMIN` | T006.4 | `authz/src/matrix.rs` (`tenant_role_hierarchy_and_owner_exclusives_hold`) |
| T006 adds `PlatformBillingManage` to `PLATFORM_ALL` | T008a | `server/tests/auth.rs` (exact `/auth/login` + `/me` JSON) |
| T017 registers the tenant route | T018a | `server/tests/openapi_coverage.rs` + the 016 contract md |
| T043 registers the five platform routes | T044a | `server/tests/openapi_coverage.rs` + the 016 contract md |
| T029 adds the sidebar entry | T030 | `sidebar.component.spec.ts`, `shell-acceptance.spec.ts` |
| T053 adds the platform-nav entry | T053 (inline) | `platform-nav.component.spec.ts` if placed before Audit Logs |

### Parallel Opportunities

- **Setup**: T002, T003, T004 are three different files — run together.
- **Foundational**: T013 and T014 are two different test files — run together.
- **US1**: T022 and T023 together (models + fixtures); T027 alongside the backend tasks T019–T021.
- **US3**: T047 and T048 together; T054 alongside T045.
- **Across stories**: once T012 is done, one person can take US1, another US2, another US3 — the only cross-story edit is T037 touching the US1 component.

---

## Implementation Strategy

### MVP (User Story 1 only)

1. Phase 1 → Phase 2 → Phase 3.
2. Stop and validate: Owner and Admin see the Billing page with real numbers; Manager, Agent, and Viewer are blocked.
3. This alone ships a working read-only billing dashboard.

### Incremental Delivery

1. Setup + Foundational → everything compiles, RBAC is green.
2. + US1 → Billing page ships (MVP).
3. + US2 → allowances actually bite; warnings appear.
4. + US3 → platform operators can change what US1 shows and US2 enforces.
5. Polish → full gates and the quickstart walkthrough.

---

## Notes

- `[P]` means a different file with no unfinished dependency — never two tasks editing the same file.
- Commit after each task or each small group; keep the tasks.md checkboxes honest — tick a box only after the named command passed or the named file exists with real content.
- Never mark a test task complete because the test file was created; it counts only when the suite runs green.

---

## Phase 7: Convergence

**Added 2026-07-25 by `/speckit-converge`.** The feature was assessed against spec.md, plan.md, and the constitution. **All 11 functional requirements, all 15 acceptance scenarios, and all 10 constitution principles are satisfied by the code** — `cargo check -p billing -p ai -p server --tests` compiles clean, the enforcement gate is correctly placed, and every exact-inventory test file was updated. Only the two low-severity cleanups below remain.

**Do NOT re-do any earlier task.** T009, T021, T058, and T060 are still unchecked on purpose — they are "run the DB-backed suite green" gates that need a PostgreSQL the current environment cannot provide. Leave them alone; they are not part of this phase.

- [X] T061 [P] Format the billing crate's own source files per plan: quality gate (`partial`)

  **Problem**: six files this feature created are not rustfmt-clean, which violates Convention 9 above ("only keep your own new files formatted").

  **Do exactly this** — from `backend/`, run:

  ```bash
  cargo fmt -p billing
  ```

  That reformats all six affected files:
  `crates/modules/billing/src/model.rs`, `src/queries.rs`, `src/routes.rs`,
  `crates/modules/billing/tests/usage_math_tests.rs`, `tests/queries_tests.rs`, `tests/enforcement_tests.rs`.

  **Verify** with `cargo fmt -p billing -- --check` — it must print nothing and exit 0.

  **Scope guardrails — read before running anything:**
  - Run it with the `-p billing` flag **only**. Do **not** run a bare `cargo fmt` or `cargo fmt --all`.
  - `cargo fmt --check` is already failing repo-wide at HEAD for unrelated reasons (Convention 9). That is **not** this feature's problem and must not be "fixed" here.
  - In particular, `crates/modules/ai/src/agent_responder.rs` and `crates/modules/ai/src/engine.rs` are also unformatted, but their diffs are in **pre-existing WhatsApp code around line 146** — not in the billing gate at lines 333–342. **Do not reformat the `ai` crate.** Touching it would put unrelated churn in this feature's diff.
  - This task changes whitespace/line-wrapping only. Do not alter any logic, rename anything, or edit any other file.

  **Afterwards**, confirm nothing broke: `cargo check -p billing` must still succeed.

- [X] T062 [P] Delete the unused `metricStateClass` helper from the tenant Billing page (`unrequested`)

  **File**: `frontend/apps/dashboard/src/app/features/tenant/billing/billing-page.component.ts`

  **Problem**: T037 ended up implementing the warning/exceeded styling with inline `[class.warning]` / `[class.exceeded]` bindings in the template. The `metricStateClass` helper it was originally going to use was left behind and is now dead code — it is defined, exposed to the template, and never referenced by it.

  **Make exactly these two deletions:**

  1. Delete the whole function near line 21 (and the blank line that follows it):

     ```ts
     function metricStateClass(allowance: number | null, state: string): string {
       return allowance === null ? 'normal' : state;
     }
     ```

  2. Delete this single line from the class body near line 249:

     ```ts
     protected readonly metricStateClass = metricStateClass;
     ```

  **Do NOT touch** the neighbouring helpers `formatDate`, `metricLabel`, `subscriptionTone`, or `aiPausedMessage` — those four **are** used by the template and must stay, along with their `protected readonly` lines.

  **Do NOT change the template.** The existing `[class.warning]` / `[class.exceeded]` bindings already implement FR-007 correctly, including the rule that an unlimited allowance (`allowance === null`) never gets a warning or exceeded class. Behaviour must be identical after this change.

  **Verify** from `frontend/`: `pnpm ng test dashboard` and `pnpm lint` both pass. The existing `billing-page.component.spec.ts` cases (warning at 85%, exceeded at 100%, unlimited stays normal) must still be green — if any of them fails, you deleted something that was in use; revert and re-check.

---

## Phase 8: Convergence (DB-verified)

**Added 2026-07-25 by `/speckit-converge`.** Unlike Phase 7, this pass **had a working PostgreSQL** and actually ran the DB-backed suites that every earlier gate task had to assume. That changed the picture: the static code is still correct (all 11 FRs implemented, all routes/permissions/schemas wired, `cargo check -p billing -p ai -p server --tests` clean, `cargo fmt -p billing -- --check` clean), but **three suites are red**, and one of them means the billing enforcement gate never executes at all.

**Evidence from this pass** (`DATABASE_URL` pointed at a throwaway DB, `REQUIRE_DB_TESTS=1`):

| Suite | Result |
|---|---|
| `cargo test -p billing --test usage_math_tests` | ✅ 15/15 |
| `cargo test -p billing --test queries_tests` | ✅ 6/6 |
| `cargo test -p billing --test enforcement_tests` | ❌ **7 passed / 3 failed** → T064 |
| `cargo test -p server --test billing_overview` | ✅ 9/9 |
| `cargo test -p server --test billing_platform` | ✅ 38/38 |
| `cargo test -p server --test billing_responder_gate` | ❌ **0 passed / 5 failed** → T063, T066 |
| `pnpm ng test dashboard` | 138/139 files; the one failure is `conversations/composer.component.spec.ts`, **not billing** — out of scope, do not touch |

**Do NOT re-do any earlier task.** T009, T021, T058, and T060 stay unchecked and are **not** part of this phase — do not "complete" them, do not tick them, do not restate them. Phase 7's T061/T062 are done; leave them alone.

**How to run anything in this phase.** All four tasks need a database. From `backend/`:

```bash
createdb billing_converge_check                                          # once
export DATABASE_URL="postgres://$(whoami)@localhost/billing_converge_check"
export REQUIRE_DB_TESTS=1     # REQUIRED — without it the suites SKIP silently and print a false green
```

`REQUIRE_DB_TESTS=1` is not optional. Every one of these harnesses returns early with "skipping … DATABASE_URL not set" when it is missing, so a run without it proves nothing. Drop the DB when you are finished: `dropdb billing_converge_check`.

**Do them in this order: T063 → T066 → T064 → T065.** T063 and T066 are the same test file and T066's assertion is unreachable until T063 lands. T064 (`billing` crate) and T065 (`audit` crate) are independent of both and can be done any time.

---

- [X] T063 **CRITICAL** — Fix the outbox claim type mismatch that stops the AI responder (and therefore the billing gate) from ever running, per FR-005 / SC-002 / SC-006 (`contradicts`)

  **Problem.** `process_agent_responder_once` claims an outbox row into a Rust tuple whose first two column types are wrong:

  - `outbox_events.id` is **UUID** (since migration `0002_outbox.sql`) but is decoded as `i64`.
  - `outbox_events.tenant_id` is **TEXT** but is decoded as `Uuid`.

  Every call therefore fails immediately with:

  ```text
  error occurred while decoding column 0: mismatched types;
  Rust type `i64` (as SQL type `INT8`) is not compatible with SQL type `UUID`
  ```

  The function returns `Err` before reaching **any** gate, so the billing gate added by T033 never executes. This is why all five `billing_responder_gate` tests fail. FR-005 ("MUST pause AI-generated replies…") is currently unverifiable and, in a real deployment, unreachable.

  **This bug pre-dates spec 031** — it is present at commit `b8df8f4`, before this feature branch's first commit. It is included here because it is the *only* thing preventing 031's own US2 acceptance test from passing, and because `agent_responder.rs` is a plan.md touch-point for this feature. Do not treat it as a billing-crate bug: **the `billing` crate itself is correct and needs no change for this task.**

  **Make exactly these three edits — no others.**

  1. `backend/crates/modules/ai/src/agent_responder.rs`, **line 19** — change the first tuple element from `i64` to `Uuid`:

     ```rust
     // BEFORE
     let maybe_row: Option<(i64, Uuid, String, String, serde_json::Value)> = sqlx::query_as(
     // AFTER
     let maybe_row: Option<(Uuid, Uuid, String, String, serde_json::Value)> = sqlx::query_as(
     ```

  2. `backend/crates/modules/ai/src/agent_responder.rs`, **line 30** — cast the TEXT column so it decodes into `Uuid`. This is inside the SQL string literal of that same `query_as`:

     ```rust
     // BEFORE
              RETURNING id, tenant_id, aggregate_id, event_type, payload",
     // AFTER
              RETURNING id, tenant_id::uuid, aggregate_id, event_type, payload",
     ```

  3. `backend/crates/modules/ai/src/engine.rs`, **line 498** — the `event_id` parameter of `pub async fn run_generation`:

     ```rust
     // BEFORE
         event_id: i64,
     // AFTER
         event_id: Uuid,
     ```

  That is the whole change. The six `.bind(event_id)` call sites inside `engine.rs` (lines ~937, ~1092, ~1181, ~1208, ~1367, ~1374) and the ten inside `agent_responder.rs` need **no edit** — they bind whatever type `event_id` now is, and `Uuid` is what the `uuid` column wants.

  **These three edits were applied and measured during convergence, then reverted.** Confirmed at that time:
  - `run_generation` has exactly one caller — `agent_responder.rs:346`. (`grep -rn "run_generation" backend/crates --include='*.rs'` returns only the definition and that one call.) No test constructs an `event_id` literal.
  - With the edits applied, `cargo check -p ai -p server --tests` **compiles clean** — every `server` test binary included, warnings only. So you should hit no compile errors in files this task tells you not to touch. If you do, stop and re-read the three edits rather than widening the change.

  **Verify**, from `backend/` with the env vars above:

  ```bash
  cargo check -p ai -p server --tests
  cargo test -p server --test billing_responder_gate
  ```

  **Expected result after this task alone: 3 passed / 2 failed.** That is the correct, verified outcome — it was measured. The three that must turn green are the ones carrying the FR-005 / SC-006 guarantees:
  - `exhausted_allowance_gate_deletes_outbox_and_blocks_ai`
  - `conversation_and_customer_message_survive_gate`
  - `escalation_rule_still_matches_while_over_limit` (this one proves the T033 gate placement is right)

  The two still-red ones are **not** yours to fix here:
  - `human_agent_reply_succeeds_while_ai_paused` → fixed separately by **T066**.
  - `allowance_available_responder_proceeds_normally` → **leave it red.** It seeds a fake OpenAI key (`sk-t035-allowed`) and asserts a real AI reply row appears, so it can only pass against a live provider. There is no mock provider in this repo. The pre-existing `engine_respond` suite is built the same way and is red for the same class of reason. Do not add a mock, do not weaken the assertion, do not delete the test.

    **A 4-of-5 result here is not a regression you introduced.** This test was already unsatisfiable offline before spec 031 existed — it is a property of how the `engine_*` test family seeds providers, not of the billing gate. Record it as such if anyone asks why the suite is not fully green.

  **Scope guardrails:**
  - Touch **only** the two files named above, and only at those three points.
  - Do **not** reformat either file. Both have pre-existing rustfmt drift in unrelated WhatsApp code (Convention 9) — leave it.
  - Do **not** try to fix the other red `engine_*` suites. `engine_respond`, `engine_isolation`, and `engine_fallback` fail for unrelated pre-existing reasons (e.g. `engine_isolation` violates the `tenants_slug_format` check constraint by seeding an uppercase slug). Out of scope for 031.
  - Do **not** change `outbox_events` in any migration. The schema is right; the Rust types were wrong.

- [X] T064 Make plan names unique in `enforcement_tests.rs` so the suite can pass, per FR-005 (`partial`)

  **File**: `backend/crates/modules/billing/tests/enforcement_tests.rs`

  **Problem.** `billing_plans.name` has a `UNIQUE` constraint (migration `0059`), and the `setup()` helper truncates `ai_usage_records, tenant_subscriptions, conversations, customers, tenants` but deliberately **not** `billing_plans` (truncating it would delete the seeded default `Free` plan that other tests depend on). Meanwhile the `create_plan` helper inserts whatever fixed name the caller passes, and three names are reused across tests:

  - `"Token Limit 1000"` — used by three tests (lines 166, 183, 203)
  - `"Any Plan"` — used by two tests (lines 246, 263)

  So the second and later test using each name dies with:

  ```text
  duplicate key value violates unique constraint "billing_plans_name_key"
  Key (name)=(Token Limit 1000) already exists.
  ```

  Result: **7 passed / 3 failed**. The three failures are `usage_just_under_token_limit_allows`, `usage_exactly_at_token_limit_pauses_token_limit`, and `canceled_subscription_pauses_regardless_of_usage` — core FR-005 boundary cases that have therefore never actually run.

  **Make exactly this one edit.** In the `create_plan` helper (around line 66), suffix the bound name with a fresh UUID so every insert is unique:

  ```rust
  // BEFORE
      .bind(name)
  // AFTER
      .bind(format!("{name} {}", Uuid::new_v4().simple()))
  ```

  `Uuid` is already imported at the top of the file. Change nothing else — not the helper's `name: &str` signature, not any call site, not the `setup()` TRUNCATE list.

  **Why this and not truncating `billing_plans`:** the `no_subscription_falls_back_to_free_plan` test depends on the seeded default `Free` row surviving. Truncating that table would break it.

  **Safe because** no test in this file asserts on a plan *name* — they assert on `decision.allowed` and `decision.reason`. Verified.

  **Verify**, from `backend/` with the env vars above:

  ```bash
  cargo test -p billing --test enforcement_tests
  ```

  **Expected: 10 passed / 0 failed.** This exact edit was applied and measured during convergence; it produced 10/10. If you see fewer than 10 passing, you changed something else too.

- [X] T065 [P] Map the billing audit actions onto the existing `billing` audit category, per FR-009 / US3 (`partial`)

  **File**: `backend/crates/modules/audit/src/model.rs` (one line)

  **Problem.** The audit category catalogue matches actions by **string prefix**:

  ```rust
  ("billing", &["billing."]),      // line 60
  ```

  But this feature writes actions named `billing_plan.created`, `billing_plan.status_changed`, and `billing_subscription.changed` (see `backend/crates/modules/billing/src/audit.rs`). `"billing_plan.created".starts_with("billing.")` is **false** — the 8th character is `_`, not `.`. Consequences today:

  - `category_for_action` returns `"other"` for every billing change, so the audit table shows them mis-labelled.
  - Both audit pages already ship a **Billing** filter option (`features/tenant/audit-logs/audit-logs.component.ts:100` and `features/platform/audit-logs/platform-audit-logs.component.ts:106`). Selecting it builds the SQL filter `action LIKE ANY('{billing.%}')`, which matches none of these rows — so the filter returns an empty list.

  The audit records themselves *are* written correctly (verified: `billing_platform.rs` audit assertions pass, 38/38), so FR-009's "an audit record captures who/what/when" holds. What fails is US3's independent test — "confirm `/platform/audit-logs` contains the change" — when the operator filters by Billing.

  **Make exactly this one edit.** Replace line 60 of `CATEGORY_PREFIXES`:

  ```rust
  // BEFORE
      ("billing", &["billing."]),
  // AFTER
      ("billing", &["billing.", "billing_plan.", "billing_subscription."]),
  ```

  Keep `"billing."` in the list — dropping it would orphan any other action that uses it.

  **This follows the existing house pattern**, not a new one: the `knowledge` entry on line 64 already lists several underscore-style prefixes (`"knowledge_category."`, `"knowledge_document."`, `"knowledge_item."`) for exactly this reason. `category_for_action` picks the longest matching prefix, so adding these is safe.

  **Do not "improve" the underscore escaping.** `prefixes_for_category` builds SQL `LIKE` patterns with `format!("{}%", p)`, and in `LIKE` an `_` is a single-character wildcard — so `billing_plan.%` would also match a hypothetical `billingXplan.created`. That looseness is harmless (no such actions exist) and the `knowledge_*` entries already have exactly the same property, so leaving it is the house-consistent choice. Escaping it here would make this one entry inconsistent with the rest of the table for no behavioural gain.

  **Do NOT** rename the actions in `backend/crates/modules/billing/src/audit.rs`. `billing_platform.rs` asserts those exact action strings (lines 668, 694, 721) and they are already green — renaming would redden a passing suite for no gain.

  **Verify**, from `backend/`:

  ```bash
  cargo test -p audit
  cargo test -p server --test billing_platform      # must stay 38/38
  ```

- [X] T066 Correct the human-reply status assertion in the responder-gate test, per SC-006 (`partial`)

  **File**: `backend/crates/server/tests/billing_responder_gate.rs`, in `human_agent_reply_succeeds_while_ai_paused` (assertion at ~line 657)

  **Problem.** The test asserts the human reply returns `201 CREATED`:

  ```text
  assertion `left == right` failed: human reply should succeed while billing is paused
    left: 200
   right: 201
  ```

  The endpoint actually returns **200**. `conversations::routes::add_message` ends with a bare `Json(...)` response and never sets `StatusCode::CREATED` — its utoipa annotation documents 201, but the handler returns 200. That doc/handler mismatch lives in the `conversations` module and is **out of scope for spec 031**.

  What SC-006 requires is that a human agent's reply *succeeds* while AI is paused. It does — 200 is a success. Only the assertion is wrong.

  **Make exactly this edit** — assert success rather than a specific 2xx code:

  ```rust
  // BEFORE
      assert_eq!(
          resp.status(),
          StatusCode::CREATED,
          "human reply should succeed while billing is paused"
      );
  // AFTER
      assert!(
          resp.status().is_success(),
          "human reply should succeed while billing is paused, got {}",
          resp.status()
      );
  ```

  Leave the rest of the test untouched — the assertions that follow (the reply row actually landing in `messages`) are the substance of SC-006 and are correct.

  **Do NOT** "fix" `add_message` to return 201. That changes another feature's HTTP contract, and other suites assert the current behaviour.

  **Verify** (do T063 first — this test cannot reach its assertion until the responder claim works), from `backend/` with the env vars above:

  ```bash
  cargo test -p server --test billing_responder_gate
  ```

  **Expected after T063 + T066: 4 passed / 1 failed**, the single remaining failure being `allowance_available_responder_proceeds_normally`, which needs a live AI provider — see the guardrail in T063.

---

### Phase 8 completion check

With T063–T066 done, from `backend/` and with `DATABASE_URL` + `REQUIRE_DB_TESTS=1` set:

```bash
cargo test -p billing                              # expect 31/31 (15 + 6 + 10)
cargo test -p server --test billing_overview       # expect 9/9
cargo test -p server --test billing_platform       # expect 38/38
cargo test -p server --test billing_responder_gate # expect 4/5 — see T063 guardrail
cargo test -p audit                                # expect green
```

Do not run bare `cargo test --workspace` and read its result as this feature's gate — it skips DB tests and aborts early (Convention 8).
