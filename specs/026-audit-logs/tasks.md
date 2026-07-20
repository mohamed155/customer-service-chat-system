---
description: "Task list for 026-audit-logs implementation"
---

# Tasks: Audit Logs

**Input**: Design documents from `/specs/026-audit-logs/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/audit-api.md](contracts/audit-api.md), [quickstart.md](quickstart.md)

**Tests**: Test tasks ARE included — the constitution (Principle VII) makes unit/integration/API tests a required category, and the repo has established gate tests (`rbac.rs`, `openapi_coverage.rs`) that will FAIL if this feature is added without updating them.

**Organization**: Tasks are grouped by user story. US1 (tenant view) is the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: [US1] tenant audit view · [US2] platform audit view · [US3] recording coverage
- Every task names its exact file path.

---

## READ THIS FIRST — Conventions that apply to EVERY task

These rules were verified against the codebase on 2026-07-19. Follow them exactly; do not invent alternatives.

**Copy-these-patterns reference files** (open the reference before writing the new file):

| You are writing | Copy the structure from |
|---|---|
| Backend module `model.rs` / `queries.rs` / `routes.rs` | `backend/crates/modules/analytics/src/{model,queries,routes}.rs` |
| Cursor pagination (encode/decode + query) | `backend/crates/modules/conversations/src/queries.rs` lines ~147-161 (`encode_cursor`/`decode_cursor`) and `inbox_query` |
| Paginated response DTO | `backend/crates/modules/conversations/src/routes.rs` (`Pagination`, `PaginatedResponse<T>`) |
| Audit row INSERT | `backend/crates/modules/tools/src/audit.rs` (`record_config_change`) |
| Frontend API service (paginated) | `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` — **use this one**, not the analytics service (see rule 7 below) |
| Frontend SignalStore | `frontend/apps/dashboard/src/app/features/tenant/analytics/analytics.store.ts` |
| Frontend drawer component | `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/version-history-drawer.component.ts` (uses `app-dialog-shell` with `variant="drawer-right"`) |

**Hard rules**:

1. **Backend routes MUST be registered with the `routes!()` macro** in `backend/crates/server/src/router.rs`, never plain `.route()`. Plain `.route()` silently drops the `#[utoipa::path]` annotation and `openapi_coverage.rs` will fail.
2. **JSON field names are `snake_case`** on the wire (Rust DTOs use plain snake_case field names — no `#[serde(rename_all)]` needed). The Angular side converts to camelCase in a `…FromWire()` function in `core/api/tenant-api.models.ts`.
3. **Never write to `audit_logs` from the new read module.** The table is append-only (DB trigger `audit_logs_append_only`). The only new write in this feature is T033 (tool executions).
4. **Tenant queries MUST filter `tenant_id = ctx.tenant_id`** from `tenancy::TenantContext`. Never accept a tenant id from the client on the tenant endpoint.
5. **Angular**: standalone components, `ChangeDetectionStrategy.OnPush`, signals, RxJS operator composition (no `async/await`, no `.then()`, no `firstValueFrom` in stores/services).
6. **Route paths come from `APP_PATHS`** — no hardcoded path strings in feature files.
7. **Response envelope — read this before writing any API service.** `ApiService.get<T>()` returns `ApiResponse<T> = { data: <the HTTP body>, requestId? }`. The 026 endpoints return a **wrapped** body: `{ "data": [...entries], "pagination": {...} }`. So inside the `map`, the destructured `data` **is the whole wire body** — reach the array via `data.data` and the cursor via `data.pagination`, exactly as `conversations-api.service.ts` does. **Do NOT copy `analytics-api.service.ts`**: its backend returns an *unwrapped* body (`Json(dto)` in `analytics/src/routes.rs`), so its `data.data` access does not transfer to a wrapped endpoint and will yield `undefined` here.

**Verification commands** (run after the phase that touches each side):

```bash
cd backend  && cargo build && cargo test -p authz -p audit -p server
cd frontend && pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check
```

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Turn the placeholder `audit` crate into a real module and add the index the read queries need.

- [x] T001 Replace the placeholder contents of `backend/crates/modules/audit/Cargo.toml` with a dependency block copied from `backend/crates/modules/analytics/Cargo.toml`. Keep the existing `[package]` section (`name = "audit"`, `version.workspace = true`, `edition.workspace = true`) and add a `[dependencies]` section with exactly: `axum.workspace = true`, `chrono = { workspace = true, features = ["serde"] }`, `hex.workspace = true`, `kernel = { path = "../../shared/kernel" }`, `serde.workspace = true`, `serde_json.workspace = true`, `sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }`, `tenancy = { path = "../tenancy" }`, `tracing.workspace = true`, `utoipa.workspace = true`, `uuid.workspace = true`. If `hex` or `serde_json` is not already in the root `[workspace.dependencies]` of `backend/Cargo.toml`, use the same declaration style the `conversations` crate uses for `hex`.

