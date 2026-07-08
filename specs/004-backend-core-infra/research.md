# Research: Backend Core Infrastructure

**Feature**: 004-backend-core-infra | **Date**: 2026-07-07

All Technical Context unknowns resolved below. No external services or novel
technology are involved; decisions select among mature ecosystem options and
are constrained by the constitution's fixed stack (Rust, Axum, Tokio, SQLx,
PostgreSQL, Redis, Tracing) and by the workspace versions already pinned in
`backend/Cargo.toml` (axum 0.8, sqlx 0.8, tokio 1, tracing-subscriber 0.3).

## R1. Redis client crate

- **Decision**: `redis` crate (redis-rs) with `tokio-comp` and
  `connection-manager` features, wrapped in a new `shared/cache` crate. Hold a
  `redis::Client` in `AppState` (cheap, performs no I/O on construction) and
  lazily initialize a shared `ConnectionManager` on first use; readiness
  probes issue `PING` through it with a timeout.
- **Rationale**: `ConnectionManager` reconnects automatically with backoff,
  which directly implements the FR-008a clarification (boot unready, recover
  without restart). `Client::open` never touches the network, so startup can
  never block on Redis. redis-rs is the de-facto standard crate and the
  smallest dependency that meets the need.
- **Alternatives considered**: `deadpool-redis` (adds a pool abstraction we do
  not need yet — sessions/rate-limiting can adopt it later without breaking
  the `shared/cache` seam); `fred` (feature-rich, heavier, different API
  surface; overkill for connectivity + probes).

## R2. Postgres pool initialization that cannot block startup

- **Decision**: `sqlx::postgres::PgPoolOptions` with `connect_lazy_with` /
  `connect_lazy`, `max_connections` and `acquire_timeout` from `AppConfig`.
  Readiness probe executes `SELECT 1` wrapped in `tokio::time::timeout`.
- **Rationale**: A lazy pool is constructed synchronously and only dials
  Postgres when a connection is first acquired — startup succeeds with the
  database down (FR-008a) and the pool self-heals as soon as Postgres is
  reachable, with no custom retry loop to maintain.
- **Alternatives considered**: eager `connect()` + retry loop (custom state
  machine, contradicts the accepted "start unready" clarification); eager
  connect with process exit (rejected by clarification Q1).

## R3. Readiness probe design (testability)

- **Decision**: A `HealthCheck` trait in `shared/observability`
  (`name() -> &str`, `async check() -> Result<(), String>`); `shared/db`
  implements it for the pool, `shared/cache` for Redis. The `/ready` handler
  iterates a registry (`Vec<Arc<dyn HealthCheck>>`) held in `AppState`,
  applying the configured per-probe timeout, and renders a `HealthReport`.
- **Rationale**: Handlers depend on the trait, not on live services, so
  FR-018's readiness tests (healthy / db-down / redis-down) run against mock
  checks with no infrastructure. Later modules can register additional checks
  without touching the handler (extension point per constitution Principle I).
- **Alternatives considered**: handlers calling `PgPool`/`ConnectionManager`
  directly (untestable without live deps; couples observability crate to sqlx
  and redis); a health-check library crate (unnecessary dependency for two
  probes).

## R4. Failing `/ready` response shape

- **Decision**: HTTP 503 with the same `HealthReport` JSON body as success
  (`status`: `"ready" | "not_ready"`, `checks[]` with per-dependency
  `name`/`status`/optional `error`), per spec clarification Q2. Documented in
  `contracts/core-http.md` as the sole exception to the error envelope.
- **Rationale**: locked by clarification; single body shape for both outcomes
  keeps orchestrator/monitor parsing trivial.
- **Alternatives considered**: error envelope with details (rejected in
  clarification Q2).

## R5. Request-ID generation and validation

- **Decision**: Format `req_<UUIDv7>` (hyphenated lowercase UUID), e.g.
  `req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f`. Generation:
  `format!("req_{}", Uuid::now_v7())` — already what the scaffolding does.
  Validation of inbound `X-Request-Id`: exact `req_` prefix + suffix parses as
  a UUID (`Uuid::parse_str`), total length 40; anything else is replaced.
  Constant + validator live in `shared/observability::request_id` so tests
  and future crates share one definition.
- **Rationale**: UUIDv7 is time-sortable (clarification Q3 requires a
  time-sortable ID and the contract example `req_01J...` shows a sortable
  identifier), collision-safe, and already a workspace dependency (`uuid`
  with `v7` feature). Parsing with the `uuid` crate avoids a hand-rolled
  regex and a new dependency.
- **Alternatives considered**: ULID via the `ulid` crate (matches the
  contract example's Crockford-base32 look exactly, but adds a dependency for
  cosmetic fidelity — the contract specifies IDs are *opaque strings*);
  accepting any bounded opaque string (rejected in clarification Q3).

## R6. CORS

- **Decision**: `tower-http` `CorsLayer` (add `tower-http` with `cors` +
  `trace` features to the workspace). Origins from
  `AppConfig.cors_allowed_origins` (comma-separated env var, parsed to a
  list); allow methods GET/POST/PATCH/PUT/DELETE/OPTIONS; allow headers
  `content-type`, `authorization`, `x-request-id`, `idempotency-key`; expose
  `x-request-id`; no wildcard in deployed environments.
