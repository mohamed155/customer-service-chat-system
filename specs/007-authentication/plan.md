# Implementation Plan: Authentication

**Branch**: `007-authentication` | **Date**: 2026-07-09 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/007-authentication/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Ship real sign-in on top of 006's identity seam: Argon2id password verification against a new nullable `users.password_hash` column, an 8-hour HS256 JWT delivered exclusively as an httpOnly/Secure/SameSite=Lax cookie, per-request validation in `identity::principal_middleware` (cookie first in all environments; `X-Dev-User-Id` fallback stays dev/test-only per 006 FR-019), server-side revocation via a `revoked_sessions` table so logout kills the token, CSRF protection (SameSite + Origin-check middleware), and `auth.login_succeeded` / `auth.login_failed` / `auth.logged_out` audit rows. Two new endpoints (`POST /api/v1/auth/login`, `POST /api/v1/auth/logout`) live in the `identity` crate; `GET /me` is unchanged and satisfies the current-user requirement. The dashboard gains a functional login page (spec-003 visuals), a credentials interceptor (`withCredentials` — no client-held token exists), an `AuthService` façade, `authGuard`/`guestGuard` with returnUrl round-trip, and 401-mid-session handling that clears state and returns to sign-in. Locked in by an auth integration matrix (byte-identical invalid-credential 401s, cookie flags, revocation replay, CSRF) plus frontend Vitest specs.

## Technical Context

**Language/Version**: Backend Rust (workspace edition 2021) — Axum 0.8, SQLx 0.8, Tokio; Frontend Angular 22 (standalone, signals, zoneless, OnPush), TypeScript ~6.0, Taiga UI 5

**Primary Dependencies**: Backend: existing workspace crates (`kernel`, `config`, `db`, `observability`, `identity`, `tenancy`) plus **two new external crates**: `jsonwebtoken` (HS256 JWT, research R1) and `argon2` (Argon2id password hashing, research R2); `axum-extra` (cookie jar) as needed for cookie parsing. Frontend: existing `ApiService`, functional interceptors, `CurrentUserService` — no new packages.

**Storage**: PostgreSQL via migration workflow (feature 005): migration adds `users.password_hash TEXT NULL` and a `revoked_sessions` table (jti PK, user_id, expires_at). No Redis usage in the auth hot path (research R1).

**Testing**: Backend: `cargo test` — unit tests in `identity` (token issue/validate, env gating) + live-gated integration suite `crates/server/tests/auth.rs` (skip-if-unreachable `DATABASE_URL`, real in CI). Frontend: Vitest via `@angular/build:unit-test` for interceptor/service/guards/login-page specs.

**Target Platform**: Linux server (CI ubuntu-latest, Docker Compose dev); dashboard in evergreen browsers. Dev geometry: `localhost:4200` → `http://localhost:8080` is cross-origin but same-site, so Lax cookies + credentialed CORS work unchanged (research R3).

**Project Type**: Web application — Rust API backend + Angular dashboard frontend.