- [x] T002 Replace the single placeholder comment line in `backend/crates/modules/audit/src/lib.rs` with a module doc comment plus module declarations. Use `backend/crates/modules/analytics/src/lib.rs` as the template for the doc-comment sections (Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, Extension Points — required by the constitution's Documentation section). Document that this module owns the READ side of the existing `audit_logs` table, that the table is append-only, and that per-module writers live in their own crates. End the file with: `pub mod model;`, `pub mod queries;`, `pub mod routes;`.

- [x] T003 Add `audit = { path = "../modules/audit" }` to the `[dependencies]` section of `backend/crates/server/Cargo.toml`, keeping the existing alphabetical ordering (it goes right after `analytics = { path = "../modules/analytics" }`).

- [x] T004 Create migration `backend/migrations/0053_audit_read_indexes.sql` containing exactly one statement: `CREATE INDEX audit_logs_actor_created_idx ON audit_logs (actor_user_id, created_at DESC);`. Add a leading SQL comment explaining it supports the `actor_id` filter on the audit list endpoints. Do NOT alter any existing column, constraint, or trigger on `audit_logs`.

**Checkpoint**: `cd backend && cargo build` succeeds (the audit crate compiles with empty-but-declared modules once T007/T008/T015 land; until then it is expected to fail on missing modules — create empty `model.rs`, `queries.rs`, `routes.rs` files in T002 if you want an intermediate green build).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The permission catalog, the shared read/query layer, and the shared presentational components that BOTH user stories need.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Backend foundation

- [x] T005 Add two permissions to `backend/crates/modules/authz/src/permission.rs`. In the `Permission` enum add `#[serde(rename = "audit.view")] AuditView,` and `#[serde(rename = "platform.audit.view")] PlatformAuditView,`. Then update ALL FOUR places that enumerate permissions in this file: (a) add `Self::AuditView` to the `TENANT` const and change its length from `[Self; 22]` to `[Self; 23]`; (b) add both `Self::AuditView` and `Self::PlatformAuditView` to the `ALL` const and change its length from `[Self; 28]` to `[Self; 30]`; (c) add both match arms to the `impl fmt::Display` block (`Self::AuditView => "audit.view"`, `Self::PlatformAuditView => "platform.audit.view"`); (d) in the `catalog_parity_with_contract` test, add `"audit.view"` and `"platform.audit.view"` to `contract_codes` and change its type from `[&str; 28]` to `[&str; 30]`.

- [x] T006 Grant the new permissions in `backend/crates/modules/authz/src/matrix.rs`. Add `Permission::AuditView` to the `TENANT_ADMIN` const array. Add `Permission::PlatformAuditView` to ALL FIVE platform const arrays: `PLATFORM_ALL`, `PLATFORM_DEVELOPER`, `PLATFORM_TENANT_ACCESS`, `PLATFORM_SUPPORT`, `PLATFORM_FINANCE`. Do NOT add `AuditView` to `TENANT_MANAGER`, `TENANT_AGENT`, or `TENANT_VIEWER` (Owner/Admin only — spec clarification). Do NOT add it to any `STAFF_PRODUCTION_*` array. Owner inherits it automatically via `Permission::TENANT` from T005. Then run `cargo test -p authz` and confirm all four existing matrix tests still pass — in particular `tenant_role_hierarchy_and_owner_exclusives_hold`, whose Owner-minus-Admin difference set must remain exactly `{BillingView, BillingManage, TenantDelete, OwnerAssign}`.

- [x] T007 [P] Create `backend/crates/modules/audit/src/model.rs` with the DTOs and query types from [data-model.md](data-model.md). Derive `#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]` on DTOs. Define: (a) `AuditActorDto` with fields `kind: String`, `id: Option<Uuid>`, `display_name: Option<String>`, `email: Option<String>`, `is_platform_staff: bool`, `deleted: bool`; (b) `AuditEntryDto` with fields `id: Uuid`, `action: String`, `category: String`, `actor: AuditActorDto`, `resource_type: String`, `resource_id: String`, `tenant_id: Option<Uuid>`, `details: serde_json::Value`, `created_at: DateTime<Utc>`; (c) `AuditPagination { next_cursor: Option<String>, has_more: bool }`; (d) `AuditListResponse { data: Vec<AuditEntryDto>, pagination: AuditPagination }`; (e) `AuditQuery` (a `#[derive(Deserialize, IntoParams)]` struct with `#[into_params(parameter_in = Query)]`) holding `cursor: Option<String>`, `limit: Option<i64>`, `from: Option<String>`, `to: Option<String>`, `category: Option<String>`, `actor_id: Option<Uuid>`, `tenant_id: Option<Uuid>`. Also define the category table as a `const CATEGORY_PREFIXES: &[(&str, &[&str])]` exactly matching the table in data-model.md, plus two functions: `pub fn category_for_action(action: &str) -> &'static str` (longest-prefix match, returns `"other"` when nothing matches) and `pub fn prefixes_for_category(category: &str) -> Option<Vec<String>>` (returns the LIKE patterns, i.e. each prefix with `%` appended, or `None` for an unknown category). Add `#[cfg(test)] mod tests` covering: `category_for_action("member.role_changed") == "members"`, `category_for_action("agent_prompt.version_created") == "prompts"`, `category_for_action("zzz.unknown") == "other"`, and that `prefixes_for_category("nope")` is `None`.

- [x] T008 [P] Create `backend/crates/modules/audit/src/queries.rs` with the SQL read layer. Include: (a) `pub fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String` and `pub fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)>` — copy the hex-encoded `"{rfc3339}|{uuid}"` implementation verbatim from `backend/crates/modules/conversations/src/queries.rs` lines ~147-161, only renaming the first parameter; (b) `pub async fn list_entries(...) -> Result<Vec<AuditRow>, sqlx::Error>` taking `pool: &PgPool`, `tenant_scope: Option<Uuid>` (Some = tenant endpoint, hard tenant filter; None = platform endpoint), `filter_tenant_id: Option<Uuid>` (platform-only optional filter), `from_ts`/`to_ts: Option<DateTime<Utc>>`, `category_prefixes: Option<Vec<String>>`, `actor_id: Option<Uuid>`, `cursor: Option<(DateTime<Utc>, Uuid)>`, `limit: i64`. Build the query with `sqlx::QueryBuilder` following the dynamic-filter style of `inbox_query` in the conversations crate. The SQL selects from `audit_logs a LEFT JOIN users u ON u.id = a.actor_user_id` — the join MUST NOT filter on `u.deleted_at` (deleted actors must still resolve, FR-011). Select `a.id, a.action, a.actor_user_id, a.resource_type, a.resource_id, a.tenant_id, a.details, a.created_at, u.display_name, u.email, u.platform_role, u.deleted_at`. Apply: `a.tenant_id = $tenant` when `tenant_scope` is Some; `a.tenant_id = $filter` when `filter_tenant_id` is Some; `a.created_at >= $from` / `a.created_at <= $to`; `a.action LIKE ANY($prefixes)`; `a.actor_user_id = $actor`; and for the cursor `(a.created_at, a.id) < ($ts, $id)`. Always end with `ORDER BY a.created_at DESC, a.id DESC LIMIT $limit + 1` (fetch one extra row to compute `has_more`). Define `pub struct AuditRow` with `#[derive(sqlx::FromRow)]` matching the selected columns.

### Frontend foundation

- [x] T009 [P] Add `| 'audit.view'` and `| 'platform.audit.view'` to the `Permission` union type in `frontend/apps/dashboard/src/app/core/authz/permissions.ts`. Place `'audit.view'` after `'analytics.view'` and `'platform.audit.view'` after `'platform.diagnostics.view'` to mirror the backend catalog ordering.

- [x] T010 Register the two new routed pages in the router constants. (a) In `frontend/apps/dashboard/src/app/core/router/app-paths.ts` add `auditLogs: 'audit-logs',` to the `tenant` object (after `analytics`) and `auditLogs: 'audit-logs',` to the `platform` object (after `newTenant`). (b) In `frontend/apps/dashboard/src/app/core/authz/permissions.ts` add `[APP_PATHS.tenant.auditLogs]: 'audit.view',` and `[APP_PATHS.platform.auditLogs]: 'platform.audit.view',` to the `PAGE_PERMISSIONS` object. (c) In `frontend/apps/dashboard/src/app/core/router/page-title.ts` add `'auditLogs'` and `'platformAuditLogs'` to the `PageTitleKey` union AND matching entries to the `PAGE_TITLES` record: `auditLogs: { title: 'Audit Logs', subtitle: 'Track sensitive actions in your workspace' }` and `platformAuditLogs: { title: 'Audit Logs', subtitle: 'Platform-wide activity across all tenants' }`. Note: `PAGE_TITLES` is typed `Record<PageTitleKey, PageTitleEntry>`, so a missing entry is a compile error.

- [x] T011 [P] Add the audit wire types and mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, following the existing `AnalyticsSummaryWire` / `analyticsSummaryFromWire` pattern near the end of the file. Define: `AuditActorWire` / `AuditEntryWire` / `AuditListWire` (snake_case fields exactly matching the Rust DTOs in T007) and their camelCase counterparts `AuditActor` / `AuditEntry` / `AuditList` (`isPlatformStaff`, `resourceType`, `resourceId`, `tenantId`, `createdAt`, `nextCursor`, `hasMore`). Export `auditListFromWire(wire: AuditListWire): AuditList` that maps every entry and the pagination object. Keep `details` as `Record<string, unknown>` — do not transform its keys, it is free-form metadata rendered verbatim in the drawer.

- [x] T012 [P] Create `frontend/apps/dashboard/src/app/shared/components/audit-log-table/audit-log-table.component.ts`: a standalone, OnPush, purely presentational component with selector `app-audit-log-table`. Inputs: `entries = input.required<AuditEntry[]>()`, `loading = input(false)`, `showTenantColumn = input(false)`. Output: `rowSelected = output<AuditEntry>()`. Template wraps the existing `<app-data-table>` (from `../data-table/data-table.component`) around a `<table>` with columns: Time (`{{ entry.createdAt | date: 'medium' }}`), Actor, Action, Target, and Tenant (rendered only when `showTenantColumn()` is true). Actor cell rules: when `entry.actor.kind === 'system'` render the text `System`; otherwise render `actor.displayName`, append a `Platform staff` badge span when `actor.isPlatformStaff`, and append a `deleted` badge span when `actor.deleted`. Action cell shows the raw `action` plus a muted `category` label. Target cell shows `resourceType` and a muted truncated `resourceId`. Each `<tr>` gets `(click)="rowSelected.emit(entry)"`, `tabindex="0"`, `(keydown.enter)="rowSelected.emit(entry)"`, and `role="button"` for keyboard accessibility. Render `<app-empty-state>` when `entries().length === 0 && !loading()`, and `<app-loading-state>` when `loading()` — both already exist in `shared/components/`. Use only `--app-*` design tokens for styling; no raw Taiga classes.

- [x] T013 [P] Create `frontend/apps/dashboard/src/app/shared/components/audit-detail-drawer/audit-detail-drawer.component.ts`: a standalone, OnPush, purely presentational component with selector `app-audit-detail-drawer`. Inputs: `entry = input<AuditEntry | null>(null)`, `open = input(false)`. Output: `closed = output<void>()`. Structure it on `features/tenant/ai-agent/prompt/version-history-drawer.component.ts` — wrap everything in `<app-dialog-shell variant="drawer-right" [open]="open()" (dismiss)="closed.emit()">` with a header row containing the title `Audit Entry` and a close button that emits `closed`. Body renders a definition list of: Time (full date via `DatePipe`), Actor (same System/name/platform-staff/deleted rules as T012), Action, Category, Target type, Target ID, Tenant ID (only when present), and a Metadata section rendering `entry.details` as pretty-printed JSON inside a `<pre>` with `white-space: pre-wrap; overflow-x: auto; max-height: 40vh; overflow-y: auto` so large or deeply nested payloads stay readable (spec Edge Cases). Render nothing when `entry()` is null.

- [x] T014 [P] Create `frontend/apps/dashboard/src/app/shared/fixtures/audit.fixtures.ts` exporting `AUDIT_ENTRY_FIXTURES: AuditEntry[]` — at least five entries that cover the display branches the specs assert on: a normal tenant user actor, a `kind: 'system'` actor (e.g. `auth.login_failed`), a `isPlatformStaff: true` actor, a `deleted: true` actor, and one entry whose `details` is a nested object. Follow the typing/export style of the existing `analytics.fixtures.ts` in the same folder.

**Checkpoint**: `cd backend && cargo test -p authz` passes; `cd frontend && pnpm ng build dashboard` compiles. Both user stories can now start in parallel.

---

## Phase 3: User Story 1 - Tenant admin reviews their tenant's audit trail (Priority: P1) 🎯 MVP

**Goal**: A tenant Owner/Admin can open Audit Logs, see their tenant's entries newest-first, filter them, and open a row's full detail — with Manager/Agent/Viewer denied server-side and zero cross-tenant leakage.

**Independent Test**: Sign in as tenant Admin, change a member's role, open Audit Logs, confirm the `member.role_changed` entry appears with correct actor/action/target/timestamp, filter by category `members`, and open the detail drawer. Then confirm an Agent gets 403 from `GET /api/v1/tenant/audit-logs`.

### Backend for User Story 1

- [x] T015 [US1] Create `backend/crates/modules/audit/src/routes.rs` with the tenant handler `pub async fn list_tenant_audit_logs`. Signature follows `analytics::routes::get_analytics_summary`: `State(pool): State<PgPool>`, `ctx: tenancy::TenantContext`, `Query(query): Query<model::AuditQuery>`, returning `Response`. Annotate with `#[utoipa::path(get, path = "/tenant/audit-logs", tag = "audit", operation_id = "list_tenant_audit_logs", ...)]` documenting every query param from [contracts/audit-api.md](contracts/audit-api.md) and responses 200 / 403 / 422. Logic: clamp `limit` into `1..=100` defaulting to 50; parse `from`/`to` as `YYYY-MM-DD` into inclusive UTC timestamps (start-of-day for `from`, end-of-day for `to`) returning `ApiError::unprocessable_entity` on a bad date; resolve `category` via `model::prefixes_for_category` returning 422 on an unknown value; decode `cursor` via `queries::decode_cursor` returning 422 on malformed input; **ignore `query.tenant_id` entirely** and pass `Some(ctx.tenant_id)` as `tenant_scope`. Call `queries::list_entries`, then map rows to `AuditEntryDto`: `category` from `model::category_for_action(&row.action)`; actor is `kind: "system"` with all other fields empty when `actor_user_id` is `None` or the join produced no user, otherwise `kind: "user"` with `is_platform_staff = row.platform_role.is_some()` and `deleted = row.deleted_at.is_some()`. Compute `has_more` by checking whether the extra row was returned, truncate to `limit`, and set `next_cursor` from the last kept row via `queries::encode_cursor`. On any sqlx error log with `tracing::error!` and return `ApiError::internal_error("Failed to load audit logs")` — mirror the error handling in `analytics/src/routes.rs`.

- [x] T016 [US1] Register the tenant endpoint in `backend/crates/server/src/router.rs`. Inside `fn tenant_routes`, directly after the two existing analytics `.routes(...)` blocks (around line 720), add: `.routes(routes!(audit::routes::list_tenant_audit_logs).layer(require_permission(Permission::AuditView)))` with a `// Audit logs (spec 026)` comment. Use the `routes!()` macro — a plain `.route()` will break `openapi_coverage.rs`.

- [x] T017 [US1] Register the audit DTO schemas in `backend/crates/server/src/openapi.rs`. Add `audit::model::AuditActorDto`, `audit::model::AuditEntryDto`, `audit::model::AuditPagination`, and `audit::model::AuditListResponse` to the `components(schemas(...))` list, placing them next to the existing `analytics::model::*` entries (around line 151).

- [x] T018 [P] [US1] Add `("GET", "/tenant/audit-logs"),` to the `EXPECTED` const in `backend/crates/server/tests/openapi_coverage.rs`, right after the two `/tenant/analytics/*` entries (around line 166). Note the paths in this list have NO `/api/v1` prefix.

- [x] T019 [P] [US1] Add `("/api/v1/tenant/audit-logs", "audit.view"),` to the `TENANT_OPERATIONS` const in `backend/crates/server/tests/rbac.rs`, after the two `/api/v1/tenant/analytics/*` entries (around line 88). This path DOES carry the `/api/v1` prefix. No test-closure route is needed — the real route is used directly, exactly as analytics does.

- [x] T020 [US1] Create `backend/crates/server/tests/audit_logs.rs` with DB-gated integration tests for the tenant endpoint. Copy the harness helpers (`require_db_tests`, `get_pool`, `app_state`, `session_cookie`, `authenticated_request`, `send`) from `backend/crates/server/tests/rbac.rs`. Cover: (1) **tenant isolation** — seed `audit_logs` rows for tenant A and tenant B, request as an Admin of A, assert every returned `tenant_id` equals A and B's rows never appear (FR-004, SC-004); (2) **platform-level rows excluded** — a row with `tenant_id IS NULL` is not returned to a tenant caller; (3) **ordering** — results are newest-first by `created_at`; (4) **cursor pagination** — with `limit=2` over 5 seeded rows, following `next_cursor` yields all 5 with no duplicates and ends with `has_more: false`; (5) **category filter** — `?category=members` returns only `member.*`/`skill.*`/`availability.*` rows; (6) **deleted actor** — a row whose actor user has `deleted_at` set still returns with `actor.deleted == true` (FR-011); (7) **system actor** — a row with `actor_user_id IS NULL` returns `actor.kind == "system"` (FR-009); (8) **platform-staff actor** — a row whose actor has a non-null `platform_role` returns `actor.is_platform_staff == true` (FR-013); (9) **422** on `?category=bogus`, on `?from=not-a-date`, and on `?cursor=zzz`.

- [x] T021 [US1] Add a regression test to `backend/crates/server/tests/audit_logs.rs` proving immutability (FR-003, SC-003): execute `UPDATE audit_logs SET action = 'tampered' WHERE id = $1` and then `DELETE FROM audit_logs WHERE id = $1` directly against the pool and assert BOTH return an error mentioning `append-only` (raised by the `audit_logs_append_only` trigger).

### Frontend for User Story 1

- [x] T022 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs-api.service.ts` modeled on `features/tenant/conversations/conversations-api.service.ts` (NOT the analytics service — see conventions rule 7). `@Injectable({ providedIn: 'root' })`, injects `ApiService`, exposes `list(query: { cursor?: string | null; from?: string; to?: string; category?: string | null; actorId?: string | null }): Observable<ApiResponse<AuditList>>`. The body is:

  ```ts
  return this.api
    .get<AuditListWire>('/tenant/audit-logs', this.buildParams(query))
    .pipe(map(({ data, ...response }) => ({ ...response, data: auditListFromWire(data) })));
  ```

  Note `data` here is the whole wire body (`{ data: [...], pagination: {...} }`) and is passed to `auditListFromWire` **as-is** — do not write `auditListFromWire(data.data)`, which would drop the pagination and pass the wrong shape. Build `HttpParams` with a private `buildParams` helper that only sets non-empty values, and note the query-string key for the actor filter is `actor_id` (snake_case) even though the TypeScript property is `actorId`.

- [x] T023 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs.store.ts` — an NgRx SignalStore modeled on `analytics.store.ts`. State: `entries: AuditEntry[]`, `nextCursor: string | null`, `hasMore: boolean`, `from: string`, `to: string`, `category: string | null`, `actorId: string | null`, `loading: boolean`, `loadingMore: boolean`, `error: string | null`, `selectedEntry: AuditEntry | null`, `drawerOpen: boolean`. Default the date range to the last 30 days using the same `formatDate`/`initialFrom`/`initialTo` helpers as `analytics.store.ts`. Methods: `load()` (resets `entries` and `nextCursor`, then fetches), `loadMore()` (fetches with the stored `nextCursor` and APPENDS to `entries` — never replaces), `setCategory(value: string)` (treat `'all'` as `null`, then reload), `setDateRange(from: string, to: string)` (guard `from > to` by setting an error and returning without a request, like `setCustomRange` does), `setActor(id: string | null)` (reload), `openEntry(entry: AuditEntry)` (sets `selectedEntry` + `drawerOpen: true`), and `closeDrawer()`. Use `rxMethod` + `pipe(tap, switchMap, map, catchError)` — RxJS only, no promises. Reload on tenant switch via the `withHooks` + `selectActiveTenant` effect pattern at the bottom of `analytics.store.ts`.

- [x] T024 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs.component.ts` — standalone, OnPush, selector `app-audit-logs`, `providers: [AuditLogsStore]`. Layout: a filter toolbar (reuse `<app-toolbar>` and `<app-select-filter>` from `shared/components/`) with a category select whose options are `all` plus the twelve category codes from [data-model.md](data-model.md), two date inputs bound to the store's `from`/`to`, and a `<app-search-input>` for actor id; then `<app-audit-log-table [entries]="store.entries()" [loading]="store.loading()" (rowSelected)="store.openEntry($event)" />`; then a "Load more" `<app-button>` shown only when `store.hasMore()`, calling `store.loadMore()` and disabled while `store.loadingMore()`; then `<app-audit-detail-drawer [entry]="store.selectedEntry()" [open]="store.drawerOpen()" (closed)="store.closeDrawer()" />`. Render `<app-inline-alert>` when `store.error()` is set. Do NOT pass `showTenantColumn` (tenant page shows one tenant only).

- [x] T025 [US1] Register the tenant route in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts`. Copy the analytics route object (around line 154) and adapt: `path: APP_PATHS.tenant.auditLogs`, `canMatch: [permissionGuard]`, `loadComponent: () => import('./audit-logs/audit-logs.component').then((m) => m.AuditLogsComponent)`, `data: { pageTitle: 'auditLogs', requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.auditLogs] }`, `title: PAGE_TITLES.auditLogs.title`.

- [x] T026 [US1] Add the sidebar entry in `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts`. Two edits: (a) add `auditLogs: \`/${APP_PATHS.tenant.base}/${APP_PATHS.tenant.auditLogs}\`,` to the `links` object (around line 230); (b) in the template, immediately AFTER the closing `}` of the existing "Insights" `@if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.analytics])) { ... }` block (around line 120-129), add a NEW separate block:

  ```html
  @if (permissionsService.has(PAGE_PERMISSIONS[APP_PATHS.tenant.auditLogs])) {
    <app-sidebar-nav-group label="Security" [collapsed]="collapsed()">
      <app-sidebar-nav-item
        icon="@tui.scroll-text"
        label="Audit Logs"
        [link]="links.auditLogs"
        [collapsed]="collapsed()"
      />
    </app-sidebar-nav-group>
  }
  ```

  It MUST be its own `@if` guarded on `audit.view` — do NOT nest it inside the analytics `@if`, because Manager/Agent/Viewer hold `analytics.view` but must never see the Audit Logs link. The icon `@tui.scroll-text` is verified present in `@taiga-ui/icons`.

- [x] T027 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs.store.spec.ts` using `AUDIT_ENTRY_FIXTURES` and a mocked `AuditLogsApiService` (follow `analytics.store.spec.ts` for TestBed setup). Assert: `load()` populates `entries` and clears `loading`; `loadMore()` APPENDS rather than replaces and carries `nextCursor` into the request; `setCategory('all')` sends no category param while `setCategory('members')` sends `members`; `setDateRange` with `from > to` sets `error` and issues NO request; `openEntry`/`closeDrawer` toggle `selectedEntry` and `drawerOpen`; an API error sets `error` and clears `loading`.

- [x] T028 [P] [US1] Create `frontend/apps/dashboard/src/app/shared/components/audit-log-table/audit-log-table.component.spec.ts` and `frontend/apps/dashboard/src/app/shared/components/audit-detail-drawer/audit-detail-drawer.component.spec.ts` driven by `AUDIT_ENTRY_FIXTURES`. Table spec asserts: one row per entry; a `system`-actor row renders the text `System`; a platform-staff row renders the staff badge; a deleted-actor row renders the deleted label; clicking a row emits `rowSelected` with that entry; the empty state renders when `entries` is `[]`; and the tenant column appears only when `showTenantColumn` is true. Drawer spec asserts: nothing renders when `entry` is null; the nested-`details` fixture renders as pretty-printed JSON; and the close button emits `closed`.

- [x] T029 [P] [US1] Update `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.spec.ts`: the existing permission arrays in that file list granted permissions per role, so add `'audit.view'` to the Owner/Admin cases and add an assertion that the Audit Logs nav item is ABSENT for a role granted `'analytics.view'` but not `'audit.view'` (guards the T026 grouping decision).

**Checkpoint**: US1 is fully functional and independently testable — the MVP. Backend and frontend gates must both pass.

---

## Phase 4: User Story 2 - Platform user reviews platform-wide audit logs (Priority: P2)

**Goal**: Any platform role can see audit entries across all tenants plus tenant-less platform events, and filter by tenant. Tenant-only users are denied.

**Independent Test**: Sign in as a platform user, open the platform Audit Logs page, confirm entries from multiple tenants and at least one `tenant_id: null` platform event are visible, then filter to a single tenant. Confirm a tenant-only user gets 403 from `GET /api/v1/platform/audit-logs`.

### Backend for User Story 2

- [x] T030 [US2] Add `pub async fn list_platform_audit_logs` to `backend/crates/modules/audit/src/routes.rs`. It is the same handler body as T015 with three differences: (a) it does NOT take `ctx: tenancy::TenantContext` (platform routes have no tenant context) — its extractors are `State(pool): State<PgPool>` and `Query(query): Query<model::AuditQuery>`; (b) it passes `None` as `tenant_scope` so rows from every tenant AND rows with `tenant_id IS NULL` are returned; (c) it honors `query.tenant_id` as the optional `filter_tenant_id`. Annotate with `#[utoipa::path(get, path = "/platform/audit-logs", tag = "audit", operation_id = "list_platform_audit_logs", ...)]` documenting the extra `tenant_id` param. Extract the shared row-to-DTO mapping and parameter parsing from T015 into private helper functions in this file rather than duplicating them.

- [x] T031 [US2] Register the platform endpoint in `backend/crates/server/src/router.rs`. Inside `fn platform_routes`, after the existing `ai::routes::test_platform_config` block (around line 313), add `.routes(routes!(audit::routes::list_platform_audit_logs).layer(require_permission(Permission::PlatformAuditView)))` with a `// Platform audit logs (spec 026)` comment. It must go in `platform_routes`, NOT `tenant_routes` — that is what applies the platform-permission middleware and keeps it free of tenant-context requirements.

- [x] T032 [P] [US2] Add `("GET", "/platform/audit-logs"),` to the `EXPECTED` const in `backend/crates/server/tests/openapi_coverage.rs` (no `/api/v1` prefix), and add `"/api/v1/platform/audit-logs",` to the `PLATFORM_OPERATIONS` const in `backend/crates/server/tests/rbac.rs` (with the `/api/v1` prefix).

- [x] T033 [US2] Extend `backend/crates/server/tests/audit_logs.rs` with platform-endpoint tests: (1) a platform user receives rows from BOTH seeded tenants plus a `tenant_id IS NULL` platform row; (2) `?tenant_id=<A>` narrows results to tenant A only; (3) a tenant-only user (no `platform_role`) receives **403**; (4) each of the five platform roles (`super_admin`, `developer`, `sales`, `support`, `finance`) receives **200**, proving the T006 grant across all five arrays.

### Frontend for User Story 2

- [x] T034 [P] [US2] Create `frontend/apps/dashboard/src/app/features/platform/audit-logs/platform-audit-logs-api.service.ts` — identical in shape to T022's service (including the `auditListFromWire(data)` envelope handling from conventions rule 7 — pass the whole body, never `data.data`) but hitting `'/platform/audit-logs'` and accepting an extra optional `tenantId` in its query object, serialized as the `tenant_id` query-string key.

- [x] T035 [US2] Create `frontend/apps/dashboard/src/app/features/platform/audit-logs/platform-audit-logs.store.ts` — same state and methods as T023's store plus `tenantId: string | null` in state and a `setTenant(id: string | null)` method that reloads. It injects `PlatformAuditLogsApiService`. Omit the `selectActiveTenant` tenant-switch effect — the platform view is not tenant-scoped; instead load once in `withHooks` `onInit`.

- [x] T036 [US2] Create `frontend/apps/dashboard/src/app/features/platform/audit-logs/platform-audit-logs.component.ts` — standalone, OnPush, selector `app-platform-audit-logs`, `providers: [PlatformAuditLogsStore]`. Same layout as T024 but with an additional tenant filter control and `[showTenantColumn]="true"` passed to `<app-audit-log-table>`. It imports the SAME `audit-log-table` and `audit-detail-drawer` components from `shared/components/` — do not create platform-specific copies.

- [x] T037 [US2] Register the platform route in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`, following the existing entries in that file: `path: APP_PATHS.platform.auditLogs`, `canMatch: [permissionGuard]`, `data: { pageTitle: 'platformAuditLogs', requiredPermission: 'platform.audit.view' }`, `loadComponent: () => import('./audit-logs/platform-audit-logs.component').then((module) => module.PlatformAuditLogsComponent)`. Add it AFTER the `${APP_PATHS.platform.tenants}/:id` route so the parameterized tenants route does not shadow it.

- [x] T038 [P] [US2] Create `frontend/apps/dashboard/src/app/features/platform/audit-logs/platform-audit-logs.store.spec.ts` mirroring T027's assertions, plus: `setTenant('<uuid>')` sends the `tenant_id` param and `setTenant(null)` omits it.

**Checkpoint**: US1 and US2 both work independently.

---

## Phase 5: User Story 3 - Complete recording coverage (Priority: P3)

**Goal**: Close the one verified recording gap so every in-scope category that has actions produces audit entries. Research confirmed auth, tenant, roles, prompts, and AI provider changes are ALREADY recorded — do not re-instrument them. Billing has no actions to audit (placeholder crate).

**Independent Test**: Trigger a tool execution in a conversation and confirm a `tool.executed` entry appears in the tenant audit view with the tool name and outcome in its metadata.

- [x] T039 [US3] Add `pub async fn record_execution` to `backend/crates/modules/tools/src/audit.rs`, following the existing `record_config_change` in that same file. Parameters: `pool: &PgPool`, `tenant_id: Uuid`, `conversation_id: Uuid`, `tool_name: &str`, `resource_id: &str`, `outcome: &str`. It INSERTs into `audit_logs` with `actor_user_id = None` (tool executions are AI-driven — the system actor per FR-009), `action = "tool.executed"`, `resource_type = "tenant_tool"`, the passed `resource_id`, the passed `tenant_id`, and `details = json!({ "tool_name": tool_name, "conversation_id": conversation_id, "outcome": outcome })`. On failure log via `tracing::error!` and return `()` — a failed audit write must never fail the tool execution (FR-012). Note `resource_id` is NOT NULL-constrained (`audit_logs_resource_required`), so callers must always pass a non-empty value.

- [x] T040 [US3] Call the new writer from `backend/crates/modules/tools/src/executor.rs` in `pub async fn execute`. Capture the `ExecutionOutcome` into a local binding instead of returning the `match` directly, then before returning it call `crate::audit::record_execution(&ctx.pool, ctx.tenant_id, ctx.conversation_id, &resolved.spec.name, &resource_id, outcome_label).await` where `resource_id` is `resolved.tenant_tool_id.map(|id| id.to_string()).unwrap_or_else(|| resolved.spec.name.to_string())` (builtin tools have no tenant tool id) and `outcome_label` is `"succeeded"` / `"failed"` / `"timed_out"` derived from the outcome variant. Record on ALL THREE outcomes — failures are security-relevant. Make sure both the `ToolSource::Builtin` and `ToolSource::Tenant` branches flow through this single recording point rather than duplicating the call.

- [x] T041 [US3] Add an integration test to `backend/crates/server/tests/audit_logs.rs` that runs a builtin tool through `tools::executor::execute` (copy the `exec_ctx` / `ResolvedTool` setup from `backend/crates/server/tests/tenant_defined_tool_execution.rs` around line 273) and then asserts a `tool.executed` row exists in `audit_logs` for that tenant with `resource_type = 'tenant_tool'`, `actor_user_id IS NULL`, and a `details` payload containing the tool name and an `outcome` of `succeeded`. Add a second case asserting a failing execution still records a row with `outcome` `failed`.

**Checkpoint**: All three user stories are independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T042 [P] Update `backend/crates/modules/tools/src/lib.rs` module documentation to mention that tool executions now emit `tool.executed` audit entries (the constitution requires each module to document its Responsibilities and Data Model).

- [x] T043 [P] Add a short "Audit Logs" subsection to `frontend/CLAUDE.md` noting that `shared/components/audit-log-table` and `audit-detail-drawer` are shared by the tenant and platform audit pages and must not be forked per area.

- [x] T044 Run the full verification suite and fix anything red: `cd backend && cargo build && cargo test -p authz -p audit -p server`, then `cd frontend && pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check`. Report actual output — do not claim success without it.

- [x] T045 Walk through every scenario in [quickstart.md](quickstart.md) against a running stack (`cargo run -p server` + `pnpm ng serve dashboard`) and confirm each expected outcome, especially Scenario 2 (RBAC denial), Scenario 3 (tenant isolation), and Scenario 6 (platform-staff visibility in the tenant view).

- [x] T046 Add a performance verification test to `backend/crates/server/tests/audit_logs.rs` proving **SC-005** (list responds in under 2 seconds at realistic volume) — this is the only task that verifies SC-005; T004's index is the intended mechanism but nothing else measures it. Seed ~50,000 `audit_logs` rows for one tenant in a SINGLE bulk insert: `INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details, created_at) SELECT ... FROM generate_series(1, 50000) i`, spreading `created_at` across the range with `now() - (i || ' minutes')::interval`, and varying `action` across at least three category prefixes so the category filter meets realistic selectivity. Then, using `std::time::Instant`, assert each of these completes in under 2 seconds: (a) the unfiltered first page; (b) a page filtered by `?category=members`; (c) a page reached via a `next_cursor` deep in the range; (d) a page filtered by `?actor_id=<uuid>` — this one specifically exercises the `audit_logs_actor_created_idx` index added in T004. Gate it behind `require_db_tests()` like the other tests in this file and mark it `#[ignore]` with a comment to run it explicitly (`cargo test -p server -- --ignored`) so it does not slow the default suite. If an assertion fails, fix the index or the query — do NOT raise the 2-second threshold to make it pass.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies — start immediately.
- **Foundational (Phase 2)**: needs Phase 1 (the crate must exist and be wired into the server). **Blocks both user stories.**
- **US1 (Phase 3)** and **US2 (Phase 4)**: both need Phase 2 only. They are independent of each other and can run in parallel — US2 deliberately reuses the shared components built in Phase 2, not in US1.
- **US3 (Phase 5)**: needs Phase 2 only (it writes rows; it does not depend on the read endpoints). Its *verification* is easiest once US1 exists, but the code is independent.
- **Polish (Phase 6)**: after the stories you intend to ship.

