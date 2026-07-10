# Feature Specification: Authentication

**Feature Branch**: `007-authentication`

**Created**: 2026-07-09

**Status**: Draft

**Input**: User description: "Authentication — Allow users to securely sign in and access the platform. Scope: login, logout, JWT generation, JWT validation, password hashing, auth middleware, current user endpoint, frontend auth state, route guards. Backend: POST /auth/login, POST /auth/logout, GET /auth/me. Frontend: login page, auth service, token storage strategy, route guard, current user loading. Acceptance: users can log in, invalid credentials are rejected, protected routes require authentication, frontend redirects unauthenticated users, tests cover login, token validation, and route protection."

## Clarifications

### Session 2026-07-09

- Q: How should the browser hold and send the session token? → A: httpOnly secure cookie set by the server — the session credential is never readable by page scripts; the browser attaches it automatically; cross-site request forgery (CSRF) protection accompanies the cookie-based session.
- Q: How long should a session remain valid after sign-in? → A: 8 hours (a work-day session); no refresh flow — an expired session requires signing in again.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Credential Sign-In (Priority: P1)

A provisioned user (platform user or tenant user) signs in with their email and password. Valid credentials establish an authenticated session and take them into the product; invalid credentials are rejected with a single generic message that never reveals whether the account exists or which part of the credentials was wrong.

**Why this priority**: Nothing else in this feature — or in any future feature that needs a real signed-in user — works without a way to establish who is calling. This replaces the development-only identity mechanism (feature 006, FR-019) as the production source of the authenticated principal.

**Independent Test**: With a seeded user whose password is known: sign in with the correct email/password (succeeds, session established, subsequent requests are authenticated); sign in with a wrong password, a nonexistent email, and a deactivated account (all rejected with the same generic error and no account-existence leak). Fully automatable via the API.

**Acceptance Scenarios**:

1. **Given** a provisioned, active user, **When** they submit their correct email and password, **Then** an authenticated session is established and their identity (including platform role or tenant memberships) is available to the application.
2. **Given** any visitor, **When** they submit a wrong password for an existing account, **Then** the attempt is rejected with a generic invalid-credentials error.
3. **Given** any visitor, **When** they submit an email that matches no account, **Then** the attempt is rejected with a response indistinguishable from the wrong-password case (no account-enumeration).
4. **Given** a user whose account has been deactivated (soft-deleted), **When** they attempt to sign in with previously valid credentials, **Then** the attempt is rejected the same way as invalid credentials.
5. **Given** a failed or successful sign-in attempt, **When** it completes, **Then** the outcome is recorded in the security/audit trail (who/what/when).

---

### User Story 2 - Protected Access & Session Continuity (Priority: P2)

Every protected part of the product — API operations and application routes — requires an authenticated session. An unauthenticated visitor who tries to open a protected page is redirected to the sign-in page, and after signing in they land on the page they originally asked for. A signed-in user's session survives a page reload; when their session expires or becomes invalid, they are returned to sign-in cleanly instead of seeing raw errors or stale data.

**Why this priority**: Enforcement is what makes authentication meaningful — sign-in without server-side protection of every endpoint is decorative. It builds directly on Story 1's session.

**Independent Test**: Call a protected API operation with no session, an expired session, and a tampered session token — all rejected with the standard unauthenticated error. In the app: open a protected route while signed out (redirected to sign-in, then returned to the original route after signing in); reload while signed in (still signed in); let the session expire (next interaction returns to sign-in).

**Acceptance Scenarios**:

1. **Given** no authenticated session, **When** any protected API operation is called, **Then** it is rejected with the platform's standard unauthenticated error before any business logic or data access runs.
2. **Given** an expired, malformed, or tampered session token, **When** it is presented on a protected request, **Then** the request is rejected exactly as if no session were present.
3. **Given** a signed-out visitor, **When** they navigate to any protected application route, **Then** they are redirected to the sign-in page, and **When** they sign in successfully, **Then** they are taken to the route they originally requested.
4. **Given** a signed-in user, **When** they reload the page or open a new tab, **Then** their session is still active without re-entering credentials (while the session remains valid).
5. **Given** a signed-in user whose session becomes invalid mid-use (expiry or revocation), **When** the application next receives an unauthenticated error from the server, **Then** the client auth state is cleared and the user is returned to sign-in with a clear message — no raw errors, no stale authenticated UI.
6. **Given** an already signed-in user, **When** they navigate to the sign-in page, **Then** they are taken into the application instead of being shown the sign-in form again.

---

### User Story 3 - Sign-Out (Priority: P3)

A signed-in user explicitly signs out. Their session is ended on the server — the session token can no longer be used to authenticate, even if a copy was retained — the client's auth state is cleared, and they are returned to the sign-in page. The sign-out is recorded in the audit trail.

**Why this priority**: Completes the session lifecycle and is a security expectation (shared machines, support staff switching accounts), but the product is usable for signed-in work with Stories 1 and 2 alone.