**Performance Goals**: Session validation adds zero extra I/O beyond one indexed query per request (user liveness + revocation NOT EXISTS in a single round trip — same cost class as 006's tenant check). Argon2 verification (~100ms by design) runs only on login, inside `spawn_blocking`, never on the per-request path.

**Constraints**: Session credential never readable by scripts (httpOnly; never in a response body); invalid-credential 401 bodies byte-identical across wrong-password/unknown-email/deactivated/no-credential cases (anti-enumeration, FR-003); CSRF protection on all state-changing requests (FR-005a); dev identity header hard-disabled outside dev/test, unchanged (006 FR-019); `AUTH_JWT_SECRET` only via environment, redacted in Debug (Constitution III); tenant-authorization logic untouched (006 replacement guarantee).

**Scale/Scope**: 2 new API endpoints, 1 middleware extension (principal) + 1 new middleware (CSRF origin check), 1 migration (2 objects), 2 config fields, 3 audit actions; frontend: 1 functional page, 1 service, 2 guards, 2 interceptor changes; auth integration matrix ≈ 14 scenarios.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Assessment |
|---|-----------|------------|
| I | Enterprise Modular Monolith | ✅ Credential verification, token lifecycle, and login/logout handlers live in the `identity` module crate (which already owns `Principal`); the server crate only mounts routes/middleware and extends CORS. No cross-module reach-through — `tenancy` consumes `Principal` exactly as before. |
| II | Multi-Tenant Isolation, No Exceptions | ✅ Untouched by design: this feature swaps the principal *source*; tenant context validation/authorization (006) runs unchanged downstream. Frontend guards remain UX-only — the server re-authenticates every request. |
| III | Zero-Trust Security & RBAC | ✅ All `/api/v1` routes keep requiring a principal (401 otherwise); login is the deliberate public exception per 001's endpoint table. Passwords stored only as Argon2id hashes; `AUTH_JWT_SECRET` env-only and redacted; sign-in outcomes and sign-outs audited (FR-004/FR-008); anti-enumeration 401s test-pinned. |
| V | API-First & Contract Consistency | ✅ `POST /auth/login` / `POST /auth/logout` match 001's endpoint table; errors use the kernel envelope; `GET /me` reused rather than aliased. One recorded divergence from 001 prose: dashboard session is a JWT-in-cookie rather than an "opaque session token" in an Authorization header — per spec clarification (httpOnly cookie) and feature description (JWT); contracts/http-api.md §A records it. |
| VI | Observability by Default | ✅ Existing request-id/trace middleware wraps the new endpoints; principal middleware keeps recording `principal.id` on the span; login/logout/denial timeline lives in `audit_logs`. |
| VII | Test-First & Regression Discipline | ✅ Auth matrix (FR-016) written against acceptance scenarios before/alongside implementation (006 discipline); frontend specs via Vitest; SC-006 makes the suite a CI gate. |
| VIII | Database Integrity & Migration Discipline | ✅ Schema changes only via a new migration (`password_hash` column + `revoked_sessions` table with PK/index); no manual schema edits; no denormalization. |
| X | Performance & Efficiency | ✅ One indexed query per authenticated request; expensive hashing confined to login in `spawn_blocking`; no caching that would violate FR-009-style freshness (revocation and user liveness checked per request). |

**Gate result**: PASS — no violations requiring Complexity Tracking. (Principles IV and IX are not exercised: no AI surface; UI reuses existing spec-003 login visuals and shared components.)

**Post-Phase-1 re-check**: PASS — design keeps enforcement server-side in module crates, adds exactly one migration, no new state mechanisms on the frontend, and records the single 001-prose divergence in the contract doc.

## Project Structure

### Documentation (this feature)

```text
specs/007-authentication/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/
│   └── http-api.md      # Endpoint, cookie, CSRF, and error-semantics contract
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/0007_auth.sql              # NEW: users.password_hash + revoked_sessions
├── crates/shared/config/src/lib.rs       # EDIT: auth_jwt_secret (redacted) + auth_session_ttl_seconds
├── crates/modules/identity/
│   ├── Cargo.toml                        # EDIT: + jsonwebtoken, argon2, axum-extra(cookie), rand
│   └── src/
│       ├── lib.rs                        # EDIT: principal_middleware gains cookie-first resolution
│       │                                 #   (all envs) + SessionClaims extension; dev-header
│       │                                 #   fallback unchanged (dev/test only)
│       ├── session.rs                    # NEW: JWT issue/validate, cookie build/clear (R1/R3)
│       ├── password.rs                   # NEW: Argon2id hash/verify + dummy-verify (R2)
│       ├── audit.rs                      # NEW: auth.login_succeeded/login_failed/logged_out (R8)
│       └── routes.rs                     # NEW: POST /auth/login, POST /auth/logout (R6)
├── crates/server/src/
│   ├── router.rs                         # EDIT: mount auth routes; csrf_origin_middleware on
│   │                                     #   /api/v1; cors allow_credentials(true)
│   └── state.rs / main wiring            # EDIT: pass auth config into IdentityConfig
└── crates/server/tests/auth.rs           # NEW: auth integration matrix (live-gated, R10)

frontend/apps/dashboard/src/app/
├── core/
│   ├── http/auth-token.interceptor.ts    # EDIT: no-op → withCredentials for apiBaseUrl (R9)
│   ├── http/api-error.interceptor.ts     # EDIT: 401 mid-session → clear auth state + go to login
│   ├── auth/auth.service.ts              # NEW: login()/logout() façade over ApiService
│   ├── router/auth.guard.ts              # NEW: authGuard (canMatch) + returnUrl
│   ├── router/guest.guard.ts             # NEW: guestGuard for the auth area
│   └── tenant/current-user.service.ts    # EDIT: bootstrap load resolves 401 as signed-out
├── features/auth/login/login.component.ts  # EDIT: fixture form → functional (pending/error/returnUrl)
├── app.routes.ts                         # EDIT: authGuard on shell, guestGuard on auth area
└── app.config.ts                         # EDIT: initializer tolerant of 401
```

**Structure Decision**: Backend logic stays in the `identity` module crate that already owns principal resolution (modular-monolith boundary from spec 004); the server crate only mounts and layers. `tenancy` is untouched. Frontend additions follow spec-002 layering (`core/` singletons, `features/auth` page) and repurpose the two interceptor seams that were explicitly reserved for this feature.

## Complexity Tracking

> No constitution violations. Two additive external crates (`jsonwebtoken`, `argon2`) extend the mandated stack rather than deviating from it — required because the stack list has no password-hashing or token-signing primitive; recorded here for visibility. The 001-prose divergence (JWT-in-cookie vs opaque bearer session) is a spec-driven clarification, documented in Constitution Check row V and contracts/http-api.md §A.