### Critical intra-phase dependencies

- T005 → T006 (matrix references the enum variants added in T005).
- T007 + T008 → T015 (the handler consumes the model and query layers).
- T015 → T030 (the platform handler reuses helpers extracted in T015).
- T012 + T013 + T011 → T024 and T036 (pages consume the shared components and wire types).
- T010 → T025 and T037 (routes reference `APP_PATHS` / `PAGE_TITLES` keys).
- T039 → T040 → T041.

### Parallel Opportunities

- Phase 2: T007 and T008 (backend) run in parallel with T009, T011, T012, T013, T014 (frontend) — different files, no shared edits. T005 → T006 stay sequential.
- Phase 3: T018, T019 (test tables) run in parallel with each other and with T022; T027, T028, T029 all run in parallel at the end.
- Phase 4: T032, T034 run in parallel; T038 at the end.
- Whole stories: with two developers, US1 and US2 proceed simultaneously after Phase 2; US3 is a small third track.

**Watch for file collisions** (these tasks touch the same file and must NOT run concurrently): T016 and T031 both edit `router.rs`; T018 and T032 both edit `openapi_coverage.rs`; T019 and T032 both edit `rbac.rs`; T020, T021, T033, T041, T046 all edit `audit_logs.rs`; T009 and T010 both edit `permissions.ts`.