**Independent Test**: Sign in, capture the session token, sign out, then replay the captured token against a protected API operation — it is rejected as unauthenticated. In the app: after sign-out, protected routes redirect to sign-in and no authenticated data remains visible. The sign-out event appears in the audit trail.

**Acceptance Scenarios**:

1. **Given** a signed-in user, **When** they sign out, **Then** the client auth state is cleared, the session cookie is cleared/invalidated, and they are redirected to the sign-in page.
2. **Given** a completed sign-out, **When** the previous session token is presented on any protected request, **Then** it is rejected as unauthenticated (server-side invalidation, not just client cleanup).
3. **Given** a sign-out, **When** it completes, **Then** an audit record captures the actor and time.
4. **Given** a user signed in across two tabs, **When** they sign out in one tab, **Then** the other tab's next server interaction is rejected and it returns to the sign-in state cleanly.

---

### Edge Cases

- What happens when a valid session token references a user deleted after the token was issued? The request is rejected as unauthenticated — authentication reflects current stored user state, not just token validity.
- What happens when a tenant user of a suspended tenant signs in? Sign-in itself succeeds (their identity is valid); tenant-scoped access remains governed by feature 006's tenant authorization (forbidden with a suspension message).
- What happens on repeated failed sign-in attempts against one account? Each failure is recorded in the security/audit trail so probing and credential-stuffing patterns are observable to operators; automated lockout/throttling policy is a follow-up hardening feature (see Assumptions).
- What happens when the browser presents a session cookie that no longer validates (expired, revoked, or garbage)? The current-user fetch fails, the client treats itself as signed out, and the user starts at sign-in — never a crash or a broken half-authenticated state.
- What happens to the development/test identity mechanism from feature 006? It remains hard-disabled outside development/test exactly as before; real sign-in becomes the only production path to an authenticated principal, and tenant-authorization logic is unchanged (006 FR-019's replacement guarantee).
- What happens when a sign-in request arrives while already authenticated? A new session is established, replacing the client's previous one; the previous session's validity follows the same lifecycle rules (expiry/sign-out).

## Requirements *(mandatory)*

### Functional Requirements

**Credential verification**

- **FR-001**: Users MUST be able to sign in with their email address and password; a successful sign-in establishes an authenticated session and returns the user's identity summary (same shape as the current-user operation).
- **FR-002**: Passwords MUST be stored only as strong one-way hashes suitable for password storage — never in plaintext, never reversibly encrypted, never logged; verification compares against the stored hash.
- **FR-003**: Failed sign-in attempts — wrong password, unknown email, or deactivated (soft-deleted) account — MUST all produce the same generic invalid-credentials rejection in the platform's standard error envelope, with no content signal that distinguishes the cases (no account enumeration).
- **FR-004**: Sign-in outcomes (success and failure, with actor where known, and time) MUST be recorded in the security/audit trail per Constitution Principle III.

**Session tokens**

- **FR-005**: A successful sign-in MUST issue an integrity-protected session token that identifies the user and expires 8 hours after issuance, delivered as an httpOnly, secure session cookie — never readable by page scripts and never returned in the response body; the token MUST NOT carry sensitive data (no password material, no secrets) and MUST become unusable after its expiry.
- **FR-005a**: Because the session is cookie-carried, state-changing operations MUST be protected against cross-site request forgery; a cross-site request without valid CSRF protection MUST be rejected before any handler runs.
- **FR-006**: Every protected request MUST validate the presented session token — integrity, expiry, and that the referenced user still exists and is active — before any handler or data access runs; missing, expired, malformed, or tampered tokens MUST be rejected with the platform's standard unauthenticated error.
- **FR-007**: Session validation MUST be enforced in the server's request pipeline as the production source of the authenticated principal, replacing the development-only identity header (006 FR-019) with no change to tenant-authorization logic; the development/test identity mechanism remains available only in development/test environments.
- **FR-008**: Users MUST be able to sign out; sign-out MUST invalidate the session on the server such that the same token can no longer authenticate any request, and the sign-out MUST be recorded in the audit trail.

**Current user**

- **FR-009**: An authenticated user MUST be able to retrieve their own identity summary (id, email, display name, platform role, active tenant memberships) via the current-user operation; unauthenticated callers MUST receive the standard unauthenticated error. This is the existing current-user operation from feature 006, now driven by real sessions.

**Frontend authentication**

- **FR-010**: The web application MUST provide a sign-in page with email and password fields, visible progress feedback while the attempt is in flight, and a clear generic error message on rejection.
- **FR-011**: The web application MUST maintain a single source of truth for authentication state (the current user, resolved via the current-user operation); the session credential itself is carried automatically by the browser (httpOnly cookie) and MUST never be held, read, or attached by application code — individual features MUST NOT hand-roll authentication handling.
- **FR-012**: All application routes except the authentication screens MUST require an authenticated session; unauthenticated visitors MUST be redirected to the sign-in page, and after successful sign-in the user MUST be taken to the route they originally requested.
- **FR-013**: An authenticated session MUST survive a page reload: on startup the application re-establishes auth state by fetching the current user against the browser-held session cookie; if that fails (expired, revoked, or absent session), the user is routed to sign-in gracefully — never a crash or half-authenticated state.
- **FR-014**: When the server rejects a request as unauthenticated mid-session, the application MUST clear its auth state and return the user to sign-in with a clear message — it MUST NOT keep rendering authenticated UI or partial data.
- **FR-015**: Signing out in the application MUST invoke the server sign-out operation, clear all client-held auth state, and redirect to the sign-in page; an already-authenticated user navigating to the sign-in page MUST be redirected into the application.

**Verification**

- **FR-016**: Automated tests MUST cover: sign-in with valid credentials (success), wrong password / unknown email / deactivated account (identical generic rejection), token validation (missing, expired, malformed, tampered, revoked-by-sign-out, user-deleted-after-issuance), cookie posture (session cookie is httpOnly/secure; state-changing cross-site requests without CSRF protection are rejected), route protection (protected API operations reject unauthenticated callers; frontend guards redirect signed-out visitors and preserve the intended destination), and audit records for sign-in/sign-out events.

### Key Entities

- **User Credential**: The password verification material for a user (feature 005 `users`) — stored exclusively as a strong one-way hash appropriate to password storage. Its addition to the data model follows the platform's migration discipline.
- **Session Token**: A time-limited, integrity-protected proof of authentication issued at sign-in, carried by the browser in an httpOnly secure cookie, and presented on every protected request. Cheap to validate per request, but revocable at sign-out (a signed-out token no longer authenticates). Never readable by page scripts; a runtime credential with a defined lifecycle (issue → validate per request → expire or revoke).
- **Authenticated Principal** (existing, feature 006): The resolved identity on each request — this feature changes only how it is produced in production (real session validation instead of the dev identity header); everything that consumes it, including tenant authorization, is unchanged.
- **Audit Log Entry** (existing, feature 005): Records sign-in successes/failures and sign-outs (actor, time).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A provisioned user can go from the sign-in page to working inside the product in under 15 seconds on the first attempt, with no manual configuration (no dev headers, no seeded browser state).
- **SC-002**: 100% of invalid-credential attempts (wrong password, unknown email, deactivated account) are rejected, and 0 responses reveal whether the account exists.
- **SC-003**: 100% of protected API operations reject requests with missing, expired, tampered, or signed-out session tokens; 0 protected application routes render for unauthenticated visitors.
- **SC-004**: A signed-in session survives page reloads for its full 8-hour validity period; on expiry or sign-out, the user reaches the sign-in page cleanly with 0 raw error surfaces or stale authenticated views.
- **SC-005**: 100% of sign-in outcomes and sign-outs are traceable in the audit/security record (who, what, when).
- **SC-006**: The automated authentication test suite (FR-016) passes in CI on every change; a regression in any authentication rule fails the build.

## Assumptions

- **Users are provisioned, not self-registered**: Account creation, password reset, email verification, and MFA are out of scope for this feature. Users exist in the feature 005 `users` table; how they receive their initial password is an operational concern until a user-management feature ships. The existing auth screens from spec 003 (register, forgot/reset password, verify email) remain visual fixtures — only the login page becomes functional.
- **Credential storage is a schema addition**: The `users` table currently has no password material; adding it follows the migration workflow (feature 005, Constitution Principle VIII).
- **Token-based, mostly-stateless sessions**: Sessions are carried by a signed token presented on each request, valid for 8 hours (per clarification); a refresh-token flow is out of scope (an expired session simply requires signing in again). Sign-out performs server-side invalidation so a signed-out token cannot be replayed — the mechanism (e.g., a short-lived revocation record) is a planning decision.
- **Client persistence**: Per clarification, the session credential lives in an httpOnly secure cookie managed entirely by the server and browser — application code never stores or reads a token. Auth state persists across reloads because the cookie does; the client re-derives "who am I" from the current-user operation at startup. CSRF protection (FR-005a) is part of this feature's scope; the specific mechanism (e.g., SameSite attributes and/or a token pattern) is a planning decision.
- **Anti-abuse hardening is follow-up**: Failed attempts are audited and observable now; automated lockout, throttling, CAPTCHA, and IP-level controls are a later hardening feature.
- **Path naming**: The user description names `/auth/login`, `/auth/logout`, `/auth/me`. A current-user operation already exists (feature 006's `GET /me`); whether `/auth/me` aliases, replaces, or simply *is* that operation is reconciled at planning against the 001 REST contract — the behavioral requirement (FR-009) is the same either way.
- **Tenant context unchanged**: Feature 006's tenant isolation, switching, and `X-Tenant-ID` propagation are consumed as-is; this feature only upgrades the principal source underneath them, per 006's replacement guarantee (FR-019).
