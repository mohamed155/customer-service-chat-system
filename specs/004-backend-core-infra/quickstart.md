# Quickstart: Backend Core Infrastructure

**Feature**: 004-backend-core-infra

Validation guide proving the feature end-to-end. Shapes and rules referenced
here are defined in [data-model.md](./data-model.md) and
[contracts/core-http.md](./contracts/core-http.md).

## Prerequisites

1. **Rust toolchain** (not currently installed on the dev machine):
   install from <https://rustup.rs> (`rustup default stable`). Verify:
   `cargo --version`.
2. **PostgreSQL 16+ with pgvector** and **Redis 7+** reachable locally.
   With Docker:

   ```powershell
   docker run -d --name csp-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 pgvector/pgvector:pg16
   docker run -d --name csp-redis -p 6379:6379 redis:7
   ```

   (Docker is also not installed on this machine — install Docker Desktop or
   provision native services.)

## Configuration

Required environment (see data-model.md for the full matrix and defaults):

```powershell
$env:DATABASE_URL = "postgres://postgres:postgres@localhost:5432/postgres"
$env:REDIS_URL = "redis://localhost:6379"
$env:APP_ENVIRONMENT = "development"
$env:CORS_ALLOWED_ORIGINS = "http://localhost:4200"
```

## Run

```powershell
cd backend
cargo run -p server
```

Expected: startup logs (pretty format in development) showing the bind
address; the process starts even if Postgres/Redis are down (FR-008a).

## Validation scenarios

### 1. Liveness & readiness (User Story 1)

```powershell
curl.exe -i http://localhost:8080/health   # 200 {"status":"ok"}
curl.exe -i http://localhost:8080/ready    # 200 {"status":"ready","checks":[...]} both checks ok
docker stop csp-redis
curl.exe -i http://localhost:8080/ready    # 503 {"status":"not_ready",...} cache=error, database=ok
curl.exe -i http://localhost:8080/health   # still 200
docker start csp-redis                     # readiness recovers without restart
```

Startup validation: unset `DATABASE_URL` and `cargo run -p server` — process
exits promptly with a descriptive config error (and no secret values printed).

### 2. Error envelope (User Story 2)

```powershell
curl.exe -i http://localhost:8080/nope
# 404, body = error envelope: code=not_found, message, request_id=req_...
```

Every non-2xx body matches the envelope in contracts/core-http.md; 500 bodies
contain no internal detail.

### 3. Request traceability (User Story 3)

```powershell
curl.exe -i http://localhost:8080/health                                  # response has X-Request-Id: req_<uuid>
curl.exe -i -H "X-Request-Id: req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f" http://localhost:8080/health
#   → same ID echoed back
curl.exe -i -H "X-Request-Id: <script>" http://localhost:8080/health      # → replaced with a fresh req_ ID
```

Server logs for each request carry the same `request_id`, plus a completion
record with method/path/status/latency.

### 4. CORS (User Story 4)

```powershell
curl.exe -i -X OPTIONS http://localhost:8080/api/v1/anything `
  -H "Origin: http://localhost:4200" -H "Access-Control-Request-Method: POST"
#   → Access-Control-Allow-Origin: http://localhost:4200
curl.exe -i -X OPTIONS http://localhost:8080/api/v1/anything `
  -H "Origin: http://evil.example" -H "Access-Control-Request-Method: POST"
#   → no Access-Control-Allow-Origin header
```

### 5. Graceful shutdown

Send Ctrl-C to the running server: logs show shutdown initiation; in-flight
requests complete within `SHUTDOWN_GRACE_SECONDS` (default 10); process exits.

## Test suite

```powershell
cd backend
cargo test --workspace          # unit + router tests; no live services required
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Live-dependency integration tests (optional) run only when both
`TEST_DATABASE_URL` and `TEST_REDIS_URL` are set.

## Done when

- All five scenarios behave as shown.
- `cargo test --workspace` passes with no live services running.
- FR-018's enumerated cases each have a passing automated test.
