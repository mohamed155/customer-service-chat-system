# Feature Specification: Multi-Tenancy Foundation

**Feature Branch**: `006-multi-tenancy-foundation`

**Created**: 2026-07-08

**Status**: Draft

**Input**: User description: "Multi-Tenancy Foundation — Implement tenant context and isolation. Scope: tenant model, active tenant resolution, X-Tenant-ID handling, tenant authorization, platform user tenant switching, tenant user restrictions. Backend: tenant context middleware, validate X-Tenant-ID, platform users can access any tenant, tenant users only assigned tenants. Frontend: tenant context service, tenant switcher for platform users only, hidden from tenant users. Acceptance: tenant users cannot access other tenants, platform users can switch active tenant, API requests include active tenant context, unauthorized tenant access returns forbidden error, tests verify tenant isolation."

## Clarifications

### Session 2026-07-08

- Q: How should the authenticated principal be supplied until the real authentication feature exists? → A: A development/test-only identity header resolves the principal from the users table; hard-disabled outside development/test environments. Real auth later replaces the principal source; isolation logic unchanged.
- Q: How is a platform user's tenant switch represented on the server? → A: Stateless — tenant context lives only in each request's `X-Tenant-ID`; switching means the client calls a lightweight switch action that validates access and writes the audit record, then sends the new header thereafter. No server-side session state.
- Q: Should the frontend tenant context service and switcher call the real backend API in this feature, or stay fixture-driven? → A: Real API now — the switcher lists tenants from the real API and the switch action + `X-Tenant-ID` propagation run against the real backend (dev identity header in development). First real HTTP integration in the dashboard; existing fixture pages stay untouched.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Tenant Isolation Enforcement (Priority: P1)

Every request that touches tenant-owned data carries an explicit tenant context, and the system verifies — on the server, for every such request — that the requesting user is allowed to act within that tenant. A tenant user (Owner, Admin, Manager, Agent, Viewer) can only ever operate inside tenants where they hold an active membership; any attempt to reach another tenant's data is rejected with a forbidden error that reveals nothing about the other tenant.

**Why this priority**: This is the existential guarantee of the platform (Constitution Principle II: "a single cross-tenant data leak is an existential trust failure"). Nothing tenant-scoped can safely ship before this enforcement exists.

**Independent Test**: With two tenants (A and B) and a tenant user who is a member of A only: requests in tenant A's context succeed; identical requests in tenant B's context — or with a nonexistent tenant, a malformed tenant identifier, or no tenant at all — are rejected with the standard forbidden/validation error. Verifiable entirely through the API with automated tests.

**Acceptance Scenarios**:

1. **Given** a tenant user with an active membership in tenant A, **When** they make a tenant-scoped request in tenant A's context, **Then** the request is processed within tenant A only.
2. **Given** the same user, **When** they make a request in tenant B's context (where they have no active membership), **Then** the request is rejected with a forbidden error and no tenant-B data or existence information is disclosed.
3. **Given** any user, **When** they make a tenant-scoped request with a malformed tenant identifier, **Then** the request is rejected with a validation error before any data access occurs.
4. **Given** any user, **When** they make a tenant-scoped request naming a tenant that does not exist (or is deleted), **Then** the response is indistinguishable from the unauthorized case (forbidden), so tenant existence cannot be probed.
5. **Given** a tenant user whose membership in tenant A is deactivated (soft-deleted), **When** they make their next request in tenant A's context, **Then** it is rejected with a forbidden error.
6. **Given** a tenant-scoped request with no tenant context supplied, **When** it reaches the system, **Then** it is rejected with a validation error identifying the missing tenant context.

---

### User Story 2 - Platform User Tenant Switching (Priority: P2)

A platform user (Super Admin, Developer, Sales, Support, Finance) can select any existing tenant as their active working context via a tenant switcher, operate within that tenant as needed for support or administration, and switch to another tenant at will. Each switch takes effect immediately for subsequent work, and every switch is recorded in the audit trail (who, which tenant, when).

