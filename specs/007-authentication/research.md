# Research: Authentication

Decisions resolving every open point in the Technical Context. Each entry: Decision / Rationale / Alternatives considered.

## R1. Session token format: JWT in an httpOnly cookie, revocation in Postgres

- **Decision**: Sign a compact JWT (HS256, `jsonwebtoken` crate) with claims `{ sub: user_id, jti: uuid, iat, exp = iat + 8h }`, delivered only as an httpOnly cookie (never in a response body). Logout inserts the token's `jti` into a `revoked_sessions` table (jti PK, `expires_at` for later cleanup). Per-request validation = signature + expiry check (pure CPU, no I/O) followed by **one** indexed query that loads the user (`deleted_at IS NULL`) and checks `NOT EXISTS revoked_sessions(jti)` in the same round trip — satisfying FR-006 (user liveness) and FR-008 (revocation) at the same cost class as 006's tenant-authorization query.
- **Rationale**: The feature description explicitly scopes "JWT generation, JWT validation". FR-006 forces a per-request user query anyway, so the revocation check rides along for free; Postgres (not Redis) makes revocation durable and keeps tests live-gated on `DATABASE_URL` only, like every suite since 004. The 8h TTL is spec-pinned (clarification).
- **Alternatives considered**: Opaque random session id in a `sessions` table (001's prose says "opaque session token" — simpler crypto, but drops the explicitly requested JWT mechanics; divergence recorded in contracts/http-api.md §A either way). Redis `jti` denylist (adds a second stateful dependency to the auth hot path; `cache::Cache` currently exposes only `ping`).

## R2. Password hashing: Argon2id via the `argon2` crate

- **Decision**: Hash with Argon2id (crate defaults, OWASP-aligned) producing PHC-format strings into a new nullable `users.password_hash TEXT` column. Verification runs inside `tokio::task::spawn_blocking` (hashing is deliberately ~10⁴× slower than a query). A user with `password_hash IS NULL` fails login exactly like a wrong password. On unknown email, verify against a fixed dummy hash so the work performed is the same on every failure path (best-effort timing uniformity backing FR-003's indistinguishability).
- **Rationale**: Argon2id is the current OWASP first choice; PHC strings self-describe parameters so future tuning needs no migration. Nullable column means existing seeded users are unaffected until given credentials.
- **Alternatives considered**: bcrypt (fine, but weaker memory-hardness); scrypt (less ecosystem momentum in Rust). Both rejected in favor of the OWASP default.

## R3. Cookie posture and dev-mode geometry

- **Decision**: Cookie `app_session=<jwt>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800`. Logout responds with the same cookie name, empty value, `Max-Age=0`. Backend CORS gains `.allow_credentials(true)` (origin list is already explicit, which credentialed CORS requires); the frontend sends `withCredentials: true` on all `apiBaseUrl` requests.
- **Rationale**: Dev runs the dashboard at `localhost:4200` against `http://localhost:8080` — cross-**origin** but same-**site** (SameSite ignores port), so `Lax` cookies flow in dev unchanged; browsers accept `Secure` cookies on `localhost` as a trustworthy origin. Production (`apiBaseUrl: '/api/v1'`) is same-origin and trivially fine.
- **Alternatives considered**: `SameSite=None` (needless — nothing is cross-site) ; Angular dev proxy to fake same-origin (extra moving part; not needed given same-site semantics); `SameSite=Strict` (breaks the return-to-app flow after following an emailed link in future features; Lax + FR-005a protections suffice).

## R4. CSRF protection (FR-005a): SameSite=Lax + Origin-check middleware

- **Decision**: Two independent layers. (1) `SameSite=Lax` stops the browser attaching the session cookie to cross-site subresource/form requests. (2) A `csrf_origin_middleware` on `/api/v1` rejects any state-changing request (non-GET/HEAD/OPTIONS) whose `Origin` header is present but not in `cors_allowed_origins`, with 403 `unauthorized` — browsers always send `Origin` on cross-origin POSTs, so a forged request cannot omit it while a same-origin or non-browser client is unaffected. JSON-only request bodies (kernel `ApiJson`) additionally force a CORS preflight on any cross-origin scripted attempt.
- **Rationale**: Defense-in-depth without token-synchronization machinery the SPA would have to thread through every request; both layers are integration-testable (FR-016 pins them).
- **Alternatives considered**: Double-submit CSRF token (requires a JS-readable cookie — reintroduces the surface the httpOnly decision removed, and complicates every frontend mutation); synchronizer tokens (server session state we deliberately don't have).

## R5. Principal resolution order (replacing 006's dev-only source)

- **Decision**: Extend `identity::principal_middleware`: in **all** environments, first try the `app_session` cookie — validate JWT, run the liveness+revocation query, attach `Principal`. Only when no valid cookie principal was resolved **and** the environment is Development/Test does the existing `X-Dev-User-Id` path run, unchanged. Production/Staging: cookie is the only source (006 FR-019's hard-disable preserved verbatim).
- **Rationale**: Exactly fulfills the 006 replacement guarantee — `Principal`, extractors, tenant middleware, and every downstream consumer are untouched; the dev header keeps existing tenancy tests and local tooling working without real credentials.
- **Alternatives considered**: Removing the dev header entirely (would force every 006 integration test to mint real sessions — churn with no isolation benefit; spec explicitly keeps it dev/test-only).

## R6. Endpoint placement and the `/auth/me` question

- **Decision**: `POST /api/v1/auth/login` (public — mounted inside the principal middleware but requiring no principal) and `POST /api/v1/auth/logout` (requires principal; also needs the presented `jti`, so the middleware records the validated session's `jti` alongside `Principal` as a `SessionClaims` extension). Handlers live in the `identity` crate (`routes.rs`) — it owns principal production; `tenancy` continues to own `/me`, `/tenant`, `/platform/*`. **No `/auth/me` route is added**: the existing `GET /api/v1/me` already satisfies FR-009 and the 001 contract; the user description's `/auth/me` is recorded as satisfied-by-`/me` in contracts/http-api.md §A.
- **Rationale**: Matches 001's endpoint table (`POST /auth/login · /auth/logout` public/any; `GET /me`); keeps module boundaries clean (identity = who you are; tenancy = where you may act).
- **Alternatives considered**: A new `auth` module crate (nothing left over for `identity` to own — needless split); aliasing `/auth/me` → same handler (two paths for one resource violates 001's consistency principle).

## R7. Configuration additions

- **Decision**: `AppConfig` gains `auth_jwt_secret: String` (required, min 32 bytes, `[REDACTED]` in Debug like the URLs) and `auth_session_ttl_seconds: u64` (default 28800). Docker-compose/dev `.env` and CI workflows set `AUTH_JWT_SECRET`; integration tests construct `AppConfig` directly as today.
- **Rationale**: Constitution III — secrets only via environment; TTL as config keeps the 8h spec value declarative and test-overridable (tests can mint short-lived tokens without waiting).
- **Alternatives considered**: Hardcoded TTL (untestable expiry path); optional secret with dev default (a baked-in default secret is a footgun that ships to prod).

## R8. Audit vocabulary (FR-004, FR-008)

- **Decision**: Three actions in 005's `audit_logs` via an identity-crate helper mirroring `tenancy::audit`'s insert (fail-open, `tracing::error!` on insert failure): `auth.login_succeeded` (actor = user, `tenant_id` NULL), `auth.login_failed` (actor NULL, details `{ email, reason }`), `auth.logged_out` (actor = user, details `{ jti }`).
- **Rationale**: Same vocabulary style as 006 (`tenant.access_denied`, `platform.tenant_switched`); failed-attempt rows make probing observable (spec edge case) without lockout machinery. Consolidating the two audit helpers into the placeholder `audit` module crate is deferred — noted, not blocking.
- **Alternatives considered**: Depending on `tenancy::audit` from `identity` (inverts the 006 dependency direction — tenancy depends on identity); implementing the `audit` module crate now (worthwhile refactor, out of this feature's scope).

## R9. Frontend auth architecture

- **Decision**:
  - **Credentials**: repurpose the no-op `authTokenInterceptor` (its documented purpose) into a credentials interceptor — set `withCredentials: true` for `apiBaseUrl` requests. No token is ever stored or read by application code (spec clarification).
  - **State**: `CurrentUserService` remains the single source of truth for "who am I" (FR-011); a new `AuthService` façade exposes `login(email, password)` (POST, then `CurrentUserService.load()`) and `logout()` (POST, clear user + tenant context, navigate to login).
  - **Guards**: new `authGuard` (`canMatch`) on the shell route — unauthenticated → `router.parseUrl('/auth/login?returnUrl=<attempted>')`; new `guestGuard` on the auth area — authenticated → into the app. `areaAccessGuard` is untouched (it runs after `authGuard` within the shell).
  - **Bootstrap**: the existing `provideAppInitializer` current-user load treats a 401 as "signed out" (resolve, don't crash); guards then route accordingly.
  - **401 mid-session** (FR-014): extend `apiErrorInterceptor` — on 401 `unauthenticated` from `apiBaseUrl` (excluding the login request itself), clear auth + tenant state and navigate to the login page with a session-expired message via `ApiErrorNotificationService`.
  - **Login page**: make the existing spec-003 login fixture functional (reactive form, pending state, generic error display, returnUrl handling) — visuals unchanged, no other auth screens touched.
- **Rationale**: Every piece lands on a seam that already exists (interceptor placeholder, notification service, guard pattern, app initializer) — spec-002 layering intact, no new state mechanism.
- **Alternatives considered**: NgRx slice for auth state (CurrentUserService signals already hold it; duplicating into Store violates the "never duplicate state across mechanisms" rule); functional login via NgRx effects (over-engineering a two-call façade).

## R10. Test strategy (FR-016, SC-006)

- **Decision**: Backend: unit tests in `identity` for pure parts (JWT issue/validate round-trip, expiry rejection, tamper rejection, cookie attribute construction, env-gating matrix) using a test secret; live-gated integration suite `backend/crates/server/tests/auth.rs` (006 harness pattern — oneshot router, unique-per-test seeds now including Argon2 hashes) covering: login success (200 MeResponse + Set-Cookie flags asserted), wrong password / unknown email / deleted user / NULL hash → **byte-identical** 401 bodies (modulo request_id), cookie authenticates `/me`, tampered/expired/garbage cookie → 401, logout → cleared cookie + replay of old cookie → 401, revoked-session query row present, user-deleted-after-issuance → 401, CSRF origin-check (foreign `Origin` on POST → 403), dev-header fallback still works in test env, audit rows for all three actions. Frontend: Vitest specs for credentials interceptor, `AuthService`, `authGuard`/`guestGuard` (returnUrl round-trip), login component states, and 401-handling in `apiErrorInterceptor`.
- **Rationale**: Byte-identical 401 pinning mirrors 006's anti-enumeration discipline; expiry is testable without sleeping because token issuance takes an injectable TTL (R7).
- **Alternatives considered**: E2E browser tests for cookie flow (valuable later; the integration suite asserts Set-Cookie attributes directly, which is what the browser enforces).

## R11. What this feature does NOT do (spec-pinned boundaries)

- No registration, password reset, email verification, MFA, refresh tokens, session listing (`GET /auth/sessions` from 001 is a later feature), lockout/throttling, or `users` module CRUD. Initial passwords for dev/demo arrive via seed SQL in the quickstart (hash generated by a one-liner documented there) — the user-management feature owns real provisioning.
