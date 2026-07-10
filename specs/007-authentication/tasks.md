# Tasks: Authentication

**Input**: Design documents from `/specs/007-authentication/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/http-api.md, quickstart.md

**Tests**: INCLUDED — constitution Principle VII (Test-First) applies; spec FR-016 mandates the auth matrix. Backend integration tests are live-gated on `DATABASE_URL` (skip with notice, run for real in CI); frontend specs run via Vitest.

**Organization**: Tasks are grouped by user story. The session-token core (issue/validate/cookie, password hashing, cookie-first principal resolution) is **Foundational** — US1's independent test ("subsequent requests are authenticated") already exercises validation, so it cannot belong to any single story. US1 = login endpoint + login UI; US2 = enforcement hardening (validation matrix, CSRF, guards, 401 continuity); US3 = logout + revocation write-path (the revocation *check* ships in Foundational so no rework).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)

## Path Conventions

Backend Cargo workspace: module crates in `backend/crates/modules/`, shared crates in `backend/crates/shared/`, server in `backend/crates/server/`, migrations in `backend/migrations/`. Frontend: `frontend/apps/dashboard/src/app/` with spec-002 layering (`core/`, `features/`, `layout/`).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Schema, config, dependencies, and test harness every story needs

- [X] T001 [P] Create `backend/migrations/0007_auth.sql` per data-model.md: `ALTER TABLE users ADD COLUMN password_hash TEXT NULL` with `CHECK (password_hash IS NULL OR password_hash LIKE '$argon2%')`, and `CREATE TABLE revoked_sessions (jti UUID PRIMARY KEY, user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT, expires_at TIMESTAMPTZ NOT NULL, revoked_at TIMESTAMPTZ NOT NULL DEFAULT now())`; apply with `sqlx migrate run` against local Postgres
- [X] T002 [P] Extend `backend/crates/shared/config/src/lib.rs`: add `auth_jwt_secret: String` (required, min 32 bytes, `[REDACTED]` in Debug) and `auth_session_ttl_seconds: u64` (env `AUTH_SESSION_TTL_SECONDS`, default 28800) with parser validation + unit tests; set `AUTH_JWT_SECRET` in `docker-compose` env / `.env` sample and in `.github/workflows/backend.yml` job env so CI and dev boot
- [X] T003 [P] Add workspace deps to `backend/Cargo.toml` (`jsonwebtoken`, `argon2`, `axum-extra` with `cookie` feature, `rand` as needed) and wire them into `backend/crates/modules/identity/Cargo.toml`; keep the crate compiling (`cargo check -p identity`)
- [X] T004 [P] Create integration-test scaffolding in `backend/crates/server/tests/auth.rs`: reuse the 006 pattern from `crates/server/tests/tenancy.rs` (live-gated pool helper, `tower::ServiceExt::oneshot` harness against the router, unique-per-test seeds) extended with a `seed_user_with_password(email, platform_role, password)` helper that stores a real Argon2id hash; add `argon2` to `backend/crates/server/Cargo.toml` `[dev-dependencies]`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Token lifecycle, password verification, and cookie-first principal resolution — the mechanics every story authenticates through

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T005 Implement `backend/crates/modules/identity/src/session.rs` per research R1/R3 and data-model §3: `SessionClaims { sub, jti, iat, exp }`, `issue_token(secret, ttl, user_id) -> (jwt, jti, expires_at)`, `validate_token(secret, jwt) -> Result<SessionClaims>` (HS256, expiry enforced), `build_session_cookie(jwt, ttl)` / `clear_session_cookie()` producing `app_session=…; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=…`; unit tests: issue/validate round-trip, expired rejected (ttl 0), tampered payload rejected, wrong-secret rejected, cookie attribute strings exact
- [X] T006 [P] Implement `backend/crates/modules/identity/src/password.rs` per research R2: `hash_password`, `verify_password` (Argon2id, called inside `tokio::task::spawn_blocking` by callers), and `verify_dummy()` running verification against a fixed PHC hash for the unknown-email path; unit tests: round-trip, wrong password false, dummy path returns false without panicking
- [X] T007 Extend `principal_middleware` in `backend/crates/modules/identity/src/lib.rs` per research R5 / contracts §D: `IdentityConfig` gains `auth_jwt_secret` + `auth_session_ttl_seconds`; in ALL environments first try the `app_session` cookie — `validate_token`, then ONE query (user `deleted_at IS NULL` AND `NOT EXISTS (SELECT 1 FROM revoked_sessions WHERE jti = $2)`), attach `Principal` + `SessionClaims` extension, record `principal.id` on the span; existing `X-Dev-User-Id` path runs only when no valid cookie principal AND environment is Development/Test; update `backend/crates/server/src/router.rs` to pass the new `IdentityConfig` fields; keep `cargo test -p server --test tenancy` green

**Checkpoint**: A session cookie (once minted) authenticates requests; dev header still works in dev/test — user story implementation can begin

---

## Phase 3: User Story 1 - Credential Sign-In (Priority: P1) 🎯 MVP

**Goal**: `POST /auth/login` verifies Argon2id credentials, issues the httpOnly session cookie, returns `MeResponse`, and audits every outcome; invalid credentials are byte-identically rejected (anti-enumeration). Users can sign in from the dashboard login page.

**Independent Test**: With a seeded user whose password is known: login succeeds (200 + cookie with pinned flags, cookie authenticates `GET /me`); wrong password, unknown email, soft-deleted user, and NULL-hash user all return byte-identical 401 bodies; audit rows recorded — fully automatable via the API (quickstart §2).

### Tests for User Story 1 ⚠️ write first — must FAIL before T010–T011 exist

- [X] T008 [US1] Write login integration tests in `backend/crates/server/tests/auth.rs` per contracts §B: valid credentials → 200 `MeResponse` (platform role + active memberships) + `Set-Cookie` asserting `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`, `Max-Age=28800`; returned cookie authenticates `GET /api/v1/me` → 200; wrong password / unknown email / soft-deleted user / `password_hash IS NULL` → 401 `unauthenticated` with **byte-identical bodies** (modulo request_id); missing or blank email/password and non-JSON body → 400 `validation_failed`; supplied `X-Tenant-ID` ignored; login while already holding a valid cookie → 200 + fresh cookie; audit: one `auth.login_succeeded` row (actor = user) per success, one `auth.login_failed` row (actor NULL, details `{email, reason}`) per failure

### Implementation for User Story 1

- [X] T009 [P] [US1] Implement the audit helper in `backend/crates/modules/identity/src/audit.rs` per data-model §5: fail-open `record(pool, action, actor, resource_id, details)` insert into `audit_logs` (`tracing::error!` and continue on failure, mirroring `tenancy::audit`), with `login_succeeded(user_id)`, `login_failed(email, reason)`, `logged_out(user_id, jti)` wrappers
- [X] T010 [US1] Implement the login handler in `backend/crates/modules/identity/src/routes.rs` per contracts §B: parse `{email, password}` via kernel `ApiJson` (400 on validation failure), fetch user by email (`deleted_at IS NULL`), `spawn_blocking` verify — or `verify_dummy` when no user/no hash — with ONE shared 401 constructor for every failure path (anti-enumeration, keep it a single fn so bodies can never drift), on success `issue_token` + `Set-Cookie` via `build_session_cookie` + `MeResponse` body (memberships query as in `tenancy::routes::me`), audit both outcomes (depends on T005, T006, T009)
- [X] T011 [US1] Mount `POST /api/v1/auth/login` in `backend/crates/server/src/router.rs` (both `app` and `app_with_test_routes`; inside the `/api/v1` nest, requires no principal) and add `.allow_credentials(true)` to `cors_layer` per research R3; verify `cargo test -p server --test auth` login scenarios green (T008), tenancy suite still green
- [X] T012 [P] [US1] Create `frontend/apps/dashboard/src/app/core/auth/auth.service.ts` (+ spec): `login(email, password)` → `ApiService.post('/auth/login', …)` then `CurrentUserService.load()`; maps failure to the generic invalid-credentials message via `http-error.mapper`; expose pending state signal
- [X] T013 [US1] Make the login page functional in `frontend/apps/dashboard/src/app/features/auth/login/login.component.ts` (+ spec): reactive form (email/password, required validators), submit → `AuthService.login`, pending state disables resubmission, generic error message on 401, on success navigate to validated internal `returnUrl` query param or the default route; spec-003 visuals unchanged (depends on T012)

**Checkpoint**: Real sign-in works end-to-end (API + dashboard form) — the MVP guarantee holds

---

## Phase 4: User Story 2 - Protected Access & Session Continuity (Priority: P2)

**Goal**: Every protected surface rejects missing/expired/tampered sessions and CSRF attempts; the dashboard redirects signed-out visitors (preserving destination), survives reloads, and cleanly returns to sign-in when the session dies mid-use.

**Independent Test**: API: expired, tampered, and garbage cookies plus deleted-user tokens all → 401; production env ignores the dev header while cookies still work; cross-origin POST → 403. App: protected route while signed out redirects to login and returns after sign-in; reload keeps the session; deleting the cookie then acting returns to login with a notice (quickstart §5).

### Tests for User Story 2 ⚠️ write first — must FAIL before T015 exists

- [X] T014 [US2] Add validation-matrix integration tests to `backend/crates/server/tests/auth.rs` per contracts §D/§E: expired token (mint via `session::issue_token` with ttl 0) → 401; tampered signature → 401; garbage cookie value → 401; valid token whose user was soft-deleted after issuance → 401; harness with `Environment::Production` config: `X-Dev-User-Id` ignored (401) while a valid cookie authenticates (200) — 006 FR-019 preserved; CSRF: `POST /api/v1/auth/logout` (or any state-changing route) with valid cookie + `Origin: https://evil.example` → 403 `unauthorized`, allowed origin → not blocked, `GET` with foreign Origin → not blocked

