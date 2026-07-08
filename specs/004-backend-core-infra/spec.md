# Feature Specification: Backend Core Infrastructure

**Feature Branch**: `004-backend-core-infra`

**Created**: 2026-07-07

**Status**: Draft

**Input**: User description: "Spec 03 — Backend Core Infrastructure. Create the backend foundation that all modules depend on. Scope: HTTP server, application state, configuration, error handling, request IDs, logging, tracing, health endpoint, database connection pool, Redis connection, CORS, API response format. Implement GET /health, GET /ready, standard API error format, request tracing middleware, database pool initialization, Redis client initialization. Acceptance: backend exposes health checks; logs include request IDs; errors are returned in consistent JSON format; database and Redis connections are verified; tests cover health and error behavior."

## Clarifications

### Session 2026-07-07

- Q: When configuration is valid but PostgreSQL or Redis is unreachable at startup, what should the backend do? → A: Start serving with readiness failing; retry connections automatically until they succeed (no fail-fast, no bounded-retry exit).
- Q: When /ready fails, what should the response body be? → A: The same Health Report shape as a successful check (overall status + per-dependency status) with HTTP 503 — a documented operational-endpoint exception to the standard error envelope.
- Q: What format should request identifiers use? → A: `req_` prefix followed by a time-sortable unique identifier (matching the v1 contract example `req_01J...`); client-supplied `X-Request-Id` values are honored only when they match this format, otherwise replaced.
- Q: How should log output format be handled across environments? → A: Configurable via a runtime setting — machine-parseable JSON is the production/staging default; human-readable output is the local-development default. The request-identifier guarantee holds in both formats.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator verifies the service is alive and ready (Priority: P1)

An operator (or an orchestration system such as a load balancer or container
scheduler) needs to know two distinct things about a running backend instance:
whether the process is alive and able to answer requests, and whether it is
ready to serve real traffic — meaning its critical dependencies (database and
cache/session store) are reachable and responding.

**Why this priority**: Without health and readiness signals, the service cannot
be deployed safely anywhere — no rolling deploys, no automatic restarts, no
traffic gating. It is also the smallest independently valuable slice: a running
server with observable status, which every later module builds on.

**Independent Test**: Start the backend with valid configuration and reachable
dependencies; call the liveness endpoint and the readiness endpoint and confirm
both report success. Stop the database or cache and confirm the readiness
endpoint reports failure while the liveness endpoint still reports success.

**Acceptance Scenarios**:

1. **Given** a running backend instance, **When** the liveness endpoint is called, **Then** it responds with a success status and a machine-readable body indicating the service is up.
2. **Given** a running backend with reachable database and cache, **When** the readiness endpoint is called, **Then** it responds with a success status and reports each dependency as healthy.
3. **Given** a running backend whose database (or cache) is unreachable, **When** the readiness endpoint is called, **Then** it responds with a non-success status identifying which dependency check failed, while the liveness endpoint continues to report success.
4. **Given** invalid or incomplete startup configuration (e.g., missing database connection settings), **When** the backend starts, **Then** it fails fast with a clear error describing the missing/invalid setting rather than starting in a broken state.

---

### User Story 2 - API consumers receive consistent, structured errors (Priority: P2)

Any client of the API (dashboard, widget, or programmatic integration) that
triggers an error — a nonexistent route, a malformed request body, an
unauthorized call, or an internal failure — receives the same predictable JSON
error envelope: a stable error code, a human-readable message, optional field
details, and the request identifier for support/debugging.

**Why this priority**: The error contract is consumed by every future module
and by the already-specified frontend HTTP layer (`ApiError`). Establishing it
now prevents each module from inventing its own error shape and prevents
breaking changes later.

**Independent Test**: Issue requests that trigger each error class (unknown
route, malformed body, internal failure) and verify every non-success response
matches the standard envelope, carries an appropriate status code, and includes
the request identifier.

**Acceptance Scenarios**:

1. **Given** a running backend, **When** a client requests a route that does not exist, **Then** the response is a not-found status with the standard error envelope (code, message, request identifier).
2. **Given** a running backend, **When** a client sends a syntactically invalid request body to an endpoint expecting one, **Then** the response is a client-error status with the standard error envelope.
3. **Given** an unexpected internal failure while handling a request, **When** the response is produced, **Then** it is a server-error status with the standard envelope, the message does not leak internal details (stack traces, connection strings), and the full failure detail is captured in server logs under the same request identifier.
4. **Given** any error response, **When** its body is inspected, **Then** it conforms to the platform error envelope defined in the v1 REST API contract (`error.code`, `error.message`, optional `error.details[]`, `error.request_id`).

---

### User Story 3 - Every request is traceable end to end (Priority: P3)

A developer or support engineer investigating a problem takes the request
identifier from an API response (or from a client bug report) and finds every
log line and trace span the backend produced while handling that exact request.

