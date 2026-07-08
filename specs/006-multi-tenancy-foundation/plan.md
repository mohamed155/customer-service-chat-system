# Implementation Plan: Multi-Tenancy Foundation

**Branch**: `master` (feature dir `006-multi-tenancy-foundation`) | **Date**: 2026-07-08 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/006-multi-tenancy-foundation/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Deliver runtime tenant isolation on top of feature 005's tables: an identity layer that resolves an authenticated principal (dev/test-only `X-Dev-User-Id` header until real auth ships), a tenant-context middleware that validates `X-Tenant-ID` and authorizes the principal against `tenant_memberships` (tenant users) or `users.platform_role` (platform users), stateless per-request context propagation, an explicit audited switch action for platform users, and the dashboard's first real API integration — a tenant context NgRx slice, an `X-Tenant-ID` interceptor, and a platform-user-only tenant switcher in the topbar. Isolation is enforced in the `tenancy`/`identity` module crates (modular monolith), surfaced through four `/api/v1` endpoints aligned with the 001 REST contract, and locked in by an automated isolation-matrix integration test suite.

## Technical Context

**Language/Version**: Backend Rust (workspace edition 2021) — Axum 0.8, SQLx 0.8, Tokio; Frontend Angular 22 (standalone, signals, zoneless, OnPush), TypeScript ~6.0, NgRx 21, Taiga UI 5

**Primary Dependencies**: Backend: existing workspace crates only (`kernel` error envelope, `db` pool, `config` environments, `observability` request-id/trace) — no new external crates expected. Frontend: existing `ApiService`, functional interceptors, NgRx Store, Taiga-wrapped components.

**Storage**: PostgreSQL via feature 005 schema — `users`, `tenants`, `tenant_memberships`, `audit_logs`. **No new tables or migrations**; authorization reads use existing partial-unique/PK indexes.

**Testing**: Backend: `cargo test` — unit tests in `tenancy`/`identity` crates + live-gated integration tests in `crates/server/tests/` (skip-if-unreachable `DATABASE_URL` pattern); the isolation matrix (FR-018) runs for real in CI's Postgres service. Frontend: Vitest via `@angular/build:unit-test` for service/interceptor/switcher/store specs.

**Target Platform**: Linux server (CI ubuntu-latest, Docker Compose dev); dashboard in evergreen browsers.

**Project Type**: Web application — Rust API backend + Angular dashboard frontend.

**Performance Goals**: Tenant authorization adds at most one indexed query per request (membership check by `(tenant_id, user_id)` partial-unique index or tenant PK lookup); no N+1, no cross-request caching of allow decisions (FR-009 correctness beats caching here).

**Constraints**: Stateless tenant context (header-only, no server session state); anti-enumeration (nonexistent tenant ≡ unauthorized, both 403 `unauthorized`); dev identity header hard-disabled outside development/test (FR-019); client-side checks never sole enforcement (Constitution II).