### Implementation for User Story 2

- [X] T015 [US2] Implement `csrf_origin_middleware` in `backend/crates/server/src/router.rs` (or a small `server/src/csrf.rs`) per research R4: for non-GET/HEAD/OPTIONS requests under `/api/v1`, when an `Origin` header is present and not in `config.cors_allowed_origins` → 403 `unauthorized` before any handler; layer it in both `app` and `app_with_test_routes`; verify T014 green
- [X] T016 [P] [US2] Rewrite `frontend/apps/dashboard/src/app/core/http/auth-token.interceptor.ts` (+ spec) into the credentials interceptor per contracts §F: set `withCredentials: true` on requests targeting `apiBaseUrl`; leave non-API requests untouched
- [X] T017 [P] [US2] Create `frontend/apps/dashboard/src/app/core/router/auth.guard.ts` and `guest.guard.ts` (+ specs): `authGuard` (canMatch) — `CurrentUserService.currentUser()` null → `router.parseUrl('/auth/login?returnUrl=<attempted URL>')`; `guestGuard` (canMatch on the auth area) — authenticated → redirect to the app root
- [X] T018 [US2] Wire session continuity in `frontend/apps/dashboard/src/app/app.routes.ts`, `core/tenant/current-user.service.ts`, and `app.config.ts` (+ specs): `authGuard` on the shell route (`''` with `AppShellComponent` — login lives outside it, so no canMatch loop), `guestGuard` on the auth area; `CurrentUserService.load()` resolves a 401 as signed-out (`null`) instead of rejecting so the `provideAppInitializer` bootstrap never crashes (depends on T016, T017)
- [X] T019 [US2] Handle 401-mid-session in `frontend/apps/dashboard/src/app/core/http/api-error.interceptor.ts` (+ spec) per FR-014 / contracts §F: on 401 `unauthenticated` from an `apiBaseUrl` response (excluding the `/auth/login` call itself), clear `CurrentUserService` + tenant context, surface a session-expired notice via `ApiErrorNotificationService`, and navigate to the login page

