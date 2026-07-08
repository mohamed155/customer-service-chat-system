# Research: Multi-Tenancy Foundation

**Feature**: 006-multi-tenancy-foundation | **Date**: 2026-07-08

Technical Context contains no `NEEDS CLARIFICATION` (the three scope-level forks were settled in `/speckit-clarify`). Research resolves the design decisions the spec and clarifications leave to planning.

## R1. Where isolation logic lives (module boundaries)

- **Decision**: `identity` module crate owns the principal: `Principal { user_id, display_name, platform_role: Option<PlatformRole>, memberships: … }` plus the principal-resolution middleware. `tenancy` module crate owns `TenantContext`, the tenant-context middleware, authorization queries, audit writes, and this feature's four route handlers. The server crate only mounts routes and layers middleware; `kernel` stays business-free (it already provides the error envelope).
- **Rationale**: Constitution I — modules behind clear interfaces, extractable later. Identity (who you are) and tenancy (where you may act) are distinct bounded contexts that later features (auth, RBAC) will grow independently.
- **Alternatives considered**: Everything in the server crate (violates modular monolith); everything in `tenancy` (identity would then need extraction the moment real auth lands); a new `middleware` shared crate (unnecessary layer — Axum middleware is just functions exported by module crates).

## R2. Principal resolution & dev identity header (FR-019)

