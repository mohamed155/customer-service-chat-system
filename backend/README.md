# Backend

REST API for the AI Customer Service Platform — Rust / Axum 0.8 modular monolith.

## Quick reference

| Task | Command |
|------|---------|
| Build | `cargo build` |
| Test (lib) | `cargo test --lib` |
| Test (integration) | `cargo test --tests` |
| Lint | `cargo clippy --workspace` |
| Format check | `cargo fmt --check` |
| Run | `cargo run -p server` |

## API documentation (Swagger / OpenAPI)

The backend publishes a code-first OpenAPI 3.1 specification and an
interactive Swagger UI alongside the API.

| Surface | URL | When served |
|---------|-----|-------------|
| Interactive UI | `/swagger-ui` | dev / test always; production only when `APP_DOCS_ENABLED=true` |
| Raw spec (JSON) | `/api-docs/openapi.json` | same gate as the UI |

Both are generated from the same Rust types and `#[utoipa::path]`
annotations that serve traffic; there is no separate hand-maintained
document. The generation uses
[`utoipa`](https://crates.io/crates/utoipa) +
[`utoipa-axum`](https://crates.io/crates/utoipa-axum) +
[`utoipa-swagger-ui`](https://crates.io/crates/utoipa-swagger-ui).
The router is migrated to `utoipa-axum`'s `OpenApiRouter` so a route
registered through the documented router *always* has a corresponding
OpenAPI path — undocumented routes are a structural impossibility, not a
convention.

### Environment gating

| `APP_ENVIRONMENT` | `APP_DOCS_ENABLED` | Docs served? |
|-------------------|--------------------|---------------|
| `development`     | any                | yes           |
| `test`            | any                | yes           |
| `staging`         | unset / `false`    | **no**        |
| `staging`         | `true`             | yes (opt-in)  |
| `production`      | unset / `false`    | **no**        |
| `production`      | `true`             | yes (opt-in)  |

`staging` is gated like `production` (safer default — see the comment on
`server::router::docs_surface_enabled`). Flip it to behave like
development by changing that one condition.

### Completeness enforcement

`backend/crates/server/tests/openapi_coverage.rs` is the regression
guardrail: it compares the documented path+method set against the
authoritative inventory in
`specs/016-backend-swagger-docs/contracts/openapi-coverage.md` and
fails on any drift (missing or extra operations). Add a route → annotate
it with `#[utoipa::path]` → register it in `server::openapi::ApiDoc`'s
`paths(...)` and `components(schemas(...))` lists. The test then
demands the corresponding entry in the contract file.

`backend/crates/server/tests/openapi_valid.rs` asserts the document is
a structurally valid OpenAPI 3.1 doc, declares all required tag
groups, and that no response schema exposes a `password` or `api_key`
field (FR-011).

`backend/crates/server/tests/openapi_contract.rs` validates that
documented response bodies resolve to registered schemas and that
`ErrorEnvelope` carries `error.code`, `error.message`, `error.details`,
and `error.request_id`.

### Why no separate yaml?

`utoipa::OpenApi` is the single source of truth. The JSON document is
derived from the same Rust types and handler annotations that serve
traffic; hand-edited yaml drifts the moment the code changes. See
`specs/016-backend-swagger-docs/quickstart.md` for end-to-end validation
scenarios.
