# Research: Backend API Documentation (Swagger/OpenAPI)

**Feature**: 016-backend-swagger-docs | **Date**: 2026-07-15

This document resolves the tooling and integration unknowns from the Technical Context. All decisions target the existing Rust/Axum 0.8 modular-monolith backend.

## Decision 1: OpenAPI generation crate — `utoipa`

**Decision**: Use `utoipa` (with the `openapi31` feature for OpenAPI 3.1) for schema derivation and path annotation, plus `utoipa-axum` for router integration.

**Rationale**:
- Code-first via proc-macros (`#[derive(ToSchema)]`, `#[derive(IntoParams)]`, `#[utoipa::path]`) — the spec is generated from the same Rust types and handlers that serve traffic, directly satisfying FR-012 (single source of truth) and SC-006 (model change reflected on rebuild).
- Mature, actively maintained, first-class Axum support, high documentation coverage.
- Derives emit a compile error when a type cannot be represented as a schema, satisfying the "fail loudly" edge case at build time rather than runtime.

**Alternatives considered**:
- **`aide`**: good Axum support but leans toward inferring schemas from handler return types. Our handlers return the untyped `axum::response::Response` (see Decision 4), so inference yields nothing useful — we would annotate manually anyway, losing aide's main advantage.
- **`okapi`/`schemars` + Rocket**: framework mismatch (not Axum).
- **Hand-written `openapi.yaml`**: violates FR-012/SC-006 (drifts immediately; forbidden by the spec's "never hand-maintained" requirement).
- **`utoipauto`** (auto-discovery of annotated items): reduces boilerplate registration but adds compile-time reflection magic and a coverage blind spot. Rejected in favor of explicit `utoipa-axum` registration, which is what makes the coverage guarantee (Decision 3) structural.

## Decision 2: Router integration — `utoipa-axum` `OpenApiRouter`

**Decision**: Migrate the `server` crate's route composition from the bespoke `ProtectedRoutes` builder to `utoipa-axum`'s `OpenApiRouter`, registering handlers with the `routes!` macro.

**Rationale**:
- `routes!(handler)` bundles the axum `MethodRouter` **and** the handler's OpenAPI path into one `UtoipaMethodRouter`; `OpenApiRouter::routes()`, `.nest()`, and `.merge()` carry both together. A route registered through the router is therefore *always* in the generated spec — omission becomes structurally impossible for routes added this way, which is the backbone of FR-015.
- The existing RBAC middleware pattern is preserved: `UtoipaMethodRouterExt::layer` (and route-scoped equivalents) applies the `require_permission(permission)` tower layer to a registered route exactly as `route_layer` does today. The `guarded`/`guarded_with_methods` helpers are re-expressed against `OpenApiRouter` while keeping the same per-route permission wiring and the `authentication_middleware`/`tenant_context`/`platform_permission` layer stacks.
- `split_for_parts()` (or `into_openapi()`) yields the final `axum::Router` plus the assembled `OpenApi` value, so the app is built once and both the live router and the spec derive from the same registration.

**Alternatives considered**:
- **Keep `ProtectedRoutes`, attach a hand-maintained `#[openapi(paths(...))]` list**: works, but the path list and the route list are two sources that drift; the coverage test would be the only guard, and it would be comparing a hand-list to a hand-list. Rejected — `OpenApiRouter` makes co-registration the default.

## Decision 3: Route-coverage enforcement (FR-015 / SC-001)

**Decision**: A `#[test]` in `server/tests/openapi_coverage.rs` builds the production (non-test) app, extracts the set of documented paths+methods from the generated `OpenApi`, and asserts it equals the set of routes the app actually serves. Because routes and paths co-register through `OpenApiRouter` (Decision 2), the assertion is that **every served `/api/v1` route and each operational route has a corresponding documented operation, and no documented operation lacks a route**. The test fails, naming the offending method+path, when they diverge.

**Rationale**:
- Axum 0.8 does not expose a public API to enumerate registered routes at runtime. Rather than reflect over the router, we make registration the single source: the test asserts the documented-path set matches an explicit expected-inventory constant that is also asserted to be exhaustive by a second check (each documented path is reachable — returns non-404 — when probed). This converts "did someone add a route without docs" into a failing test.
- Operational endpoints (`/health`, `/ready`, `/metrics`) are documented and included in the inventory; test-only routes are excluded because they are never registered through the documented `OpenApiRouter` (they remain on the plain axum router behind `include_test_routes`).

**Alternatives considered**:
- **Runtime router introspection**: no stable Axum API; rejected.
- **Review-only / advisory**: explicitly rejected by the clarification (hard quality gate chosen).

## Decision 4: Response/request annotation strategy (untyped handlers)

**Decision**: Annotate every handler with `#[utoipa::path(...)]` declaring `request_body`, `params`, `responses` (per status code), `security`, and `tag` **explicitly**, rather than relying on return-type inference. Add `#[derive(ToSchema)]` to every request/response model. For handlers that build responses with inline `json!({"data": ...})` envelopes, introduce doc-only wrapper schema types (e.g., `CustomerDetailResponse { data: CustomerDetail }`) that mirror the exact JSON shape the handler emits.

**Rationale**:
- Handlers return `axum::response::Response` and consume the custom `kernel::ApiJson<T>` extractor, so utoipa cannot infer bodies from signatures. Explicit `#[utoipa::path]` attributes are required and give precise control over status codes and error variants (FR-005–FR-008).
- The codebase has **inconsistent response envelopes** that must be documented faithfully (the spec documents existing behavior, it does not change it):
  - `{ "data": <object> }` (customers get/create/update, most detail endpoints)
  - `{ "data": [...], "pagination": { "next_cursor", "has_more" } }` (customers list)
  - `kernel::Page<T>` = `{ "items": [...], "nextCursor": ..., "hasMore": ... }` (endpoints returning `Page<T>`)
  Each distinct envelope becomes a named, reusable schema so the variance is explicit rather than hidden behind a loose `object`.
- Wrapper types are documentation artifacts only; they do not alter the bytes on the wire.

**Alternatives considered**:
- **Refactor handlers to return typed `Json<T>` so inference works**: larger, behavior-adjacent change touching every handler; out of scope for a docs feature and riskier. Rejected — annotate in place instead.
- **Normalize all envelopes to one shape**: would change the API contract; explicitly out of scope (this feature documents, not redesigns).

## Decision 5: Interactive UI — self-hosted Swagger UI

**Decision**: Serve the interactive page with `utoipa-swagger-ui` at `/swagger-ui`, and the raw document at `/api-docs/openapi.json`.

**Rationale**:
- Self-hosted (bundled assets), so the docs always match the deployed binary and require no external network — consistent with FR-002 and the "served by the backend itself" assumption.
- Swagger UI is the tool users mean by "swagger"; it renders types, enums, required/nullable, and security per operation out of the box (FR-001, FR-005–FR-009).

**Alternatives considered**:
- **`utoipa-redoc`** / **`utoipa-scalar`**: both are viable, cleaner-looking renderers and are drop-in with the same `OpenApi` value. Either can be substituted at implementation time with no spec impact. Swagger UI chosen as the default because it best matches the literal request ("add swagger") and supports "try it out". Note: `utoipa-swagger-ui` compiles a vendored asset bundle; if build-time footprint is a concern, `utoipa-scalar` (single JS include) is the fallback.

## Decision 6: Security scheme — session cookie via `Modify`

**Decision**: Register a single `SecurityScheme::ApiKey` in cookie `app_session` (the existing session cookie) via a `Modify` implementation on the root `#[derive(OpenApi)]` type. Public endpoints (`/auth/login`, `/auth/logout`, `/invitations/{token}`, `/invitations/{token}/accept`, and the operational endpoints) declare `security(())` (no requirement); all others require the cookie scheme. Each guarded operation additionally documents its required RBAC permission in its description/summary (FR-009).

**Rationale**:
- Matches the real auth mechanism (httpOnly `app_session` cookie set by login; see `007-authentication`). Documents *how* auth is supplied per endpoint.
- The `Modify` trait is the idiomatic utoipa hook for adding a security scheme, servers, and tags to the assembled document.

**Alternatives considered**:
- **Bearer/JWT scheme**: the JWT lives inside the cookie, not an `Authorization` header; documenting Bearer would misrepresent the contract. Rejected.

## Decision 7: Environment gating (FR-014)

**Decision**: Add an `AppConfig` field `docs_enabled: bool` sourced from `APP_DOCS_ENABLED`. Effective exposure = `environment ∈ {Development, Test, Staging?}` OR `docs_enabled`. In `Production`, docs mount only when `APP_DOCS_ENABLED=true`; default (unset) is off. Dev/Test always mount regardless of the flag.

**Rationale**:
- Follows the existing `AppConfig::from_env()` pattern (optional env vars parsed with defaults). Keeps the production surface closed by default per the clarification, while leaving an explicit opt-in.
- The docs routes (`/swagger-ui`, `/api-docs/openapi.json`) are conditionally merged into the app router based on this computed boolean; when disabled they are simply absent (404 via the normal fallback).

**Open sub-question deferred to tasks**: whether `Staging` should behave like production (gated) or like development (open). Default assumption: Staging is gated like production (safer). This does not affect any FR and can be a one-line change.

## Decision 8: SSE endpoint documentation (FR-010)

**Decision**: Document `GET /tenant/events` as an operation producing `text/event-stream`, with the event payload schemas (`EscalationAssignedEvent`, `EscalationQueuedEvent`, `EscalationRemovedEvent`, `AvailabilityChangedEvent`) registered as components and referenced from the operation description as the possible event `data` shapes.

**Rationale**:
- OpenAPI cannot fully model an SSE stream body, but the content type and the discrete event payloads (already `Serialize` structs in `escalations::model`) can be declared as components and described, satisfying FR-010's intent (consumers learn the stream type and each event's fields/types).

**Alternatives considered**:
- **Omit the endpoint**: violates FR-003 (must cover all non-test endpoints). Rejected.
- **Document as `application/json`**: misrepresents the contract. Rejected.
