# Quickstart: Backend API Documentation (Swagger/OpenAPI)

**Feature**: 016-backend-swagger-docs | **Date**: 2026-07-15

This guide validates that the OpenAPI documentation is generated, served, complete, and accurate. It assumes the implementation from `tasks.md` is in place. Run from `backend/`.

## Prerequisites

- Rust toolchain (workspace builds with `cargo build`).
- PostgreSQL reachable per `.env` (only needed for the running-server and contract checks, not for the spec-validity or coverage tests).
- Environment: default local `.env` sets `APP_ENVIRONMENT=development` (docs served by default).

## Scenario 1 — Interactive docs load in a non-production env (US1)

```bash
# Start the server (development env → docs enabled by default)
cargo run -p server
```

- Open `http://localhost:<PORT>/swagger-ui` → the interactive reference loads.
- **Expected**: every functional area (auth, invitations, identity, platform-tenants, platform-ai, tenant, customers, conversations, escalations, members, tenant-ai, ops) appears as a tag group; expanding any operation shows method, path, params with types, request body schema, response schemas, the `session_cookie` security requirement, and the required RBAC permission in the description.

## Scenario 2 — Machine-readable spec is valid (US2, FR-013, SC-003)

```bash
# Fetch the raw document from a running server
curl -s http://localhost:<PORT>/api-docs/openapi.json -o /tmp/openapi.json

# Validate with any off-the-shelf validator, e.g.:
npx @redocly/cli lint /tmp/openapi.json
# or import /tmp/openapi.json into Postman → all endpoints/params/models import cleanly
```

- **Expected**: zero validation errors. The document declares OpenAPI 3.1, the `session_cookie` security scheme, and the `/api/v1` server.
- The same assertion runs offline as a test (no server needed):

```bash
cargo test -p server --test openapi_valid
```

## Scenario 3 — Coverage is complete (US3, FR-015, SC-001)

```bash
cargo test -p server --test openapi_coverage
```

- **Expected**: PASS. The test compares the app's registered non-test routes against the documented path+method set (see `contracts/openapi-coverage.md`).
- **Regression demo** (proves the gate bites): add a new route to the router without a `#[utoipa::path]` annotation / without registering it via `routes!`, then re-run — the test FAILS and names the undocumented method+path. Revert.

## Scenario 4 — Test-only routes are absent (US3, FR-004)

```bash
cargo test -p server --test openapi_coverage -- excludes_test_routes
```

- **Expected**: the generated document contains none of the `/test/...`, `/test-echo`, or `/test-panic` paths, even when the app is built with `include_test_routes = true`.

## Scenario 5 — Live responses match documented schemas (SC-005)

```bash
cargo test -p server --test openapi_contract
```

- **Expected**: for a sample of ≥10 endpoints spanning all functional areas, the live JSON response validates against the documented schema (field names, types, optionality). Covers at least one of each envelope variant (`{data}`, `{data,pagination}`, `Page<T>`, `PaginatedResponse<T>`) and the error envelope.

## Scenario 6 — Docs gated in production (FR-014)

```bash
# Production without opt-in → docs absent
APP_ENVIRONMENT=production APP_DOCS_ENABLED=false cargo run -p server
curl -i http://localhost:<PORT>/swagger-ui        # → 404
curl -i http://localhost:<PORT>/api-docs/openapi.json  # → 404

# Production with explicit opt-in → docs present
APP_ENVIRONMENT=production APP_DOCS_ENABLED=true cargo run -p server
curl -i http://localhost:<PORT>/swagger-ui        # → 200
```

- **Expected**: docs are unreachable in production unless `APP_DOCS_ENABLED=true`; always reachable in development/test.

## Scenario 7 — Secrets never leak (FR-011)

- In `/tmp/openapi.json`, confirm `CredentialPayload.api_key` and `LoginRequest.password` are marked `writeOnly: true`.
- Confirm no response schema (e.g. `CredentialView`, `AiConfigurationView`) contains an `api_key`/`password` field.

```bash
cargo test -p server --test openapi_valid -- no_secrets_in_responses
```

## Done when

- Scenarios 1–7 pass.
- `cargo test -p server` is green (coverage, validity, and contract tests included).
- `cargo clippy --workspace` and `cargo fmt --check` pass (project quality gates).