**Why this priority**: Platform staff need cross-tenant access to run the business (support, billing, debugging), and the constitution names the Tenant Switcher as the mechanism. It builds directly on the isolation layer from Story 1 — the same check that blocks tenant users is what admits platform users.

**Independent Test**: As a platform user, pick tenant A from the switcher and make a tenant-scoped request (succeeds in A's context); switch to tenant B and repeat (succeeds in B's context); verify both switches appear in the audit log with actor, tenant, and timestamp.

**Acceptance Scenarios**:

1. **Given** a platform user, **When** they open the tenant switcher, **Then** they can find and select any existing active tenant as their working context.
2. **Given** a platform user who has selected tenant A, **When** they perform tenant-scoped work, **Then** it executes in tenant A's context, and **When** they switch to tenant B, **Then** subsequent work executes in tenant B's context with no residue of tenant A.
3. **Given** a platform user switches their active tenant, **When** the switch occurs, **Then** an audit record is created capturing the actor, the selected tenant, and the time.
4. **Given** a platform user with no tenant selected, **When** they use the product, **Then** they can access platform-level (non-tenant) functions, and tenant-scoped functions prompt them to select a tenant.
5. **Given** a tenant is suspended, **When** a platform user selects it, **Then** they can still work within it (support staff need access to suspended tenants), while tenant users of that tenant are refused with a forbidden error.

---

### User Story 3 - Frontend Tenant Context Propagation (Priority: P3)

The web application always knows which tenant the signed-in user is working in and attaches that tenant context to every API request automatically — no page or feature has to remember to do it. Platform users see the tenant switcher in the application shell; tenant users never see it, and their active tenant resolves automatically from their membership.

**Why this priority**: This makes the isolation and switching mechanics usable and mistake-proof in the product. It depends on Stories 1 and 2 for the server-side behavior it surfaces.

**Independent Test**: Sign in as a platform user — the switcher is visible, selecting a tenant causes subsequent API requests to carry that tenant's context, and the selection survives a page reload. Sign in as a tenant user — no switcher is rendered anywhere, and API requests automatically carry their own tenant's context.

**Acceptance Scenarios**:

1. **Given** a signed-in user with an active tenant context, **When** the application makes any tenant-scoped API request, **Then** the active tenant context is attached automatically.
2. **Given** a platform user, **When** they use the application shell, **Then** the tenant switcher is available, and selecting a tenant updates the active context for all subsequent requests without a full re-authentication.
3. **Given** a tenant user, **When** they use the application, **Then** no tenant switcher is rendered anywhere in the interface, and their active tenant is resolved from their membership without any action on their part.
4. **Given** a platform user who selected tenant A and then reloads the page, **When** the application restarts, **Then** tenant A is still the active context.
5. **Given** the server rejects a request with a forbidden tenant error, **When** the frontend receives it, **Then** the user sees a clear "no access to this tenant" message rather than a raw error, and no partial cross-tenant data is displayed.

---

### Edge Cases

- What happens when a tenant user belongs to multiple tenants? Their active tenant defaults to their primary (single or first) membership; the server accepts any of their assigned tenants as context, but an in-app switcher for tenant users is explicitly out of scope this feature (per the feature description, the switcher is platform-user-only).
- What happens when a platform user's stored tenant selection points at a tenant that was deleted since? The selection is discarded and they return to the "no tenant selected" state with a prompt to choose another.
- What happens when a tenant is suspended mid-session? Tenant users' next request is refused (forbidden with a suspension-appropriate message); platform users retain access.
- What happens when a user's role changes (tenant user gains a platform role, or vice versa) mid-session? Tenant-context authorization reflects the current stored state on the next request — no stale allow decisions are cached beyond a request.
- What happens when an unauthorized tenant access is attempted? Beyond the forbidden response, the attempt is recorded (actor, requested tenant, time) so repeated probing is visible to operators.
- What happens to platform-level endpoints (no tenant context)? They ignore any supplied tenant context and remain governed by platform-role authorization only.