**Why this priority**: Traceability is a constitutional requirement
(Observability by Default) and is cheapest to establish before any business
logic exists; retrofitting it is costly. It depends on the server existing (P1)
but not on the error contract (P2).

**Independent Test**: Send a request, read the request identifier from the
response header, and confirm server logs for that request all carry the same
identifier; send a request with a client-supplied identifier and confirm it is
honored or a fresh one is issued per policy.

**Acceptance Scenarios**:

1. **Given** an incoming request without a request identifier, **When** the backend handles it, **Then** a unique identifier is generated, returned on the response in the standard header, and attached to every log entry emitted while handling that request.
2. **Given** an incoming request that already carries a request identifier in the platform format (`req_` + time-sortable ID), **When** the backend handles it, **Then** that identifier is propagated unchanged through logs and the response header.
3. **Given** the machine-parseable (JSON) log format is configured, **When** any request is processed, **Then** at least one log record captures method, path, status, duration, and request identifier in machine-parseable form.

---

### User Story 4 - Browser clients can call the API across origins (Priority: P4)

The dashboard and the embeddable widget run on different origins than the API.
A browser-based client on an allowed origin can call the API successfully
(including preflight), while a client on a disallowed origin is refused by the
browser's cross-origin rules.

**Why this priority**: Required before any browser client can integrate, but it
has no value until the server, errors, and health endpoints exist.

**Independent Test**: Issue a cross-origin preflight request from an allowed
origin and verify permissive headers are returned; issue one from a disallowed
origin and verify it is not granted.

**Acceptance Scenarios**:

1. **Given** a configured list of allowed origins, **When** a preflight request arrives from an allowed origin, **Then** the response grants the origin, the standard methods, and the headers the API uses (including the request-identifier header).
2. **Given** a configured list of allowed origins, **When** a preflight request arrives from an origin not on the list, **Then** the response does not grant cross-origin access.

---

### Edge Cases

- Readiness is called while a dependency is slow rather than down: the check must time-bound each dependency probe so the endpoint responds within a known ceiling instead of hanging.
- The process receives a shutdown signal while requests are in flight: the server stops accepting new connections and allows in-flight requests a bounded grace period to complete.
- A handler panics or an unhandled failure occurs: the client still receives a well-formed standard error envelope (server-error status), and the process keeps serving other requests.
- A client supplies a malformed or abusive request identifier (wrong format, excessive length): the backend discards it and issues its own rather than echoing unsafe input into logs and headers.
- Configuration contains secrets (database credentials): startup logging and error messages must never print secret values, including inside connection-failure errors.
- The database is reachable but the cache is not (or vice versa): readiness reports per-dependency status so the operator can see exactly which dependency failed.
- A dependency is unreachable at boot (e.g., the backend starts before its database in an orchestrated environment): the process still starts, liveness reports success, readiness reports the failing dependency, and connectivity is retried automatically until it recovers — no restart required.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST run an HTTP service that accepts requests on a configurable network address and port.
- **FR-002**: System MUST load all runtime configuration (network address, database settings, cache settings, allowed CORS origins, log verbosity) from the deployment environment, validate it at startup, and refuse to start with a descriptive error when required settings are missing or invalid.
- **FR-003**: System MUST hold shared runtime resources (database connection pool, cache client, configuration) in a single application state made available to all request handlers, so future modules consume the same managed resources rather than creating their own.
- **FR-004**: System MUST expose a liveness endpoint (`GET /health`) that reports success whenever the process can serve requests, without consulting external dependencies.
- **FR-005**: System MUST expose a readiness endpoint (`GET /ready`) that verifies connectivity to the database and the cache and reports overall readiness plus per-dependency status. Success and failure use the same Health Report body shape; failure returns HTTP 503. This is the sole documented exception to the error-envelope rule in FR-009.
- **FR-006**: Readiness dependency checks MUST each be bounded by a timeout so the readiness endpoint always responds within a predictable ceiling.
- **FR-007**: System MUST initialize a database connection pool at startup with configurable sizing and acquisition timeout, and MUST verify database connectivity before declaring the instance ready.
- **FR-008**: System MUST initialize a cache (Redis) client/connection at startup and MUST verify cache connectivity before declaring the instance ready.
- **FR-008a**: An unreachable database or cache at startup MUST NOT prevent the process from starting (only invalid configuration does): the service starts serving, reports the failing dependency via the readiness endpoint, and retries connectivity automatically until it succeeds — readiness recovers without a restart.
- **FR-009**: System MUST return every non-success response using the platform error envelope defined in the v1 REST API contract: `error.code` (stable machine-readable code), `error.message` (human-readable, safe to display), optional `error.details[]` (field-level entries), and `error.request_id`. Sole exception: a failing readiness check returns the Health Report shape (FR-005).
- **FR-010**: System MUST map error classes to appropriate HTTP status codes consistent with the v1 REST API contract (client validation, unauthenticated, unauthorized, not found, conflict, semantic rejection, rate-limited, server error), and unknown routes MUST return the standard envelope rather than a bare or default body.
- **FR-011**: Internal failures MUST NOT leak implementation details (stack traces, queries, connection strings, secret values) to clients; full detail MUST be recorded server-side under the request identifier.
- **FR-012**: System MUST assign every request a unique request identifier in the format `req_` followed by a time-sortable unique identifier (per the v1 contract example `req_01J...`). A caller-supplied `X-Request-Id` is honored only when it matches this format; otherwise it is replaced. The identifier is returned on every response in the `X-Request-Id` header and included in the error envelope.
- **FR-013**: System MUST emit structured logs with a configurable output format: machine-parseable JSON (the default for deployed environments) or human-readable text (the default for local development). In either format, every log record produced while handling a request MUST carry that request's identifier, and each completed request MUST produce a summary record with method, path, status, and duration.
- **FR-014**: System MUST support distributed-tracing instrumentation such that each request produces a trace span (carrying the request identifier) that future modules can attach child spans to, with verbosity controlled by configuration.
- **FR-015**: System MUST enforce cross-origin resource sharing using a configurable allowlist of origins, permitting the methods and headers the API uses (including the request-identifier header) for allowed origins and denying all others.
- **FR-016**: System MUST shut down gracefully on termination signals: stop accepting new connections and allow in-flight requests a bounded grace period to complete.
- **FR-017**: System MUST provide the standard success-response conventions (JSON bodies, ISO 8601 UTC timestamps, `X-Request-Id` on every response) as reusable foundations that future endpoints inherit rather than reimplement, consistent with the v1 REST API contract.
- **FR-018**: Automated tests MUST cover: liveness success, readiness success with healthy dependencies, readiness failure per unhealthy dependency, unknown-route error envelope, malformed-request error envelope, internal-failure error envelope (no detail leakage), request-identifier generation and propagation (response header and logs), and CORS allow/deny behavior.