---

## Parallel Example: Phase 2 Foundational

```bash
# Backend read layer (after T005 → T006 complete):
Task: "T007 Create audit model.rs with DTOs and category mapping"
Task: "T008 Create audit queries.rs with cursor codec and list query"

# Frontend foundation (fully independent of the backend tasks above):
Task: "T009 Add audit permissions to the Permission union"
Task: "T011 Add audit wire types and auditListFromWire mapper"
Task: "T012 Create shared audit-log-table component"
Task: "T013 Create shared audit-detail-drawer component"
Task: "T014 Create audit fixtures"
```

---

## Implementation Strategy

### MVP (User Story 1 only)

1. Phase 1 Setup (T001-T004).
2. Phase 2 Foundational (T005-T014) — blocks everything.
3. Phase 3 US1 (T015-T029).
4. **STOP and VALIDATE**: run quickstart Scenarios 1, 2, 3, and 7. This alone delivers the feature's core value — tenant admins can inspect their audit trail with RBAC and isolation enforced.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. + US1 → tenant audit view → **MVP, shippable**.
3. + US2 → platform-wide view for support/incident investigation.
4. + US3 → closes the tool-execution recording gap.
5. + Polish → docs and full gate run.

---

## Notes

- Research verified that prompt changes (`agent_prompt.version_created/restored`) and AI provider changes (`ai_config.*`, `ai_credential.*`) are ALREADY audited. Do not add duplicate writers for them.
- Billing is a placeholder crate with no auditable actions; the `billing` category is reserved in the mapping and will populate automatically once billing actions exist. Nothing to build for it here.
- The `audit_logs` table is append-only by DB trigger; FR-003 needs no new code, only the T021 regression test.
- Commit after each task or logical group. Stop at any checkpoint to validate a story independently.