- **Rationale**: tower-http is the canonical Axum middleware collection; an
  explicit allowlist satisfies FR-015 and keeps credentialed requests safe.
- **Alternatives considered**: hand-rolled CORS middleware (error-prone
  preflight semantics); `Any` origin (violates FR-015).

## R7. Tracing middleware & log schema

- **Decision**: Custom middleware in `shared/observability::trace` that (a)
  creates a per-request `tracing::info_span` carrying `request_id`, `method`,
  `path`, entered for the whole handler chain, and (b) on completion emits one
  summary event with `status` and `latency_ms`. Runs inside the request-ID
  middleware so the span always sees the final ID.
- **Rationale**: FR-013/FR-014 require the request ID on *every* log record —
  a span field gives that for free with the `tracing` fmt layers; a custom
  ~30-line middleware gives exact control of field names vs. wrestling
  tower-http `TraceLayer` defaults into the required schema.
- **Alternatives considered**: `tower_http::trace::TraceLayer` with custom
  `MakeSpan`/`OnResponse` (workable but more configuration surface than code);
  OpenTelemetry SDK export (explicitly out of scope per spec assumptions —
  span structure is designed so an OTel layer can be added later without
  rework).

## R8. Log output format switching

- **Decision**: `AppConfig.log_format: LogFormat` (`json` | `pretty`) from
  `LOG_FORMAT` env var; default `json` when `APP_ENVIRONMENT` is
  `production`/`staging`, `pretty` otherwise. `init_observability(&config)`
  builds the subscriber accordingly (same `EnvFilter` behavior as today via
  `RUST_LOG`).
- **Rationale**: locked by clarification Q4; both formats come from
  `tracing-subscriber` already in the workspace.
- **Alternatives considered**: JSON always (rejected in clarification Q4).

## R9. Graceful shutdown

- **Decision**: `axum::serve(...).with_graceful_shutdown(signal_future)`
  where the future resolves on Ctrl-C (all platforms) or SIGTERM (unix cfg).
  A configurable grace period (`SHUTDOWN_GRACE_SECONDS`, default 10) bounds
  the wait via `tokio::time::timeout` before the process exits.
- **Rationale**: FR-016; `with_graceful_shutdown` is Axum's supported
  mechanism — it stops accepting connections and drains in-flight requests.
- **Alternatives considered**: abrupt exit (fails FR-016); tower
  `GracefulShutdown` plumbing by hand (needless with Axum's built-in).

## R10. Error handling architecture

- **Decision**: Extend the existing `kernel::ApiError` with constructors for
  the full v1 status map (validation/400, unauthenticated/401,
  unauthorized/403, not_found/404, conflict/409, unprocessable/422,
  rate_limited/429, internal/500) plus `details[]` support. Request IDs are
  injected by a response-mapping layer (middleware reads the ID it set and
  stamps `error.request_id` + header) so handlers never thread IDs manually.
  A `catch_panic`-style layer (tower-http or `std::panic::AssertUnwindSafe`
  wrapper) converts panics into a 500 envelope without crashing the process;
  the full panic detail is logged server-side under the request ID (FR-011).
  Router `.fallback()` returns the 404 envelope (already scaffolded).
- **Rationale**: keeps one envelope type (Principle V), centralizes the
  "never leak internals" rule at the boundary, and makes FR-018's
  internal-failure test deterministic.
- **Alternatives considered**: per-handler request-ID threading (already
  half-present in the fallback; repetitive and easy to forget); `anyhow`
  erasure at the edge (fine later for module errors, not needed for this
  feature's surface).

## R11. Test strategy

- **Decision**: Three tiers, no live infrastructure required by default:
  1. **Unit** (in each crate): config parsing/validation matrix, request-ID
     validation, `ApiError` serialization, `HealthReport` rendering.
  2. **Router** (`server` crate, `tower::ServiceExt::oneshot`): full app
     router with mock `HealthCheck`s and a lazy (never-connected) pool —
     covers FR-018's endpoint/envelope/header/CORS cases including
     ready-degraded permutations.
  3. **Live integration** (ignored by default / env-gated on
     `TEST_DATABASE_URL` + `TEST_REDIS_URL`): real `SELECT 1` and `PING`
     probes succeed against provisioned services.
- **Rationale**: FR-018 demands specific behaviors that must run in CI
  without Docker; the trait seam from R3 makes that possible; live tests
  still exist to catch integration drift when infrastructure is available.
- **Alternatives considered**: `testcontainers` (Docker unavailable on the
  current dev machine and adds CI weight now for little marginal coverage;
  can be adopted when real data-layer features land).

## R12. Route placement for operational endpoints

- **Decision**: `/health`, `/ready` (and the existing `/metrics` stub) at the
  server root, not under `/api/v1`; the `/api/v1` nest remains for business
  endpoints (currently empty except the fallback).
- **Rationale**: the spec's user input names `GET /health` / `GET /ready`
  explicitly; root-level probes are the orchestrator convention and are not
  part of the versioned public API contract (they may not change across
  `/api/v2`).
- **Alternatives considered**: keeping them under `/api/v1` as currently
  scaffolded (couples operational probes to API versioning; contradicts the
  spec's explicit paths).
