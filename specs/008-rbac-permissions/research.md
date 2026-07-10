# Research: RBAC & Permissions

**Feature**: 008-rbac-permissions | **Date**: 2026-07-10

All unknowns from Technical Context resolved below. No NEEDS CLARIFICATION markers remain.

## R1. Where the role→permission mapping lives

- **Decision**: Code-defined static matrix in a new `authz` module crate (`const`/`match`-based functions returning `&'static [Permission]`), not database tables.
- **Rationale**: The spec fixes the catalog per release ("Static role catalog" assumption; no role-administration UI in scope). A code matrix is the single source of truth (FR-002), versioned with the release, exhaustively testable with `match` (compiler forces every role to be mapped), and adds zero query cost (Constitution X). The same matrix serializes into `/me` so the frontend never re-derives it (FR-010).
- **Alternatives considered**: (a) `permissions` + `role_permissions` DB tables — rejected: enables runtime drift from the shipped product definition, needs migrations/seeding for no in-scope benefit, and adds per-request joins. Revisit when custom roles become a feature. (b) Config file (YAML/TOML) — rejected: loses compile-time exhaustiveness and type safety.

## R2. Permission naming scheme

- **Decision**: Dot-scoped snake_case string codes, `<area>.<action>`, with a `platform.` prefix for platform-scope permissions. Tenant examples: `conversations.view`, `conversations.manage`, `settings.manage`, `billing.manage`, `tenant.delete`, `members.manage`. Platform examples: `platform.tenants.list`, `platform.tenants.switch`, `platform.admin`, `platform.billing.view`, `platform.diagnostics.view`. Canonical list in `contracts/permissions.md`.
- **Rationale**: Matches the existing snake_case role codes (`super_admin`, `agent`) and audit action naming (`platform.tenant_switched`). `view`/`manage` pairs cover the current page-level needs without inventing per-verb granularity the product doesn't have yet; new verbs can be added area-by-area later (extension point).
- **Alternatives considered**: `resource:action` colon style (common in OAuth scopes) — rejected for consistency with existing dot-style audit actions; fine-grained CRUD verbs (`create/read/update/delete`) — rejected as speculative granularity with no current consumer.

## R3. Fail-closed enforcement mechanism (FR-003, edge case "new endpoint without declaration")

- **Decision**: A route-registration convention enforced by API shape: the server's `/api/v1` router is built from three explicit groups — `public_routes()` (auth login/logout only), `platform_routes(...)`, and `tenant_routes(...)` — where the group builders take the required `Permission` per route as a non-optional argument (`.guarded(path, method_router, Permission::X)`). A route physically cannot be added to a protected group without naming a permission. `require_permission` is implemented as an Axum `route_layer` (via `from_fn_with_state`) that reads the request's computed `PermissionSet` extension and returns `ApiError::unauthorized("Access denied")` (403) when the permission is absent — including when no `PermissionSet` was attached at all (deny on missing state, not just missing permission).
- **Rationale**: Rust's type system makes "forgot to declare" a compile error instead of a runtime hole; the deny-on-missing-extension rule closes the middleware-ordering failure mode. An integration test additionally sweeps every registered `/api/v1` route with an unprivileged principal and asserts non-2xx (belt and braces for SC-001).
- **Alternatives considered**: (a) Proc-macro attribute on handlers — rejected: more machinery than the route count justifies. (b) Runtime route-table introspection asserting coverage — kept only as the test-side sweep, not the primary mechanism. (c) Per-handler extractor `RequirePermission<{const}>` — rejected: const-generic string params are awkward on stable Rust and scatter declarations into handler signatures.

## R4. Immediate role-change propagation (FR-011)

- **Decision**: Server side — already satisfied structurally and kept that way: `principal_middleware` resolves the user row (including `platform_role`) from PostgreSQL on every request, and `tenant_context_middleware` resolves membership per request; the membership query is widened to return `role` so the effective `PermissionSet` is always computed from live data. No role or permission data is ever embedded in the JWT. Client side — the dashboard refreshes its permission snapshot (re-fetches `/me`) on: (a) any 403 `unauthorized` API response (via the existing error interceptor), (b) tenant switch, and (c) app navigation after the initial load resolves. A revoked user's next interaction therefore hits a live server check, receives 403, and the UI immediately re-syncs and re-routes.
- **Rationale**: The server boundary is the security guarantee and it is per-request-fresh by construction. For the UI, 403-triggered refresh gives "immediate on next interaction" behavior with zero new infrastructure; a user who never interacts can see stale *navigation*, but cannot *do* anything stale — the server refuses.
- **Alternatives considered**: WebSocket/SSE push of permission changes — rejected: real-time infra for a rare event; disproportionate to the requirement and nothing else in the platform uses push yet. Short-TTL polling of `/me` — rejected: constant background load for the same rare event; the 403-triggered refresh achieves the observable outcome.

## R5. Environment model for FR-005a (dev/qa/stg/prod vs existing enum)

