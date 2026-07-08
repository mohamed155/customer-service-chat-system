# Implementation Plan: Backend Core Infrastructure

**Branch**: `master` (no feature branch in use) | **Date**: 2026-07-07 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/004-backend-core-infra/spec.md`

## Summary

Deliver the backend foundation every later module depends on: an Axum HTTP
server with shared application state (Postgres pool, Redis client, validated
config), root-level `GET /health` and `GET /ready` endpoints, the v1 error
envelope for all failures, `req_`-prefixed request-ID middleware feeding
structured logs and trace spans, a configurable CORS allowlist, and graceful
shutdown. The existing scaffolding in `backend/crates/shared/*` and
`backend/crates/server` (error envelope, request-ID middleware, config loader,
health stubs) is extended and corrected — not rebuilt. Startup never blocks on
unreachable dependencies: the service boots unready and recovers automatically
(spec clarification, FR-008a).

## Technical Context

**Language/Version**: Rust, edition 2021 (as currently set in `backend/Cargo.toml`; the CLAUDE.md "edition 2024" note is a docs/workspace mismatch — changing editions is out of scope for this feature and would touch every crate)

**Primary Dependencies**: Axum 0.8, Tokio 1, SQLx 0.8 (postgres, runtime-tokio-rustls), `redis` crate (tokio + connection-manager features, to be added), tower-http (cors + trace features, to be added), tracing / tracing-subscriber (json + env-filter), serde, uuid (v7)

**Storage**: PostgreSQL (connection pool only — no schema changes in this feature; migrations runner already exists in `shared/db`), Redis (connectivity + readiness probe only)

**Testing**: `cargo test` — unit tests in-crate; router-level tests via `tower::ServiceExt::oneshot` against the assembled `Router`; readiness dependency probes mocked behind a small `HealthCheck` trait so no live Postgres/Redis is needed for the default test run; optional live-dependency integration tests gated on `TEST_DATABASE_URL`/`TEST_REDIS_URL`

**Target Platform**: Linux server (production); Windows for local development

**Project Type**: Web service (backend of an existing web application monorepo)

**Performance Goals**: `/health` p99 < 100 ms (SC-002); `/ready` bounded by per-dependency probe timeout (default 2 s) — never hangs; middleware overhead negligible (< 1 ms per request)

**Constraints**: Startup must fail fast (< 5 s) on invalid config but MUST NOT fail on unreachable dependencies (FR-008a); no secrets in logs or error bodies; error envelope per v1 REST contract with the single documented `/ready`-failure exception

**Scale/Scope**: Foundation feature — 5 shared crates touched (`config`, `db`, `kernel`, `observability`, new `cache` or Redis support inside `db`), 1 binary crate (`server`); no business endpoints, no tenant data

**Environment note**: Neither the Rust toolchain nor Docker is currently installed on this development machine. `quickstart.md` documents installation; implementation cannot be verified locally until `rustup` is installed.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Enterprise Modular Monolith | PASS | Infrastructure lives in `shared/*` crates consumed via explicit interfaces; module crates remain untouched and will consume `AppState`/error types through public APIs. |
| II | Multi-Tenant Isolation | PASS (N/A) | No tenant-owned data or tenant-aware queries introduced. |
| III | Zero-Trust Security & RBAC | **EXCEPTION — justified** | `/health` and `/ready` are unauthenticated. See Complexity Tracking: orchestrators (k8s probes, load balancers) cannot present credentials; endpoints expose no tenant/sensitive data (dependency up/down only). All future business endpoints remain subject to authz. No secrets in code; config from environment only. |
| IV | AI Provider Independence | PASS (N/A) | No AI surface in this feature. |
| V | API-First & Contract Consistency | PASS | Implements the v1 error envelope, `X-Request-Id`, and response conventions from `specs/001-ai-customer-service-platform/contracts/rest-api.md`; contract doc added under `contracts/`. |
| VI | Observability by Default | PASS | Core deliverable: request-ID propagation, structured logs, per-request trace spans. |
| VII | Test-First & Regression Discipline | PASS | FR-018 enumerates required coverage; tests written with/before implementation per task ordering. |
| VIII | Database Integrity & Migrations | PASS | No schema changes; existing migration runner reused for readiness-adjacent startup wiring only. |
| IX | Design System Discipline | PASS (N/A) | Backend-only feature. |
| X | Performance & Efficiency | PASS | Latency ceilings specified (SC-002); probes time-bounded; pool sizing configurable. |

**Post-design re-check (after Phase 1)**: unchanged — the only deviation remains the justified Principle III exception for the two operational endpoints.

## Project Structure

### Documentation (this feature)

```text
specs/004-backend-core-infra/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── core-http.md     # Phase 1 output: /health, /ready, envelope, headers, CORS
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── Cargo.toml                         # workspace: add redis, tower-http, futures deps
├── migrations/                        # unchanged (no schema changes)
└── crates/
    ├── server/
    │   └── src/
    │       ├── main.rs                # startup: config → state → router → serve + graceful shutdown
    │       ├── router.rs              # assembles Router<AppState>: root /health /ready, /api/v1 nest, fallback, layers
    │       └── state.rs               # AppState { config, db pool, redis, health registry }
    └── shared/
        ├── config/src/lib.rs          # extend AppConfig: cors origins, log format, probe timeout, pool sizing, shutdown grace
        ├── db/src/lib.rs              # lazy PgPool construction + Postgres HealthCheck impl
        ├── cache/                     # NEW crate: Redis client wrapper + Redis HealthCheck impl
        │   └── src/lib.rs
        ├── kernel/src/lib.rs          # ApiError: full status-code constructors; request-id integration
        └── observability/src/
            ├── lib.rs                 # init: json|pretty by config; re-exports
            ├── request_id.rs          # req_ format constant, validation, middleware (extracted)
            ├── trace.rs               # request span + summary-log middleware
            └── health.rs              # HealthCheck trait, HealthReport types, /health /ready handlers
```

**Structure Decision**: Extend the existing Cargo workspace in place. One new
shared crate (`shared/cache`) isolates the Redis dependency the same way
`shared/db` isolates SQLx — module crates never import `redis`/`sqlx` types
directly, preserving Principle I extraction boundaries. `server` gains
`router.rs`/`state.rs` so `main.rs` stays a thin composition root. Health
endpoints move from `/api/v1/*` to root (`/health`, `/ready`) per the spec;
the `/metrics` stub moves to root alongside them (still a stub, out of scope).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Principle III: `/health` and `/ready` served without authorization | Load balancers, container orchestrators, and uptime monitors must probe the service before/without credentials; this is the industry-standard operational pattern. Responses contain only overall/per-dependency up-down status — no tenant data, no versions of internal components, no configuration values. | Requiring auth on probes breaks k8s liveness/readiness and LB health checks (they cannot hold rotating credentials); a separate unauthenticated admin port adds deployment surface for no security gain at this stage. |