**Checkpoint**: Enforcement is airtight server-side and mistake-proof client-side; sessions survive reloads and die cleanly

---

## Phase 5: User Story 3 - Sign-Out (Priority: P3)

**Goal**: Explicit, audited sign-out that revokes the session server-side — a replayed cookie can never authenticate again — and returns the user to the login page with all client state cleared.

**Independent Test**: API: login, capture cookie, logout (204 + clearing Set-Cookie + `revoked_sessions` row + `auth.logged_out` audit row), replay captured cookie on `GET /me` → 401. App: sign out from the shell → login page; protected routes redirect; second tab's next action lands on login (quickstart §2/§5).

### Tests for User Story 3 ⚠️ write first — must FAIL before T021 exists

- [X] T020 [US3] Add logout integration tests to `backend/crates/server/tests/auth.rs` per contracts §C: valid session cookie → 204 with clearing `Set-Cookie` (`Max-Age=0`), exactly one `revoked_sessions` row (that `jti`, correct `user_id`/`expires_at`) and one `auth.logged_out` audit row with `{jti}` details; replaying the pre-logout cookie on `GET /api/v1/me` → 401; no principal → 401 `unauthenticated`; dev-header principal (no cookie) → 204 with clearing cookie and NO revocation row

### Implementation for User Story 3