---

## Phase 7: Convergence

Appended by `/speckit-converge` after assessing the codebase against spec.md, plan.md, and tasks.md. Every item below was verified as an actual gap by running the gates and reading the code — none restates already-complete work.

- [x] T047 Add the platform Audit Logs destination to `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts` per US2/AC1, FR-005 (missing). The `PLATFORM_DESTINATIONS` const (around line 23) lists only `Tenants` and `Platform overview`, so the platform audit page built in T036/T037 is reachable only by typing its URL — a platform user cannot "open the platform Audit Logs view" as US2/AC1 requires. Add an entry `{ label: 'Audit Logs', path: `/${APP_PATHS.platform.base}/${APP_PATHS.platform.auditLogs}`, permission: 'platform.audit.view' }`. The component already filters destinations by permission, so no guard logic is needed. Add a case to the component's spec asserting the entry is hidden for a platform-less user and shown for one holding `platform.audit.view`.

- [x] T048 Fix the `pnpm lint` failure introduced by this feature in `frontend/apps/dashboard/src/app/shared/components/search-input/search-input.component.ts` per plan quality gates / T044 (contradicts). 026 added `readonly search = output<string>()` (line 83), which trips `@angular-eslint/no-output-native` because `search` is a native DOM event name — `pnpm lint` currently exits 1, so the gate T044 claimed green is red. Rename the output to `searchSubmit`, update the template's `(keydown.enter)="search.emit(value())"` accordingly, and update the two consumers `features/tenant/audit-logs/audit-logs.component.ts` (line ~42) and `features/platform/audit-logs/platform-audit-logs.component.ts` (lines ~42 and ~47) to bind `(searchSubmit)`. Confirm with `pnpm lint` exiting 0.

