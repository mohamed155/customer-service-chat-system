# Data Model: Backend Core Infrastructure

**Feature**: 004-backend-core-infra | **Date**: 2026-07-07

No database schema changes in this feature. The "entities" are in-process
runtime structures and wire shapes. Types live in the crates noted; module
crates consume them only through public APIs (constitution Principle I).

## AppConfig (`shared/config`)

Validated at startup from environment variables; invalid/missing required
values abort startup with a descriptive error (FR-002). Secret-bearing fields
must never be logged (Debug impls redact or skip them).

| Field | Env var | Type | Required | Default | Validation |
|---|---|---|---|---|---|
| `database_url` | `DATABASE_URL` | String (secret) | yes | — | non-empty |
| `redis_url` | `REDIS_URL` | String (secret) | yes | — | non-empty |
| `port` | `PORT` | u16 | no | `8080` | parses as u16 |
| `environment` | `APP_ENVIRONMENT` | enum `production` \| `staging` \| `development` \| `test` | yes | — | one of the enum values |
| `cors_allowed_origins` | `CORS_ALLOWED_ORIGINS` | Vec\<String\> | yes | — | comma-separated; each parses as a valid origin (`scheme://host[:port]`); empty list allowed only outside `production` |
| `log_format` | `LOG_FORMAT` | enum `json` \| `pretty` | no | `json` if environment is `production`/`staging`, else `pretty` | one of the enum values |
| `db_max_connections` | `DB_MAX_CONNECTIONS` | u32 | no | `10` | ≥ 1 |
| `db_acquire_timeout_ms` | `DB_ACQUIRE_TIMEOUT_MS` | u64 | no | `3000` | ≥ 1 |
| `ready_probe_timeout_ms` | `READY_PROBE_TIMEOUT_MS` | u64 | no | `2000` | ≥ 1 (per-dependency probe ceiling, FR-006) |
| `shutdown_grace_seconds` | `SHUTDOWN_GRACE_SECONDS` | u64 | no | `10` | ≥ 0 (FR-016) |

**Lifecycle**: constructed once in `main`, wrapped in `AppState`, immutable
thereafter.

## AppState (`server/src/state.rs`)

The single shared-resource container handed to every handler
(`axum::extract::State<AppState>`); cheap to clone (inner `Arc`s).

| Field | Type | Notes |
|---|---|---|
| `config` | `Arc<AppConfig>` | immutable runtime configuration |
| `db` | `sqlx::PgPool` | built lazily (`connect_lazy`) — construction never dials Postgres (FR-008a) |
| `cache` | `cache::Cache` (wraps `redis::Client` + lazily-initialized shared `ConnectionManager`) | construction never dials Redis (FR-008a) |
| `health_checks` | `Vec<Arc<dyn HealthCheck>>` | registry consumed by `/ready`; seeded with the Postgres and Redis checks; extension point for future modules |

## HealthCheck trait (`shared/observability`)

| Member | Signature | Semantics |
|---|---|---|
| `name` | `fn name(&self) -> &'static str` | stable dependency key: `"database"`, `"cache"` |
| `check` | `async fn check(&self) -> Result<(), String>` | `Ok` = healthy; `Err(msg)` = unhealthy; `msg` must be safe to expose (no connection strings/credentials — FR-011) |

The `/ready` handler wraps each `check()` in
`timeout(config.ready_probe_timeout_ms)`; a timeout counts as unhealthy with
error `"timed out"` (FR-006).

## RequestContext (`shared/observability`)

Established by middleware for every request; carried as a request extension +
tracing span (FR-012, FR-013, FR-014).

| Field | Type | Rules |
|---|---|---|
| `request_id` | `String` | format `req_<uuid>` (UUIDv7 hyphenated lowercase, total length 40). Inbound `X-Request-Id` kept only if it matches this format; otherwise replaced. Always echoed on the response `X-Request-Id` header. |
| `span` | `tracing::Span` | fields: `request_id`, `method`, `path`; child spans attach here; completion event adds `status`, `latency_ms` |

**State transitions**: created at request ingress → propagated through
handlers/logs → stamped on response (header + `error.request_id` when the
response is an envelope) → dropped.

## ErrorEnvelope (`shared/kernel`) — wire shape

Already scaffolded; extended per FR-009/FR-010. Serialized shape (all non-2xx
except failing `/ready`):

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Human-readable, safe to display.",
    "details": [{ "field": "email", "code": "invalid_format", "message": "..." }],
    "request_id": "req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f"
  }
}
```

| Field | Type | Rules |
|---|---|---|
| `error.code` | String | stable machine code from the v1 status map (see `contracts/core-http.md`) |
| `error.message` | String | never contains stack traces, queries, connection strings, or secrets (FR-011) |
| `error.details` | Array, may be empty/omitted | field-level entries for validation errors |
| `error.request_id` | String | the RequestContext id; stamped by the boundary layer, not by handlers |

`ApiError` constructors cover: 400 `validation_failed`, 401 `unauthenticated`,
403 `unauthorized`, 404 `not_found`, 409 `conflict`, 422 `unprocessable`,
429 `rate_limited`, 500 `internal_error`.

## HealthReport (`shared/observability`) — wire shape

Body for `/ready` in **both** outcomes (200 ready / 503 not ready — the sole
envelope exception, clarification Q2). `/health` returns the minimal liveness
shape.

```json
{
  "status": "not_ready",
  "checks": [
    { "name": "database", "status": "ok" },
    { "name": "cache", "status": "error", "error": "timed out" }
  ]
}
```

| Field | Type | Rules |
|---|---|---|
| `status` | `"ready"` \| `"not_ready"` | `ready` iff every check is `ok` |
| `checks[].name` | String | from `HealthCheck::name` |
| `checks[].status` | `"ok"` \| `"error"` | — |
| `checks[].error` | String, present only on error | safe message (FR-011) |

`/health` body: `{ "status": "ok" }` — no dependency consultation (FR-004).

## Relationships

```text
AppConfig ──held by──► AppState ──injected into──► handlers & middleware
AppState.health_checks ◄──implemented by── db::PgHealthCheck, cache::RedisHealthCheck
RequestContext ──stamps──► ErrorEnvelope.error.request_id & X-Request-Id header
HealthCheck results ──rendered as──► HealthReport (by /ready handler)
```