- [X] T021 [US3] Implement the logout handler in `backend/crates/modules/identity/src/routes.rs` per contracts §C: requires `Principal`; when a `SessionClaims` extension is present insert its `jti` into `revoked_sessions` and write `auth.logged_out` audit; respond 204 + `clear_session_cookie()`; mount `POST /api/v1/auth/logout` in `backend/crates/server/src/router.rs` (both router fns, inside principal middleware); verify T020 green
- [X] T022 [US3] Frontend sign-out per contracts §F: add `logout()` to `frontend/apps/dashboard/src/app/core/auth/auth.service.ts` (POST `/auth/logout`, then clear `CurrentUserService`, dispatch tenant-context clear, navigate to login) and add a sign-out control to `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts` (Taiga-wrapped button/menu item, visible for any authenticated user) (+ specs)

**Checkpoint**: Full session lifecycle — issue, validate, expire, revoke — closed and regression-locked

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: End-to-end validation and documentation alignment

- [ ] T023 Execute `specs/007-authentication/quickstart.md` end-to-end (curl matrix incl. byte-identical 401 check and audit rows, §3 production-gating check, §5 browser walkthrough) and fix any doc/behavior drift — requires running Postgres + backend + dashboard
- [X] T024 [P] Verify the `007-authentication` entry in `CLAUDE.md` Recent Changes (added during planning) still matches what shipped; correct if implementation diverged
- [X] T025 Run all quality gates: backend `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (Postgres up); frontend `pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check` — all green

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none — start immediately
- **Foundational (Phase 2)**: T005/T006 need T002 (config) + T003 (deps); T007 needs T001 (revoked_sessions exists), T005
- **US1 (Phase 3)**: needs Phase 2 complete + T004 (harness)
- **US2 (Phase 4)**: needs Phase 2; T014's revoked-user scenarios reuse US1's login only for convenience — tokens can be minted directly via `session::issue_token`, so US2 is independently implementable after Phase 2
- **US3 (Phase 5)**: needs Phase 2 (SessionClaims extension from T007) + T009 (audit helper); frontend T022 builds on T012 (AuthService)
- **Polish (Phase 6)**: after US1–US3

### Task-level Dependencies

- T001 → T007 (table), T002 → T005/T007 (config), T003 → T005/T006 (crates), T004 → T008/T014/T020 (harness)
- T005 + T006 → T007 → all endpoint work
- T008 → T009/T010 → T011 (tests fail first, then green)
- T012 → T013; T012 → T022
- T014 → T015; T016 + T017 → T018 → T019
- T020 → T021 → T022

### Parallel Opportunities

- T001 ∥ T002 ∥ T003 ∥ T004 (four independent files)
- T005 ∥ T006 (session vs password modules)
- T009 ∥ T012 (backend audit vs frontend service)
- T016 ∥ T017 (interceptor vs guards)
- T024 ∥ T023/T025

---

## Parallel Example: User Story 2

```bash
# After Phase 2, backend and frontend halves proceed together:
Task: "T014 validation-matrix + CSRF tests in crates/server/tests/auth.rs"   # backend
Task: "T016 credentials interceptor in core/http/auth-token.interceptor.ts"  # frontend
Task: "T017 authGuard + guestGuard in core/router/"                          # frontend
# Then: T015 (CSRF middleware) ∥ T018 (route wiring) → T019 (401 continuity)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 (T001–T004) + Phase 2 (T005–T007)
2. Phase 3 US1 (T008–T013): real credential sign-in, cookie-issued, audited, anti-enumeration pinned
3. **STOP and VALIDATE**: `cargo test -p server --test auth` + quickstart §2 login curls — users can genuinely sign in before any enforcement UX exists

### Incremental Delivery

1. US1 → sign-in (MVP — replaces the dev-header as the way in)
2. US2 → enforcement + continuity (validation matrix, CSRF, guards, 401 handling)
3. US3 → audited revocable sign-out
4. Polish → quickstart end-to-end + both quality-gate suites

### Notes

- Anti-enumeration is a *test-pinned byte-equality* (006 discipline): one shared 401 constructor for wrong-password/unknown-email/deleted/no-hash — they can never drift apart
- Expiry tests mint tokens directly with ttl 0 via `session::issue_token` — no sleeping, no clock mocking
- Integration tests seed unique users per test (uuid-suffixed emails) against a shared dev DB — no TRUNCATE
- The dev identity header keeps all 006 tenancy tests and local tooling working unchanged; production gating is itself a pinned test (T014)
- Commit after each task or logical group; backend and frontend halves of US1/US2 can land as separate commits