## Requirements *(mandatory)*

### Functional Requirements

**Tenant context resolution**

- **FR-001**: Every tenant-scoped API request MUST carry an explicit tenant context via the `X-Tenant-ID` request header; requests to tenant-scoped operations without it MUST be rejected with a validation error.
- **FR-002**: The system MUST validate the supplied tenant context before any tenant data is touched: a malformed identifier is rejected with a validation error; a well-formed identifier that does not correspond to an existing, non-deleted tenant is rejected with a forbidden error indistinguishable from the unauthorized case (no tenant-existence leak).
- **FR-003**: Tenant context MUST be resolved once per request in the request-processing pipeline and made available to all downstream handlers and data access for that request; no handler may substitute a different tenant mid-request.
- **FR-004**: Platform-scoped operations MUST NOT require tenant context and MUST ignore any supplied tenant context for authorization purposes.

**Tenant authorization**

- **FR-005**: A tenant user MUST be authorized for a tenant-scoped request only when they hold an active (non-deleted) membership in the requested tenant; otherwise the request MUST be rejected with a forbidden error in the platform's standard error envelope.
- **FR-006**: A platform user (any user with a platform role) MUST be authorized to act within any existing, non-deleted tenant, including suspended tenants.
- **FR-007**: Requests from tenant users to a suspended tenant MUST be rejected with a forbidden error carrying a suspension-appropriate message; the tenant's data remains intact and platform-user-accessible.
- **FR-008**: Tenant authorization MUST be enforced in the server's request pipeline and honored by the data-access layer for every tenant-owned table (per Constitution Principle II); client-side checks MUST NOT be the sole enforcement anywhere.
- **FR-009**: Authorization decisions MUST reflect the current stored membership/role state — a revoked membership or role change takes effect no later than the next request (no cross-request caching of allow decisions).

**Platform user tenant switching**

- **FR-010**: Platform users MUST be able to list/search existing tenants and select any one as their active tenant context.
- **FR-011**: A platform user's tenant switch MUST take effect for all subsequent requests immediately, with no re-authentication. Tenant context is stateless on the server: it exists only in each request's `X-Tenant-ID`, and the server holds no per-user "active tenant" state.
- **FR-012**: Switching MUST be an explicit action: the client invokes a lightweight switch operation that validates the platform user's access to the selected tenant and records the switch in the audit log (actor, selected tenant, timestamp — audit substrate from feature 005); the client then carries the new tenant context on subsequent requests.
- **FR-013**: Unauthorized tenant-access attempts (forbidden outcomes from FR-005/FR-007) MUST be recorded (actor if known, requested tenant identifier, time) so probing is observable.

**Frontend tenant context**

- **FR-014**: The web application MUST maintain a single source of truth for the active tenant context and attach it automatically (as `X-Tenant-ID`) to every tenant-scoped API request; individual features MUST NOT hand-roll tenant headers.
- **FR-014a**: The tenant switcher and tenant context service MUST operate against the real backend API in this feature (tenant listing, switch action, context propagation) — not against fixtures; existing fixture-driven dashboard pages are unaffected.
- **FR-015**: The tenant switcher MUST be rendered only for platform users; tenant users MUST never see it, and their active tenant MUST resolve automatically from their membership.
- **FR-016**: A platform user's active tenant selection MUST survive a page reload; a stored selection referencing a tenant that no longer exists MUST be discarded gracefully (return to "no tenant selected").
- **FR-017**: When the server refuses a request for tenant-authorization reasons, the application MUST present a clear "no access to this tenant" state and MUST NOT render partial data from another tenant.

**Verification**

- **FR-018**: Automated tests MUST cover the isolation matrix: tenant user in own tenant (allowed), tenant user in foreign tenant (forbidden), platform user in any tenant (allowed), suspended tenant (tenant user forbidden / platform user allowed), missing/malformed/nonexistent tenant context (rejected), and revoked membership (forbidden on next request).
- **FR-019**: Until real authentication ships, the authenticated principal MUST be supplied by a development/test-only identity header that resolves an existing user; this mechanism MUST be hard-disabled outside development and test environments (a request carrying it in production is treated as unauthenticated), and replacing it with real authentication MUST require no change to tenant-authorization logic.