**Scale/Scope**: 4 API endpoints, 2 middleware layers, ~2 module crates gain real code (from placeholders), 1 NgRx feature slice, 2 interceptors, 1 switcher component; isolation matrix ≈ 10 integration test scenarios.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Assessment |
|---|-----------|------------|
| I | Enterprise Modular Monolith | ✅ Tenant-context logic lives in the `tenancy` module crate; principal resolution in `identity`. The server crate only wires middleware into the router. Modules expose typed interfaces (`Principal`, `TenantContext`, extractors) — no cross-module data reach-through. |
| II | Multi-Tenant Isolation, No Exceptions | ✅ This feature implements Principle II's runtime half: every tenant-scoped request authorized in the pipeline and honored by handlers' data access; platform vs tenant user distinction per constitution role sets; frontend checks are UX only (switcher visibility) — the server always re-authorizes. |
| III | Zero-Trust Security & RBAC | ✅ `/api/v1` endpoints require an authenticated principal (401 otherwise); tenant switches and forbidden attempts audited (FR-012/FR-013). Coarse platform-vs-tenant authorization here; per-role grants (e.g. 001's `P:sales+` on the tenant directory) are sequenced to the RBAC feature — documented in contracts/http-api.md, not a violation. Dev identity header is env-gated, never a production path. |
| V | API-First & Contract Consistency | ✅ Endpoints align with 001's `contracts/rest-api.md` (`GET /me`, `GET /platform/tenants`, `POST /platform/tenants/{id}/switch`, `GET /tenant`); errors use the kernel envelope; 403 semantics follow the contract's anti-probing rule. One divergence from 001 prose: switch context is header-carried (stateless) rather than session-carried — per spec clarification; the contract doc records this. |
| VI | Observability by Default | ✅ Existing request-id/trace middleware wraps everything; tenant id and principal id are recorded on the request's trace span; audit records give the who/what/when timeline. |
| VII | Test-First & Regression Discipline | ✅ Isolation matrix (FR-018) written as integration tests against the spec's acceptance scenarios before/alongside the middleware; frontend interceptor/switcher/store specs via Vitest. |
| X | Performance & Efficiency | ✅ One indexed authorization query per request; no allocation-heavy layers; no caching that would violate FR-009. |

**Gate result**: PASS — no violations requiring Complexity Tracking.

**Post-Phase-1 re-check**: PASS — the design keeps all enforcement server-side in module crates, adds no tables, no denormalization, and no unaudited sensitive operations; contracts match the kernel envelope and 001 conventions.

## Project Structure

### Documentation (this feature)

```text
specs/006-multi-tenancy-foundation/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/
│   └── http-api.md      # Endpoint, header, and error-semantics contract
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── crates/modules/identity/
│   ├── Cargo.toml                   # EDIT: gains axum/sqlx/kernel/config deps (was placeholder)
│   └── src/lib.rs                   # Principal, PlatformRole, principal-resolution middleware
│                                    #   (dev header → users row; env-gated per FR-019)
├── crates/modules/tenancy/
│   ├── Cargo.toml                   # EDIT: gains axum/sqlx/kernel/identity deps (was placeholder)
│   └── src/
│       ├── lib.rs                   # TenantContext + tenant-context middleware (validate
│       │                            #   X-Tenant-ID, authorize, propagate via request extension)
│       ├── authorize.rs             # membership / platform-role authorization queries
│       ├── audit.rs                 # audit writes: tenant switch, access denied
│       └── routes.rs                # GET /tenant, GET /platform/tenants,
│                                    #   POST /platform/tenants/{id}/switch, GET /me
├── crates/server/src/router.rs      # EDIT: mount /api/v1 routes + identity/tenancy middleware;
│                                    #   CORS allow-headers += X-Tenant-ID, X-Dev-User-Id
└── crates/server/tests/tenancy.rs   # NEW: isolation-matrix integration tests (live-gated)

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts             # NEW: TenantSummary, MeResponse, membership DTOs
│   ├── http/tenant-context.interceptor.ts   # NEW: attach X-Tenant-ID from store
│   ├── http/dev-identity.interceptor.ts     # NEW: dev-only X-Dev-User-Id
│   ├── state/tenant-context.feature.ts      # NEW: NgRx slice (activeTenant) + persistence effect
│   └── tenant/current-user.service.ts       # NEW: GET /me principal (platform vs tenant user)
│   └── tenant/tenant-context.service.ts     # NEW: façade — select/switch/clear tenant
├── layout/topbar/tenant-switcher.component.ts  # NEW: Taiga-wrapped switcher (platform users only)
└── app.config.ts                            # EDIT: register interceptors + store feature
```

**Structure Decision**: Backend logic goes into the existing placeholder module crates (`identity`, `tenancy`) per the spec-004 modular-monolith layout — the server crate only registers middleware and routes. Frontend additions follow the spec-002 layering: `core/` for singletons (interceptors, store slice, services), `layout/topbar` for the switcher UI; existing fixture-driven feature pages are untouched (FR-014a).

## Complexity Tracking

> No constitution violations — table intentionally left empty. (The per-platform-role gate on the tenant directory from 001 is sequenced to the RBAC feature; recorded in Constitution Check row III and contracts/http-api.md.)