- [x] T049 Fix the leaking async errors in the audit store specs per US1/AC1, T027/T038 (partial). `pnpm ng test dashboard` currently reports **10 uncaught errors** — `TypeError: Cannot read properties of undefined (reading 'pipe')` at `features/tenant/audit-logs/audit-logs.store.ts:68` — originating from `audit-logs.store.spec.ts` and `platform-audit-logs.store.spec.ts`. Cause: the stores' `withHooks` `onInit` calls `load()`, but tests such as `openEntry/closeDrawer toggle selected entry and drawer` never set a return value on the `list` mock, so `api.list(...)` yields `undefined`. Fix by setting a default `list.mockReturnValue(of({ data: MOCK_LIST }))` inside each spec's `beforeEach` (after `list.mockReset()`), letting individual tests override it. While in `audit-logs.store.spec.ts`, tighten the vacuous assertion in `setCategory('members') sends category: 'members'` (line ~58) — it currently only asserts `toHaveBeenCalled()`; assert the payload with `expect(list).toHaveBeenCalledWith(expect.objectContaining({ category: 'members' }))` as T027 specified. Verify `pnpm ng test dashboard` finishes with `Errors 0`.

- [x] T050 Correct the target identifier recorded for tool executions in `backend/crates/modules/tools/src/executor.rs` per FR-001 (contradicts). The call at line ~150 passes `&tool_request_id.to_string()` as `resource_id` while `record_execution` hardcodes `resource_type = "tenant_tool"`, so the recorded target identifies a tool *request*, not the tenant tool the resource type names — FR-001 requires the target to be "which resource, by type and identifier". Change the argument to `resolved.tenant_tool_id.map(|id| id.to_string()).unwrap_or_else(|| resolved.spec.name.to_string())` as T040 specified (builtin tools have no tenant tool id, so they fall back to the tool name). Keep `tool_request_id` discoverable by adding it to the `details` JSON in `record_execution` rather than dropping it. Update the lookup in `tool_execution_emits_audit_row` in `backend/crates/server/tests/audit_logs.rs` (it currently binds `tool_request_id.to_string()` as `resource_id`) to match the new value.

