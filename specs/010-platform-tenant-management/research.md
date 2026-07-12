# Research: Platform Tenant Management

**Feature**: 010-platform-tenant-management | **Date**: 2026-07-11

All Technical Context unknowns resolved. Decisions are grounded in the actual codebase state (migrations 0001–0015, `authz`/`tenancy` modules, 008 router conventions, 009 shell).

## R1 — Schema extension for business metadata

**Decision**: One migration, `backend/migrations/0016_tenant_business_metadata.sql`:
`plan TEXT NOT NULL DEFAULT 'trial'` with `CHECK (plan IN ('trial','starter','professional','enterprise'))`; `contact_name TEXT NULL` with `CHECK (contact_name IS NULL OR length(contact_name) BETWEEN 1 AND 200)`; `contact_email TEXT NULL` (format validated in the application; no DB email regex). Existing rows get `plan='trial'` via the default. No new index.

**Rationale**: Mirrors the table's established conventions (TEXT + CHECK for enums, exactly like `status`). Email format rules evolve; a DB regex CHECK would fossilize one and reject future valid addresses — app-level validation matches how the rest of the platform validates input. No status/plan index: the directory query already narrows on the partial-indexed `deleted_at IS NULL` set, which is hundreds of rows at the SC-002 scale.

**Alternatives considered**: (a) `citext` for contact_email — unnecessary, it's display/contact data, never a lookup key. (b) A separate `tenant_metadata` table — over-normalized for three scalar fields on a 1:1 basis (Constitution VIII prefers normalized *by default*, but a 1:1 satellite table adds a join for nothing). (c) JSONB metadata bag — rejected: unconstrained, untypeable, contradicts the clarified "structured business fields" decision.

## R2 — Permission model

**Decision**: One new permission code, `platform.tenants.manage`, covering create + edit + activate/deactivate. Matrix: granted to **SuperAdmin and Support** (per clarification); Developer/Sales/Finance unchanged. Viewing (list + detail) reuses the existing `platform.tenants.list` held by all platform roles. Catalog grows 25→26: `authz::Permission` gains `PlatformTenantsManage`, the frontend `Permission` union gains the string, and 008's `catalog_parity_with_contract` test list is updated (with the 010 contract documenting the delta; 008's contract doc stays frozen as its point-in-time catalog — the 008 spec explicitly anticipated later features declaring new permissions).

**Rationale**: The spec treats management as one capability bundle (FR-008); splitting create/edit/status into three codes triples matrix rows and tests with no differentiated grantee today. Detail-view under `platform.tenants.list` matches the spec (all platform roles view) without a redundant new `.view` code.

**Alternatives considered**: (a) Three granular codes — rejected as above; the catalog pattern makes later splitting cheap if a role ever needs partial management. (b) Reusing `platform.admin` for management — rejected: `platform.admin` is SuperAdmin-only and gates the admin area concept; Support Engineer must manage tenants without inheriting everything else `platform.admin` may come to guard.

## R3 — Endpoint semantics and error vocabulary