- **Decision**: Header `X-Dev-User-Id: <user uuid>`. Middleware behavior: if `config.app_environment` is `development` or `test`, resolve the header value against `users` (`deleted_at IS NULL`) and attach `Principal` as a request extension; malformed/unknown id or absent header ⇒ no principal. In `production`/`staging` the header is ignored entirely (treated as absent). Endpoints requiring identity return kernel `401 unauthenticated` when no principal exists. The resolution seam is one function so real auth replaces the *source* (session/token → user id) without touching consumers.
- **Rationale**: Satisfies FR-019's hard-disable exactly (env comes from spec-004's validated `AppConfig`); lets tests exercise the whole isolation matrix by just varying a header; zero credential surface added.
- **Alternatives considered**: Cargo feature flag to compile the header out of release builds (stronger but complicates CI images and staging smoke tests; env-gating from validated config is sufficient and observable); static seeded principal (can't vary users per request — rejected in clarify).

## R3. Tenant-context middleware semantics (FR-001…FR-009)

- **Decision**: A single `tenant_context_middleware` applied to tenant-scoped routes. Pipeline per request: (1) missing `X-Tenant-ID` ⇒ 400 `validation_failed`; (2) non-UUID value ⇒ 400 `validation_failed`; (3) load tenant by PK where `deleted_at IS NULL` — absent ⇒ 403 `unauthorized` (anti-enumeration, same body as denial); (4) authorize: platform principal ⇒ allowed (any status incl. suspended); tenant principal ⇒ active membership required AND tenant status must be `active` (suspended ⇒ 403 with suspension message — message differs, code stays `unauthorized`); (5) attach `TenantContext { tenant_id, tenant_status, principal_kind }` as request extension; handlers/data access read tenant_id only from it. Platform-scoped routes (`/me`, `/platform/*`) are mounted outside this middleware (FR-004) and ignore the header.
- **Rationale**: Mirrors the spec's validation-then-authorization order (FR-002), keeps one enforcement point (FR-003/FR-008), and encodes the suspended-tenant asymmetry (FR-006/FR-007). Message-only differentiation for suspension avoids a distinguishable error code that would leak state to outsiders — only members (who already know the tenant exists) ever receive it.
- **Alternatives considered**: Postgres row-level security (heavier operational model, SQLx-unfriendly session GUCs; the platform's chosen enforcement point is the app's data-access layer per Constitution II — RLS can be layered later); per-handler extractors without middleware (repeats validation per route, easy to forget — violates "resolved once per request").

## R4. Authorization queries (performance, FR-009)

- **Decision**: Two prepared queries, no caching: (a) tenant fetch: `SELECT id, status FROM tenants WHERE id = $1 AND deleted_at IS NULL`; (b) membership check (tenant principals only): `SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL`. (b) is served by the `tenant_memberships_tenant_user_active_uniq` partial index; (a) by the PK. Principal resolution adds one indexed `users` PK lookup. Worst case: 3 index lookups per request; platform principals skip (b).
- **Rationale**: FR-009 forbids cross-request allow-decision caching; these lookups are single-digit-microsecond index probes, so correctness costs almost nothing. Combining (a)+(b) into one LEFT JOIN was measured as premature cleverness — two trivial queries are clearer and (b) is skipped for platform users.
- **Alternatives considered**: Request-scoped caching only (already implicit — context resolved once per request); TTL cache of memberships (violates FR-009's next-request guarantee).

## R5. Endpoints (alignment with 001 REST contract)

- **Decision**: Four endpoints under `/api/v1`:
  - `GET /api/v1/me` — principal profile + platform role + active memberships (tenant id, name, slug, role). Identity-required, tenant-context-free. Serves frontend bootstrap (switcher visibility, tenant-user default tenant).
  - `GET /api/v1/platform/tenants` — tenant directory for the switcher: id, name, slug, status; cursor pagination + `q` search per kernel `Page`/001 conventions. Platform principals only (403 otherwise).
  - `POST /api/v1/platform/tenants/{id}/switch` — the explicit switch action: validates the tenant exists (not deleted), writes the `platform.tenant_switched` audit record, returns the tenant summary. Stateless — no server-side session mutation (clarification #2). Platform principals only.
  - `GET /api/v1/tenant` — the first real tenant-scoped endpoint (own-tenant profile from 001): returns the active tenant's profile; runs under the tenant-context middleware and reads `TenantContext` — doubles as the isolation matrix's probe target.
- **Rationale**: All four exist in 001's `rest-api.md` surface (`/me`, `/platform/tenants`, `/platform/tenants/{id}/switch`, `/tenant`), so no new API vocabulary is invented. `GET /tenant` gives the matrix a genuinely useful probe instead of a throwaway test route. 001's `DELETE /platform/switch` (exit switcher) is client-side in a stateless model — dropping the selection needs no server call; noted in the contract.
- **Alternatives considered**: A synthetic `/api/v1/tenant-scoped-ping` probe (adds throwaway surface); implementing `GET /me/tenants` separately (memberships already fit naturally in `/me` for this feature's needs; the dedicated endpoint can come with user management).

## R6. Audit records (FR-012, FR-013)

- **Decision**: Two audit actions written via feature 005's `audit_logs`:
  - `platform.tenant_switched` — actor = platform user, `tenant_id` = target tenant, details `{ "tenant_slug": … }`; written synchronously inside the switch handler (the switch response confirms the audit exists).
  - `tenant.access_denied` — written from the middleware on FR-005/FR-007 denials: actor = principal (if any), `resource_type = "tenant"`, details `{ "requested_tenant_id": "<raw>", "reason": "no_membership" | "suspended" }`. **`tenant_id` column stays NULL** — the requested tenant may not exist, and `audit_logs.tenant_id` is an FK; the requested id lives in `details` instead. Insert failures are logged and do not change the 403 outcome (denial must not depend on audit availability).
  - 400-level rejections (missing/malformed header) are *not* audited — they carry no probe signal and would flood the log; they remain visible in traces.
- **Rationale**: Uses the append-only substrate exactly as designed in 005 (nullable FK + flexible `details`); synchronous write on switch keeps SC-004's 100% traceability testable; fail-open-on-audit-error keeps the security decision independent of a secondary write.
- **Alternatives considered**: Auditing into tracing/logs only (fails SC-004's "traceable in the audit record"); background channel for denial writes (premature — volume is trivial until rate limiting exists).

## R7. CORS & headers

- **Decision**: Add `X-Tenant-ID` and `X-Dev-User-Id` to the router's CORS `allow_headers` (dev-identity header is harmless to allow in prod CORS — the server ignores it there; but gate it anyway: only include `X-Dev-User-Id` in the allow-list when the environment permits it, so production preflights advertise nothing dev-shaped).
- **Rationale**: The dashboard calls from `http://localhost:4200` with credentials-less CORS; without the allow-list additions every tenant-scoped call fails preflight.
- **Alternatives considered**: Wildcard allow-headers (loses the explicit contract the current router expresses).

## R8. Frontend state shape (per spec-002 state rules)

- **Decision**: Global cross-feature state ⇒ NgRx Store: new `tenantContext` feature slice alongside `appUi` in `core/state/`: `{ activeTenant: TenantSummary | null, status: 'idle'|'switching'|'error' }`. A persistence effect mirrors platform users' selection to localStorage (`app.tenant`) and rehydrates on init, discarding stale selections when validation fails (FR-016). `CurrentUserService` fetches `/me` once at shell bootstrap and exposes it as a signal; `TenantContextService` is the façade features use (select/switch/clear) — it calls the switch endpoint, then updates the store. The interceptor reads the active tenant via the store's selector signal.
- **Rationale**: CLAUDE.md's state law: cross-feature global ⇒ NgRx Store (this is exactly that — theme-like, but for tenancy); persistence mirrors the theme effect pattern already in `app-ui.effects.ts`; a service façade keeps components off raw store dispatch (matches existing conventions).
- **Alternatives considered**: SignalStore in `core/tenant` (spec-002 reserves SignalStore for feature-local state); component-level signal (it's global by definition).

## R9. Frontend interceptors & guard

- **Decision**: Two functional interceptors in `core/http/`, registered after the existing `authTokenInterceptor`: `tenantContextInterceptor` attaches `X-Tenant-ID: <activeTenant.id>` to requests targeting `apiBaseUrl` when an active tenant exists (platform-scoped calls like `/me`, `/platform/*` are excluded by path prefix); `devIdentityInterceptor` attaches `X-Dev-User-Id` from a dev-only setting (localStorage `app.devUserId`) **only when** `APP_CONFIG.environmentName === 'development'` — production builds compile the no-op path. The existing `areaAccessGuard` seam gains the platform/tenant area distinction: platform area requires a platform-role principal; tenant area requires an active tenant context (platform users are prompted to select one — US2/AC4).
- **Rationale**: FR-014's "single source of truth, no hand-rolled headers" is precisely an interceptor; the dev identity mirrors the backend's env-gating so the pair disappears together in production; the guard update turns the existing placeholder into the spec's "prompt to select a tenant" behavior without inventing new routing machinery.
- **Alternatives considered**: Attaching the header inside `ApiService` (misses any future direct `HttpClient` use; interceptors are the platform-blessed seam); route-level resolvers for tenant context (over-engineered for a header).

## R10. Forbidden-state UX (FR-017)

- **Decision**: Extend the existing `http-error.mapper.ts` path: a 403 with code `unauthorized` on a tenant-scoped request surfaces a Taiga-wrapped alert/banner "You don't have access to this tenant", and — when the active tenant caused it (e.g., stale persisted selection) — the `tenantContext` slice clears the selection and returns platform users to the "no tenant selected" prompt. No partial rendering: the failing request's feature shows its error state, never another tenant's cached data (the store holds no cross-tenant data to leak).
- **Rationale**: Reuses the error-mapping seam spec 002 built; keeps the UX rule (clear message, no partial data) enforceable in one place.
- **Alternatives considered**: Global redirect to an error page on any 403 (too blunt — platform-scoped 403s like non-platform users probing `/platform/tenants` just need a message).

## R11. Isolation-matrix test strategy (FR-018, SC-006)

- **Decision**: `backend/crates/server/tests/tenancy.rs`, live-gated on `DATABASE_URL` (spec-004/005 pattern), driving the real router via `tower::ServiceExt::oneshot` with seeded data (unique-per-test users/tenants/memberships, as in 005's schema tests). Matrix rows: tenant user in own tenant (200), foreign tenant (403), platform user in any tenant (200), suspended tenant (member 403 + platform 200), missing header (400), malformed header (400), nonexistent tenant (403, body identical to foreign-tenant 403), revoked membership (403 on next request), dev header ignored when env=staging/production (401), switch action writes audit row, denial writes `tenant.access_denied` row. Frontend: Vitest specs for interceptor header attachment/exclusion, store persistence/rehydration/discard, switcher visibility by principal kind, and forbidden-state mapping.
- **Rationale**: `oneshot` against the assembled router tests the wiring (middleware order, CORS, envelope) rather than functions in isolation; asserting byte-identical 403 bodies pins the anti-enumeration property; env-gating test pins FR-019.
- **Alternatives considered**: Unit-testing middleware with mocked pools (misses wiring and SQL correctness; used only for pure helpers like header parsing).

## R12. Trace enrichment (Constitution VI)

- **Decision**: The tenant-context middleware records `tenant.id` and the identity middleware records `principal.id`/`principal.kind` onto the current tracing span (fields on spec-004's `trace_middleware` span). No new metrics in this feature.
- **Rationale**: Makes cross-tenant debugging and denial investigation possible from traces alone; two `span.record` calls, no new infrastructure.
- **Alternatives considered**: Dedicated tenancy metrics (denial counters) — deferred until an operational dashboard feature exists.