- [x] T051 Add the two missing integration cases to `backend/crates/server/tests/audit_logs.rs` per T020 item 9 and T041 (partial). (a) **Malformed cursor** — the handler returns 422 via `ApiError::unprocessable_entity("Invalid cursor")` (`audit/src/routes.rs:48-49`) but no test covers it, while sibling cases `invalid_category_returns_422` and `invalid_date_returns_422` do; add `invalid_cursor_returns_422` requesting `?cursor=zzz` and asserting 422, following those two tests' structure. (b) **Failed tool execution** — only the success path is covered by `tool_execution_emits_audit_row`; T041 also required a failing execution. Add a case that drives `tools::executor::execute` to an `ExecutionOutcome::Failed` (e.g. a `ResolvedTool` whose `spec.name` is absent from the builtin catalog, which yields `Failed("builtin tool ... not found in catalog")`) and assert a `tool.executed` row exists with `details->>'outcome' = 'failed'` and `actor_user_id IS NULL`. Gate both behind `require_db_tests()` like the rest of the file.

---

## Phase 8: Convergence

Appended by `/speckit-converge` on a second pass. T047–T051 were each re-verified against the source (not their checkboxes) and are genuinely complete — nothing below restates them. Only one actual gap remained.

- [x] T052 Restore the red shared contract gate so T044's "verification suite is green" claim actually holds, per T044 / plan quality gates / Constitution V (contradicts). `cargo test -p authz -p audit -p server` currently FAILS: `documented_paths_equal_expected_inventory` panics at `backend/crates/server/tests/openapi_coverage.rs:191` with `documented operations not in the contract inventory: [("GET", "/tenant/feedback/summary"), ("GET", "/widget/v1/feedback/pending"), ("POST", "/widget/v1/conversations/{conversationId}/feedback")]`. **These three routes are not 026's** — they belong to 024-customer-feedback and 023-website-chat-widget, are correctly registered via `routes!()` in `backend/crates/server/src/router.rs` (lines 109, 110, 721), and have passing integration tests in `backend/crates/server/tests/feedback_api.rs`; only the `EXPECTED` inventory was never updated for them. Fix by adding those three tuples to the `EXPECTED` const in `openapi_coverage.rs` (paths carry NO `/api/v1` prefix, matching the existing entries) — do NOT remove or un-document the routes, and do NOT touch the two 026 entries at lines 168-169, which are already correct. Verify with `cargo test -p server --test openapi_coverage` passing, then re-run the full `cargo test -p authz -p audit -p server` and confirm it exits 0. Note: when checking, do not pipe cargo through `tail` — the pipeline masks the real exit code, which is how T044 was originally recorded green while red.

