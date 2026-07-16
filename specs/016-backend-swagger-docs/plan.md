# Implementation Plan: Backend API Documentation (Swagger/OpenAPI)

**Branch**: `016-backend-swagger-docs` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/016-backend-swagger-docs/spec.md`

## Summary

Add a code-first OpenAPI 3.1 specification and an interactive documentation UI to the Rust/Axum backend, covering every non-test endpoint with fully typed request/response/error models. The specification is generated from the same route registrations and Rust types that serve traffic, using `utoipa` (schema derivation) + `utoipa-axum` (route+path co-registration so a route cannot exist without a documented path) + a self-hosted UI. A hard completeness test compares the router's registered paths against the documented paths and fails the build when they diverge. Documentation is served in dev/test always and in production only behind an explicit opt-in config flag. This feature documents existing behavior only; it changes no endpoint semantics.

## Technical Context

**Language/Version**: Rust (workspace edition 2021), Axum 0.8

**Primary Dependencies**: `utoipa` (OpenAPI schema + `ToSchema`/`IntoParams` derives, `openapi31` feature), `utoipa-axum` (`OpenApiRouter` for atomic route+path registration), `utoipa-swagger-ui` (self-hosted interactive UI; Scalar/Redoc considered — see research.md). Existing: serde, sqlx, chrono, uuid.

**Storage**: N/A for this feature (documentation is generated from code, not persisted). PostgreSQL remains the app's store, untouched.

**Testing**: `cargo test` (workspace). New: OpenAPI validity test (serialize the doc, assert it builds), route-coverage completeness test (registered paths == documented paths), and a sampled contract test asserting live responses match documented schemas.

**Target Platform**: Linux server (same binary as the existing backend).

**Project Type**: Web service (backend crate `server` composing module crates). Single-binary modular monolith.

**Performance Goals**: Documentation generation happens once at startup (spec built into a cached `OpenApi` value); serving the static UI and JSON adds negligible per-request cost. No latency budget change for existing endpoints.

**Constraints**: Docs disabled by default in production (opt-in flag per FR-014); no secrets in any documented response (credential/password inputs are write-only per FR-011); spec generation must fail loudly at startup/build if a model cannot be represented (edge case in spec).

**Scale/Scope**: ~53 `/api/v1` endpoints across 6 module crates (identity, tenancy, customers, conversations, escalations, ai) plus 3 operational endpoints; ~35 request/response model structs plus shared error and pagination envelopes and the SSE event payloads.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|-----------|------------|
| I. Enterprise Modular Monolith | PASS. Each module crate annotates its own types/handlers; the `server` crate composes them into one `OpenApi`. No new cross-module data access; module boundaries unchanged. |
| II. Multi-Tenant Isolation | PASS. Documentation-only; no query paths change. The docs surface exposes no tenant data (it is a static schema). |
| III. Zero-Trust Security & RBAC | PASS and reinforced. Docs are gated in production (opt-in flag), the cookie security scheme is documented per endpoint, and per-endpoint RBAC permissions are surfaced (FR-009). Write-only credential/password fields never appear in response schemas (FR-011). No secrets in source (the spec is generated, not hand-authored with values). |
| IV. AI Provider Independence | PASS. Not applicable to doc generation; AI config/credential endpoints are documented like any other, with credentials write-only. |
| V. API-First & Contract Consistency | PASS and directly advanced. This feature makes the REST-first, versioned, standardized-error contract a first-class, machine-readable deliverable — the core intent of this principle. |
| VI. Observability by Default | PASS. No change to request-id propagation, logging, or tracing. The `request_id` field of the error envelope is documented. |
| VII. Test-First & Regression Discipline | PASS. Coverage test (FR-015), OpenAPI validity test (FR-013), and sampled contract tests (SC-005) are written as the enforcement mechanism; the coverage test is the permanent regression guardrail against undocumented routes. |
| VIII. Database Integrity & Migrations | PASS. No schema changes, no migrations. |
| IX. Design System Discipline | N/A. The interactive docs page is a self-hosted third-party UI asset, not part of the Angular design system; no product UI is added. |
| X. Performance & Efficiency | PASS. Spec built once and cached; no N+1 or hot-path impact. |

**Result**: No violations. Complexity Tracking not required.

## Project Structure

### Documentation (this feature)

```text
specs/016-backend-swagger-docs/
├── plan.md              # This file
├── research.md          # Phase 0 output — tooling decisions
├── data-model.md        # Phase 1 output — schema/annotation inventory
├── quickstart.md        # Phase 1 output — validation guide
├── contracts/
│   └── openapi-coverage.md   # Endpoint → request/response/error/permission map
└── checklists/
    └── requirements.md  # Spec quality checklist (from /speckit-specify)
```

### Source Code (repository root)

```text
backend/crates/
├── shared/
│   ├── kernel/src/lib.rs          # + #[derive(ToSchema)] on ErrorEnvelope, ErrorBody,
│   │                              #   ErrorDetail, Page<T>, PageParams (shared schemas)
│   └── config/src/lib.rs          # + docs_enabled flag (APP_DOCS_ENABLED) on AppConfig
├── modules/
│   ├── identity/src/routes.rs     # ToSchema on LoginRequest + response types; #[utoipa::path]
│   ├── tenancy/src/{routes,members,invitations}.rs   # ToSchema + path annotations
│   ├── customers/src/{model,routes}.rs               # ToSchema + doc-only response wrappers
│   ├── conversations/src/{model,routes}.rs           # ToSchema + path annotations
│   ├── escalations/src/{model,routes,events}.rs      # ToSchema incl. SSE event payloads
│   └── ai/src/{model,routes,usage}.rs                # ToSchema; credentials write-only
└── server/src/
    ├── router.rs        # Migrate ProtectedRoutes/api_routes to utoipa-axum OpenApiRouter;
    │                    #   mount docs UI + /api-docs/openapi.json behind env gate
    ├── openapi.rs       # NEW — #[derive(OpenApi)] root doc: security scheme, tags,
    │                    #   servers, component registration, coverage helper
    ├── handlers.rs      # ToSchema/path for the two server-local composite handlers
    └── state.rs         # (unchanged unless docs flag threaded through AppState)

backend/crates/server/tests/
├── openapi_valid.rs     # NEW — spec serializes & passes OpenAPI validation (FR-013/SC-003)
├── openapi_coverage.rs  # NEW — registered paths == documented paths (FR-015/SC-001)
└── openapi_contract.rs  # NEW — sampled live-response-vs-schema checks (SC-005)
```

**Structure Decision**: Web-service layout, reusing the existing modular-monolith crate structure. Annotations live beside the types and handlers they describe (each module owns its schemas, per Principle I); the `server` crate owns the root `OpenApi` document, the security scheme, tag grouping, the docs-serving routes, and the enforcement tests. The router migrates from the bespoke `ProtectedRoutes` builder to `utoipa-axum`'s `OpenApiRouter` so route registration and path documentation share a single source of truth (satisfying FR-012 and making FR-015 enforceable structurally rather than by convention).

## Complexity Tracking

> No Constitution Check violations. Section intentionally empty.