### Key Entities

- **Application Configuration**: The validated set of runtime settings the service starts with — network binding, database settings, cache settings, allowed origins, log verbosity. Sourced from the deployment environment; never contains secrets in logs or error output.
- **Application State**: The shared container of long-lived resources (database pool, cache client, configuration) handed to every request handler; the single seam through which future modules access infrastructure.
- **Request Context**: Per-request data established by middleware — the request identifier and trace span — that follows the request through handlers, logs, and the response.
- **Error Envelope**: The single JSON error shape for all non-success responses: stable code, safe message, optional field details, request identifier.
- **Health Report**: The readiness endpoint's body — overall status plus per-dependency (database, cache) status used by operators and orchestrators.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An orchestrator or operator can distinguish "process alive" from "ready for traffic" using two dedicated endpoints, and readiness flips to failing within one probe interval of a dependency (database or cache) becoming unreachable.
- **SC-002**: The liveness endpoint responds in under 100 ms, and the readiness endpoint responds within its configured dependency-timeout ceiling even when a dependency is unresponsive (it never hangs).
- **SC-003**: 100% of non-success API responses — including unknown routes and unexpected internal failures — conform to the standard error envelope and carry a request identifier.
- **SC-004**: Given only the request identifier from any API response, an engineer can locate every log record for that request; 100% of request-scoped log records carry the identifier.
- **SC-005**: With invalid or missing required configuration, the service exits at startup with a descriptive error in under 5 seconds instead of running in a degraded state.
- **SC-006**: A browser client on an allowed origin completes cross-origin API calls successfully; one on a disallowed origin is refused — verified for both cases by automated tests.
- **SC-007**: All acceptance behavior above is covered by automated tests that pass in the project's standard verification run, and no client-visible error output contains internal implementation details or secret values.

## Assumptions

- The error envelope, status-code usage, `X-Request-Id` header, and response conventions follow the existing v1 REST API contract (`specs/001-ai-customer-service-platform/contracts/rest-api.md`); this feature implements those conventions rather than redefining them.
- This feature delivers infrastructure only: no business endpoints, no authentication/authorization logic, no tenant-aware data access. Authorization (Constitution Principle III) applies to business endpoints introduced by later modules; the liveness/readiness endpoints are intentionally unauthenticated operational endpoints, exposed without sensitive detail.
- The backend technology stack is fixed by the constitution (Rust, Axum, Tokio, SQLx, PostgreSQL, Redis, Tracing); the existing Cargo workspace under `backend/` is the starting point.
- "Verified" database/Redis connections means connectivity is probed at startup and on every readiness check; deeper checks (migration status, replication lag) are out of scope for this feature.
- Rate limiting, idempotency-key handling, pagination helpers, and metrics endpoints are acknowledged by the v1 contract but are out of scope here; the foundations laid (state, middleware, envelope) must not preclude adding them.
- Local development uses locally reachable PostgreSQL and Redis instances; provisioning them (e.g., container tooling) is an environment concern, not part of this feature's deliverable.
- Trace export to an external collector is out of scope; this feature establishes in-process spans and structured logs that a later observability feature can wire to a collector without rework.