---

## Phase 9: Convergence

Appended by `/speckit-converge` on a third pass. T047–T052 were each re-verified against the source (not their checkboxes) and are genuinely complete: the platform-nav entry exists (`platform-nav.component.ts:30-31`), `search-input` now exports `searchSubmit` (line 83) and `pnpm lint` exits 0, the executor passes the tenant-tool id as `resource_id` (`executor.rs:156`), and `cargo test -p authz -p audit -p server` exits 0. Nothing below restates them. Only one actual code gap remained.

- [x] T053 Return `next_cursor` only when another page exists, in `build_response` in `backend/crates/modules/audit/src/routes.rs` (line ~103), per data-model.md:63 / plan cursor-pagination decision (contradicts). The envelope is specified as `{ "next_cursor": string|null, "has_more": bool }`, and the pagination pattern the plan mandates copying — `conversations/src/routes.rs:377` — computes it as `has_more.then(|| encode_cursor(...))`. The audit implementation instead sets `next_cursor` unconditionally from the last kept row, so the **final** page returns `has_more: false` together with a non-null cursor. Change it to `let next_cursor = has_more.then(|| queries::encode_cursor(last.created_at, last.id));` using the last kept row, leaving the `rows.last()` → `None` behaviour for an empty page intact. Scope note: there is **no** infinite-loop symptom — a client paginating on `next_cursor != null` makes exactly one extra request that returns an empty page with a null cursor, and the dashboard is unaffected because both audit stores gate "Load more" on `hasMore`. This is a contract-conformance fix, not a user-facing bug fix. Add a case to the cursor test in `backend/crates/server/tests/audit_logs.rs` asserting that the last page returns `next_cursor: null` alongside `has_more: false` (extend the existing `cursor_pagination_returns_all_pages`; do not add a new DB harness).