**Decision**:
- `GET /platform/tenants` (existing) gains an optional `status=active|suspended` query param, combinable with `q` and cursor pagination in the same single SQL statement. `TenantSummary` (id/name/slug/status) is extended with `plan` — additive, so the tenant switcher and tenant-select keep working.
- `POST /platform/tenants` → 201 with the full detail payload. Body: `name`, `slug`, optional `plan` (default trial), optional `contactName`/`contactEmail`.
- `GET /platform/tenants/{id}` → detail payload (404 `not_found` for unknown/soft-deleted ids).
- `PATCH /platform/tenants/{id}` → partial update; only provided fields are validated and applied (`status` changes ride the same endpoint per the user description's endpoint list). Last-write-wins on concurrent edits (spec edge case) — no version/etag in this feature.
- Errors: `validation_failed` (422) with `ErrorDetail { field, message }` entries for per-field problems; `conflict` (409) for a slug already used by a live tenant; standard 401/403 from the middleware/guards.

**Rationale**: Everything reuses existing kernel vocabulary — `ApiError::conflict` and `with_details` already exist, so field-level validation needs no new machinery. A single PATCH endpoint matches the user-supplied endpoint list and keeps status transitions auditable as field changes with a distinct audit action.

**Alternatives considered**: (a) Separate `POST .../activate|deactivate` endpoints — rejected: not in the requested surface; PATCH `status` is sufficient and the audit action still distinguishes it. (b) Optimistic concurrency (If-Match) — rejected: spec explicitly chose last-write-wins with full audit reconstructability.

## R4 — Audit strategy (and the slug-trigger constraint)

**Decision**: App-level audit via the existing `tenancy::audit::record` with three actions: `platform.tenant_created`, `platform.tenant_updated` (details: changed fields with old/new values, excluding slug), `platform.tenant_status_changed` (details: old/new status). Slug changes are **not** duplicated app-side: the existing `tenants_slug_change_audit` trigger (migration 0015) already writes `tenant.slug_changed` with old/new slug — and it **requires** `SELECT set_audit_actor($1)` inside the same transaction, otherwise the UPDATE raises. Therefore the PATCH handler always runs `set_audit_actor(principal.user_id)` + UPDATE (+ app audit insert) inside one transaction.

**Rationale**: The trigger is a live, non-negotiable schema behavior — fighting it (or double-writing slug audits) creates conflicting records. Wrapping PATCH in a transaction is required for correctness anyway (audit row + update atomicity).

**Alternatives considered**: Dropping the trigger in favor of app-only audit — rejected: it's an 005-era integrity mechanism guaranteeing slug changes can never bypass audit, exactly the constitution III posture; the app cannot provide that guarantee against future code paths.

## R5 — Platform area routing & navigation rebalance

**Decision**: The platform area's base route gate changes from `platform.admin` to `platform.tenants.list` (held by every platform role), keeping `areaAccessGuard`'s platform check. Child routes gate individually: tenants list/detail/new → `platform.tenants.list` (management actions inside the pages are gated by `platform.tenants.manage` via `*appHasPermission` + server enforcement); the overview placeholder keeps `platform.admin`. `PAGE_PERMISSIONS` and the platform-nav `PLATFORM_DESTINATIONS` are updated accordingly — the nav control gains a "Tenants" destination (`platform.tenants.list`), which means all platform roles now see the platform-nav control with at least one destination (the 009 visibility rule — hide when the permitted list is empty — needs no change).

**Rationale**: The spec requires every platform role to reach the directory; the current SuperAdmin-only area gate would lock out Support/Developer/Sales/Finance. Per-child permissions preserve fail-closed granularity (008 pattern: route data + `permissionGuard`).

**Alternatives considered**: Mounting tenant pages outside the platform area — rejected: they are platform administration surface; splitting the area fragments navigation and breadcrumbs ("Platform / Tenants").

## R6 — Frontend data & state (RxJS-first)

**Decision**: `platform-tenants.service.ts` exposes Observable-returning methods (`list(params)`, `get(id)`, `create(payload)`, `update(id, payload)`) delegating to the existing `ApiService`. List/filter/cursor state lives in a feature-provided SignalStore (`tenants.store.ts`) using `rxMethod` with operator composition: search input debounced (`debounceTime` + `distinctUntilChanged`) then `switchMap`ed to the list call; load-more concatenates pages; create/update flows use `exhaustMap` (no double-submit). No `firstValueFrom`/`async-await` in the new code — constitution v1.2.0's RxJS-first rule applied from the start.

**Rationale**: Matches the spec-002 state rules (feature-local → SignalStore) and the newly amended constitution; `rxMethod` is the SignalStore-native bridge for operator pipelines.

**Alternatives considered**: Promise-based service like the existing `TenantContextService.select()` — rejected: that predates constitution v1.2.0; new code must not copy it.

## R7 — Pagination & list UX

**Decision**: Forward-only cursor pagination surfaced as a "Load more" control appending to the list (driven by `Page { items, next_cursor, has_more }`). Search and status filter reset the cursor and replace results. Empty results render the shared `app-empty-state` (with a create action shown only to managers).

**Rationale**: The existing contract is forward-only cursor; numbered pages would require offset counts the API deliberately doesn't expose. Append-on-load-more keeps filter+cursor consistency trivially correct (spec edge case).

**Alternatives considered**: Bidirectional cursors — contract change out of scope; numbered pagination — incompatible with cursor contract.

## R8 — Form pattern (create/edit)

**Decision**: One reusable `tenant-form.component.ts` (typed Reactive Forms) used by both the create route and the detail page's edit mode: fields name, slug, plan (select), contactName, contactEmail; client-side validators mirror the DB rules (required name ≤200, slug regex `^[a-z0-9](-?[a-z0-9])*$` ≤63, email format); server `ErrorDetail`s map onto form controls, 409 slug conflict maps onto the slug control. Status is **not** part of the form — activate/deactivate is a distinct, confirmed action on the detail page (it locks out a whole customer; separating it prevents accidental suspension inside an ordinary save).

**Rationale**: FR-002/FR-004 share identical validation — one form removes drift; separating the status control matches the spec's framing of activation as an explicit administrator action and the audit design (distinct action type).

**Alternatives considered**: Status as a form dropdown — rejected for the accidental-suspension risk; modal-only create — rejected: a routed page (`/platform/tenants/new`) gets breadcrumbs, deep-linking, and guard coverage for free.
