# Contract: Core HTTP Surface (Backend Core Infrastructure)

**Feature**: 004-backend-core-infra | **Date**: 2026-07-07

Refines `specs/001-ai-customer-service-platform/contracts/rest-api.md` (the
v1 API contract) for the surface this feature delivers. Where the two
documents overlap, the v1 contract governs; this file adds the operational
endpoints (which live *outside* `/api/v1`) and pins foundation semantics.

## Endpoints

### GET /health — liveness

- **Auth**: none (documented Principle III exception — see plan.md Complexity Tracking).
- **Behavior**: no external dependency consultation; answers whenever the process can serve requests.
- **Response `200`** (always, while the process is up):

  ```json
  { "status": "ok" }
  ```

### GET /ready — readiness

- **Auth**: none (same exception).
- **Behavior**: probes each registered dependency (`database` = PostgreSQL `SELECT 1`, `cache` = Redis `PING`), each bounded by `READY_PROBE_TIMEOUT_MS` (default 2000 ms). Probe timeout ⇒ that check is unhealthy (`"timed out"`). Endpoint total time ≈ max single probe (probes run concurrently), never unbounded.
- **Response `200`** — all checks ok:

  ```json
  { "status": "ready", "checks": [
    { "name": "database", "status": "ok" },
    { "name": "cache", "status": "ok" } ] }
  ```

- **Response `503`** — any check failing. **Same body shape** (NOT the error envelope — the sole documented exception, spec clarification Q2):

  ```json
  { "status": "not_ready", "checks": [
    { "name": "database", "status": "ok" },
    { "name": "cache", "status": "error", "error": "timed out" } ] }
  ```

- `checks[].error` messages MUST be safe: no connection strings, credentials, or driver internals.

### GET /metrics — placeholder

Existing stub (`text/plain`, `# no metrics yet`) relocated to root. Out of
scope for this feature; listed only so the route move is contract-visible.

### Anything else

- Unknown route ⇒ `404` with the standard error envelope (`code: "not_found"`).
- `/api/v1` remains the base path for all future business endpoints per the v1 contract.

## Error envelope (all non-2xx except failing /ready)

```json
{ "error": {
    "code": "<stable_code>",
    "message": "<human-readable, safe>",
    "details": [ { "field": "...", "code": "...", "message": "..." } ],
    "request_id": "req_<uuidv7>" } }
```

Status ↔ code map implemented by this feature's `ApiError` constructors:

| HTTP | `error.code` |
|---|---|
| 400 | `validation_failed` |
| 401 | `unauthenticated` |
| 403 | `unauthorized` |
| 404 | `not_found` |
| 409 | `conflict` |
| 422 | `unprocessable` |
| 429 | `rate_limited` |
| 500 | `internal_error` |

500 bodies never leak internals (stack traces, SQL, connection strings);
full detail goes to server logs under the same `request_id`.

## Request identifier

- **Format**: `req_` + hyphenated lowercase UUIDv7 — regex
  `^req_[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$`
  (40 chars total). Time-sortable per clarification Q3.
- **Inbound**: a client `X-Request-Id` matching the format is honored;
  anything else (missing, malformed, wrong length/charset) is replaced with a
  freshly generated ID. The replaced value is never echoed back.
- **Outbound**: every response — success, error, `/health`, `/ready`,
  fallback — carries `X-Request-Id`.
- **Logs/traces**: every request-scoped log record and the per-request trace
  span carry `request_id`; the completion record adds `method`, `path`,
  `status`, `latency_ms`.

## CORS

- Allowlist from `CORS_ALLOWED_ORIGINS` (comma-separated exact origins).
- Preflight from an allowed origin ⇒ grants: methods
  `GET, POST, PATCH, PUT, DELETE, OPTIONS`; request headers `content-type`,
  `authorization`, `x-request-id`, `idempotency-key`; exposed response header
  `x-request-id`.
- Origin not on the list ⇒ no `Access-Control-Allow-Origin` grant.
- No wildcard origin in `production`.

## Conventions inherited from the v1 contract

JSON bodies; ISO 8601 UTC timestamps; opaque IDs; cursor pagination envelope
(`items`/`next_cursor`/`has_more` — types already in `kernel`, no list
endpoints ship in this feature); idempotency and rate-limit headers are
acknowledged but out of scope here.