- **Decision**: Reuse the existing `config::Environment` enum (Production, Staging, Development, Test) unchanged. The staff-in-tenant matrix keys off a single boolean: `is_production = matches!(env, Environment::Production)`. A `qa` deployment sets `ENVIRONMENT=staging` (or `test`); no new enum variant.
- **Rationale**: The clarification's intent is "restrict only in production"; every non-production variant behaves identically (full staff access), so a dedicated `Qa` variant would add a config surface with no behavioral difference. Existing dev-header gating in `principal_middleware` already treats Staging as production-like for *authentication*, which is unaffected by this decision (authorization ≠ dev-login gating).
- **Alternatives considered**: Adding `Qa` to the enum — rejected: touches config parsing, `.env.example`, CI, and tests for zero behavior change. A separate `RBAC_STAFF_FULL_ACCESS` flag — rejected: two knobs that can contradict each other; environment is already the deployment-level signal the spec names.

## R6. Where effective permissions are computed and carried (backend)

- **Decision**: `tenant_context_middleware` computes the effective tenant `PermissionSet` once per request (tenant user → from membership role; platform user → from `staff_tenant_permissions(platform_role, is_production)`) and stores it on `TenantContext` / request extensions. Platform-scope routes (no tenant header) get their `PermissionSet` from a lightweight `platform_authz_middleware` deriving from `Principal.platform_role`. `PermissionSet` is a `bitflags`-style or `HashSet<Permission>` wrapper with `contains(Permission)`.
- **Rationale**: One computation per request, at the same layer that already does tenant authorization — keeps enforcement at the data-access boundary (Constitution II) and makes `require_permission` a pure in-memory check. Extending `TenantContext` follows the established extension pattern (it already carries `principal_kind`).
- **Alternatives considered**: Computing inside each `require_permission` layer — rejected: repeats matrix lookup and DB-derived state assembly per layered route; harder to test. Putting the set on `Principal` — rejected: tenant permissions depend on the active tenant, which `principal_middleware` doesn't know.

## R7. Frontend permission delivery and consumption

- **Decision**: Extend `GET /me` (single existing bootstrap call) with server-computed permission arrays: `platformPermissions` (platform scope), per-membership `permissions` (that tenant's effective set for this user), and `staffTenantPermissions` (for platform staff: the environment-resolved set they hold inside any tenant). A new `core/authz/PermissionsService` derives a signal `Set<string>` of effective permissions from `CurrentUserService` + the active tenant from the NgRx `tenantContext` slice, exposing `has(permission): boolean` (computed). Consumers: `permissionGuard` (`CanMatch`, reads `route.data['requiredPermission']`, redirects to the user's first allowed tenant page, falling back to `/tenant/select`), sidebar nav filtering, and a `*appHasPermission` structural directive for in-page actions (FR-009).
- **Rationale**: One endpoint, one snapshot, server as sole source of truth (FR-010); signals keep every consumer reactive so a `/me` refresh instantly re-filters nav and re-evaluates guards (FR-011 UI half). `CanMatch` prevents the lazy route from even loading — restricted content never renders transiently (FR-008 / SC-003).
- **Alternatives considered**: Separate `GET /me/permissions?tenant=` endpoint — rejected: second round-trip on every switch; `/me` already carries memberships. Shipping the whole role→permission matrix to the client and resolving locally — rejected: violates FR-010 (client-side mapping drift) and leaks environment logic to the client.

## R8. Redirect target for blocked navigation (spec's one Outstanding UX item)

- **Decision**: The guard redirects to the user's *first permitted* tenant page in sidebar order (overview → conversations → …); if the user has an active tenant but zero permitted pages, or no active tenant, redirect to `/tenant/select`; a user with no memberships and no platform role lands on `/tenant/select`, which renders its existing empty state (safe landing, no error loop).
- **Rationale**: "First allowed page" degrades gracefully for every role without a new "access denied" page; reuses the existing tenant-select empty state for the zero-permission edge case.
- **Alternatives considered**: Dedicated 403 page — rejected: dead-end UX for a state users mostly reach by stale links; can be added later if support burden shows a need.

## R9. Denial observability & audit (Constitution III/VI)

- **Decision**: `require_permission` records the denied permission on the current tracing span (`authz.denied_permission`) and emits a `tracing::warn!`. Tenant-scope denials additionally reuse the existing `tenancy::audit::access_denied` append-only path with reason `permission_denied` (alongside the existing `not_found` / `no_membership` / `suspended` reasons). Unrecognized stored role values are logged at `error` level and treated as no role (deny) — the spec's "observable to operators" edge case.
- **Rationale**: Reuses established audit and tracing plumbing; no new tables or log formats.
- **Alternatives considered**: Auditing every denial including platform-scope — accepted partially: platform-scope denials get span/log records but not `audit_logs` rows (no tenant to attribute; can be added when a platform audit viewer exists).