### Key Entities

- **Tenant Context**: The per-request working tenant — resolved from the `X-Tenant-ID` header, validated against the tenants table (feature 005), and carried through the request pipeline. Not a stored table and not server-side session state; a stateless runtime concept with a defined lifecycle (resolve → authorize → propagate), fresh on every request.
- **Authenticated Principal**: The identity making the request — a user (feature 005 `users`) with either a platform role (platform user) or one or more active tenant memberships (tenant user). This feature consumes the principal; producing it (login/sessions) is out of scope (see Assumptions).
- **Tenant Membership** (existing, feature 005): The authorization source of truth for tenant users — active membership in the requested tenant grants access; its role value is available to downstream features (fine-grained RBAC is a later feature).
- **Audit Log Entry** (existing, feature 005): Records platform-user tenant switches and unauthorized access attempts.
- **Active Tenant Selection (frontend)**: The client-held current tenant for the signed-in user — switcher-driven for platform users (persisted across reloads), membership-derived for tenant users.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In isolation testing across two seeded tenants, 100% of cross-tenant access attempts by tenant users are refused with the standard forbidden error, and 0 responses contain another tenant's data or confirm another tenant's existence.
- **SC-002**: A platform user can locate a tenant in the switcher and complete a context switch in under 10 seconds, and their next action executes in the new tenant's context on the first request (no retry, no re-login).
- **SC-003**: 100% of tenant-scoped API requests issued by the web application carry the active tenant context automatically — zero features implement their own tenant-context handling.
- **SC-004**: 100% of platform-user tenant switches and 100% of forbidden tenant-access outcomes are traceable in the audit/security record (who, which tenant, when).
- **SC-005**: The tenant switcher appears for 100% of platform-user sessions and 0% of tenant-user sessions.
- **SC-006**: The automated isolation test matrix (FR-018) passes in CI on every change; a regression in any isolation rule fails the build.

## Assumptions

- **Authenticated principal is provided, not built here**: Login, sessions, and credential handling are a separate feature (deferred by feature 005 as well). This feature assumes the request pipeline can identify the current user (id, platform role, memberships) via an authentication abstraction; per clarification, a development/test-only identity header supplies the principal (hard-disabled outside dev/test — see FR-019) so the isolation matrix is fully testable now. The isolation rules themselves are auth-mechanism-agnostic.
- **Tenant model exists**: The `tenants`, `users`, `tenant_memberships`, and `audit_logs` tables from feature 005 are the data foundation; "tenant model" in this feature's scope means the runtime tenant context and authorization behavior, not new tables.
- **Tenant users with multiple memberships**: The server authorizes any of their assigned tenants; the client defaults their active tenant to their single/primary membership. A tenant-user-facing switcher is out of scope (the feature description restricts the switcher to platform users).
- **Platform-user selection persistence**: The active tenant selection persists across page reloads on the same browser (like the existing theme preference); it is a UX convenience, not an authorization artifact — the server re-authorizes every request.
- **Error semantics**: Forbidden responses use the platform's standard error envelope (feature 004); "nonexistent tenant" and "no access" intentionally share the same forbidden response to prevent tenant enumeration, while malformed identifiers fail validation distinctly.
- **Fine-grained permissions are later**: This feature decides only *whether* a user may act within a tenant. What they may do inside it (role-based permissions per Owner/Admin/Manager/Agent/Viewer) is the RBAC feature's concern.
- **Existing widget/dashboard fixture pages**: Current dashboard pages are fixture-driven (spec 003) with no real API calls and remain untouched. Per clarification, this feature introduces the dashboard's first real API integration: the tenant context service, switcher (tenant listing + switch action), and automatic `X-Tenant-ID` propagation all run against the real backend, using the dev identity header in development. Features that later call the real API adopt tenant context automatically.
