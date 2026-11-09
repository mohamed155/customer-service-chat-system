# Configuration Reference

Every environment variable the platform reads. Variables in the Required and
Optional sections are sourced from `backend/crates/shared/config/src/lib.rs`
(`AppConfig::from_env()`). Infrastructure and test-harness variables in later
sections are consumed by Docker Compose, deploy scripts, and test code
respectively — not by the Rust backend at runtime.

---

## Conventions

- **Required** means the application **exits with an error** if the variable is
  missing. The only exception is the `test` environment, where
  `APP_AI_KEY_ENCRYPTION_KEY` may be omitted.
- **Secret** values are redacted from `Debug` output.
- **Generation** commands are given for secret values.
- All variables are read at process startup; none are hot-reloaded.

---

## Required (always)

| Variable | Purpose | Required | Default | Secret |
|----------|---------|----------|---------|--------|
| `DATABASE_URL` | PostgreSQL connection string the server uses. Must be the `app_runtime` role, never a superuser or the schema owner (see [Database roles](#database-roles)). Example: `postgres://app_runtime:pass@host:5432/db` | all environments | — | yes |
| `REDIS_URL` | Redis connection string (e.g. `redis://host:6379`) | all environments | — | yes |
| `APP_ENVIRONMENT` | Runtime environment: `production`, `staging`, `development`, or `test` | all environments | — | no |
| `CORS_ALLOWED_ORIGINS` | Comma-separated list of allowed CORS origins. Each must start with `http://` or `https://`. In production at least one origin is required. | all environments | — | no |
| `AUTH_JWT_SECRET` | HMAC secret for JWT signing and verification. Must be **at least 32 bytes**. | all environments | — | yes |

### Database roles

The backend uses two PostgreSQL roles (ADR-004 §3), created by `infra/postgres/init/02-roles.sh`:

| Variable | Role | Read by | Secret |
|----------|------|---------|--------|
| `DATABASE_URL` | `app_runtime`: reads and writes rows only. No DDL, no `TRUNCATE`, owns nothing, `NOBYPASSRLS`, so Row-Level Security applies to it. | the server | yes |
| `MIGRATION_DATABASE_URL` | `app_owner`: owns the schema and every table. | `server migrate` only (also test setup) | yes |

The server never runs migrations on startup. Run them as a separate step, before starting a new
release: `cargo run -p server -- migrate` locally (or `sqlx migrate run --database-url
"$MIGRATION_DATABASE_URL"`), `docker compose … run --rm backend migrate` on a host (`ops/deploy.sh`
does it). `migrate` reads only `MIGRATION_DATABASE_URL` and refuses to fall back to `DATABASE_URL`.

**Generate AUTH_JWT_SECRET:**

```bash
openssl rand -base64 32
```

---

## Required (non-test only)

| Variable | Purpose | Required | Default | Secret |
|----------|---------|----------|---------|--------|
| `APP_AI_KEY_ENCRYPTION_KEY` | Base64-encoded 32-byte AES-256-GCM key for encrypting AI provider API keys at rest. | `production`, `staging`, `development`; optional in `test` | — | yes |

**Generate APP_AI_KEY_ENCRYPTION_KEY:**

```bash
openssl rand -base64 32
```

---

## Optional (with defaults)

| Variable | Purpose | Default | Secret |
|----------|---------|---------|--------|
| `PORT` | TCP port the HTTP server binds to | `8080` | no |
| `BIND_ADDRESS` | IP address to bind on | `0.0.0.0` | no |
| `LOG_FORMAT` | Log output format: `json` or `pretty`. In `production`/`staging` environments the default is `json`; in `development`/`test` it is `pretty`. | environment-dependent | no |
| `DB_MAX_CONNECTIONS` | Maximum database connection pool size | `10` | no |
| `DB_ACQUIRE_TIMEOUT_MS` | Milliseconds to wait for a database connection from the pool before timing out | `3000` | no |
| `READY_PROBE_TIMEOUT_MS` | Milliseconds to wait for each dependency during the `/ready` probe | `2000` | no |
| `SHUTDOWN_GRACE_SECONDS` | Seconds to wait for in-flight requests to complete on shutdown | `10` | no |
| `AUTH_SESSION_TTL_SECONDS` | JWT session expiry in seconds | `28800` (8 hours) | no |
| `SMTP_URL` | SMTP connection string (e.g. `smtp://user:pass@smtp.example.com:587`). Unset disables email sending. | — | yes |
| `SMTP_FROM` | `From` address for outgoing emails. | — | no |
| `PUBLIC_DASHBOARD_URL` | Public-facing dashboard URL used in emails and redirects. Must be `http://` or `https://`. Default is `https://dashboard.example.com` in `production`/`staging` and `http://localhost:4200` in `development`/`test`. | environment-dependent | no |
| `APP_DOCS_ENABLED` | Serve Swagger UI (`/swagger-ui`) and OpenAPI spec (`/api-docs/openapi.json`). In non-production environments docs are **always served** regardless of this flag. In production the flag must be explicitly set to `true`. Accepts `1`, `true`, `yes`, `on`. | `false` | no |
| `APP_RATE_LIMIT_ENABLED` | Enable request rate limiting globally. Accepts boolean-like values. | `true` | no |
| `APP_RATE_LIMIT_LOGIN_MAX` | Maximum failed login attempts per window (per email+IP pair) | `10` | no |
| `APP_RATE_LIMIT_LOGIN_WINDOW_SECS` | Login rate-limit window in seconds | `60` | no |
| `APP_RATE_LIMIT_MAX_OVERRIDE` | Maximum override value allowed in `tenant_rate_limit_overrides`. Rejects writes above this value. | `10000` | no |
| `APP_RATE_LIMIT_FAIL_MODE` | Behaviour when Redis is unreachable for rate-limiting: `auth_closed` (refuse login attempts) or `open` (fall through). | `auth_closed` | no |
| `APP_SECURITY_HEADERS_ENABLED` | Enable security response headers (`X-Content-Type-Options`, `Referrer-Policy`, `Strict-Transport-Security`, `X-Frame-Options`, CSP). | `true` | no |
| `APP_CSP_POLICY` | Content Security Policy value. Default allows `'self'` with `frame-ancestors 'none'`. | `default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'` | no |
| `APP_CSP_REPORT_ONLY` | Emit CSP via `Content-Security-Policy-Report-Only` instead of `Content-Security-Policy`. Set to `false` after soak. | `true` | no |
| `APP_HSTS_MAX_AGE` | `max-age` value for `Strict-Transport-Security`. Set to `0` for local development (prevents browser pinning localhost to HTTPS). Default seconds equals 1 year. | `31536000` | no |
| `APP_TRACE_SAMPLE_RATIO` | Read by the backend but **no longer applied at the SDK** — every span is now exported so that FR-020's "an errored request's trace is always retained" can hold. The retention rate for non-errored traces is set by `OTEL_TAIL_SAMPLE_PERCENTAGE` on the collector. | `1.0` | no |
| `APP_TRUSTED_PROXY` | Must be `true` in staging/production (traffic arrives through Caddy which overwrites `X-Forwarded-For`). Must stay `false` in local development (nothing sanitises the header). Defaults to `false`. | `false` | no |

---

## Optional (no default)

| Variable | Purpose | Secret |
|----------|---------|--------|
| `APP_AI_OPENAI_BASE_URL` | Override base URL for OpenAI API calls. Useful for proxies or mocks. | no |
| `APP_AI_ANTHROPIC_BASE_URL` | Override base URL for Anthropic API calls. | no |
| `APP_AI_GEMINI_BASE_URL` | Override base URL for Google Gemini API calls. | no |
| `APP_INTEGRATION_SECRETS_KEY` | Base64-encoded 32-byte AES-256-GCM key for encrypting integration connection secrets at rest. If set must decode to exactly 32 bytes. | yes |
| `APP_OTLP_ENDPOINT` | OTLP/gRPC endpoint for exporting traces (e.g. `http://otel-collector:4317`). Unset disables trace export. | no |
| `APP_SENTRY_DSN` | Sentry DSN for error reporting. Unset disables Sentry capture. | yes |
| `APP_RELEASE_SHA` | Git SHA of the deployed release. Attached to Sentry events and trace metadata. | no |

**Generate APP_INTEGRATION_SECRETS_KEY:**

```bash
head -c 32 /dev/urandom | base64
```

---

## S3 configuration (all-or-none)

These variables must be set **together** or left **all unset**. A partial set
causes a startup error. When all are unset the server uses ephemeral in-memory
storage.

| Variable | Purpose | Default | Secret |
|----------|---------|---------|--------|
| `APP_S3_ENDPOINT` | S3-compatible endpoint URL (e.g. `http://localhost:9000` for MinIO) | — | no |
| `APP_S3_REGION` | S3 region (e.g. `us-east-1`) | — | no |
| `APP_S3_BUCKET` | Bucket name. The bucket must exist before first upload; the application does not create it. | — | no |
| `APP_S3_ACCESS_KEY_ID` | S3 access key | — | yes |
| `APP_S3_SECRET_ACCESS_KEY` | S3 secret key | — | yes |
| `APP_S3_FORCE_PATH_STYLE` | Force path-style addressing (required for MinIO). | `true` | no |

---

---

## Local development stack (not read by the backend)

Read by `infra/docker-compose.yml` and the scripts mounted into its containers. Defaults are
the dev values in `.env.example`. `just up` copies it to `.env` on first run and starts the stack with `--env-file .env`, so edit `.env`; `.env.example` documents every variable.

| Variable | Purpose | Default | Secret |
|----------|---------|---------|--------|
| `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD` | Database and bootstrap superuser, read only when the Postgres volume is first created. | `customer_service`, `customer_service`, `customer_service_dev` | yes (password) |
| `APP_OWNER_PASSWORD` | Password of the `app_owner` role (owns the schema, runs migrations; `MIGRATION_DATABASE_URL`). Read on first volume creation by `infra/postgres/init/02-roles.sh`. When this or `APP_RUNTIME_PASSWORD` is empty, neither role is created. | `app_owner_dev` in `.env.example` | yes |
| `APP_RUNTIME_PASSWORD` | Password of the `app_runtime` role the backend connects as (`DATABASE_URL`): DML only, `NOBYPASSRLS`, owns nothing. | `app_runtime_dev` in `.env.example` | yes |
| `MINIO_ROOT_USER`, `MINIO_ROOT_PASSWORD` | MinIO root credentials, also used by the bucket bootstrap. | `minioadmin` | yes (password) |
| `POSTGRES_PORT`, `REDIS_PORT`, `MINIO_PORT`, `MINIO_CONSOLE_PORT`, `MAILPIT_SMTP_PORT`, `MAILPIT_UI_PORT` | Host ports (bound to `127.0.0.1`). Override when a default is taken. | `5432`, `6379`, `9000`, `9001`, `1025`, `8025` | no |

Avoid the dollar sign in these values: Compose expands it.

## Deployment-time variables (not read by the backend)

These variables are consumed by **infrastructure tooling** (Docker Compose,
Caddy, deploy scripts, Alertmanager) — not by the Rust backend.

| Variable | Purpose | Default | Where used |
|----------|---------|---------|------------|
| `APP_ALERT_WEBHOOK_URL` | Webhook URL Alertmanager sends alert notifications to. Substituted into `infra/alertmanager.yml.tmpl` by `sed` in `infra/alertmanager-entrypoint.sh` at container start (busybox is minimal and has no template-string substitution tool). **The container refuses to start if this is unset**, so a missing destination fails loudly rather than silently dropping every alert. | — | Alertmanager (`infra/alertmanager.yml.tmpl`) |
| `APP_DOMAIN` | Domain name used by Caddy for TLS and routing. Required in `infra/docker-compose.staging.yml`. | — | Caddy (`infra/Caddyfile`, `infra/docker-compose.staging.yml`) |
| `IMAGE_BASE` | Container image repository prefix. Read by deploy and rollback scripts. | `ghcr.io/mohamed155/customer-service-chat-system` | `ops/deploy.sh`, `ops/rollback.sh`, `infra/docker-compose.staging.yml` |
| `K6_BASE_URL` | Base URL the k6 load-test scripts target. **No default — scripts exit 2 if unset** (prevents accidentally pointing at production). Used by `loadtest/dashboard-read.js`, `loadtest/widget-conversation.js`, `loadtest/webhook-ingest.js`. | — | k6 load-test scripts |
| `LOADTEST_ALLOWED_HOSTS` | Comma-separated allowlist of hosts the load-test scripts may target. A `K6_BASE_URL` host outside it aborts the run. Used by the same three scripts. | — | k6 load-test scripts |
| `NODE_EXPORTER_TEXTFILE_DIR` | Directory where `ops/backup.sh` writes the Prometheus textfile collector metric. Must exist on the host. | `/var/lib/node_exporter/textfile_collector` | `ops/backup.sh` |
| `OTEL_TAIL_SAMPLE_PERCENTAGE` | Percentage (0–100) of non-errored traces the collector retains; errored traces are always retained regardless of this value. | `100` | OpenTelemetry collector (`infra/otel-collector-config.yaml` via `infra/docker-compose.yml`) |

---

## Test-only note

In the `test` environment `APP_AI_KEY_ENCRYPTION_KEY` may be omitted — the
encryption path is effectively disabled for test helpers. All other validation
rules (base64 decoding, 32-byte length) apply when the variable is set.

---

## Test-Harness Variables (not read by the running platform)

These variables are consumed **only by test code** — the running platform never
reads them. They control which test suites run and how failures are reported.

| Variable | Purpose | Default |
|----------|---------|---------|
| `REQUIRE_DB_TESTS` | Set to `1` to make database- and Redis-backed tests **fail** instead of silently skipping when the dependency is unreachable. **Required for any run whose result you intend to trust**; the CI gate sets it. | unset (tests self-skip) |
| `MIGRATION_DATABASE_URL` | Test setup that needs the schema owner (applying migrations, `TRUNCATE` between tests) uses it, through `db::testing`; the code under test uses `DATABASE_URL` (`app_runtime`). Required when `REQUIRE_DB_TESTS=1`; otherwise falls back to `DATABASE_URL`. | unset |
| `TEST_DATABASE_URL` | PostgreSQL connection string for live-dep tests. Read by `crates/server/tests/live_deps.rs`. | unset (suite skips) |
| `TEST_REDIS_URL` | Redis connection string for live-dep tests. Read by `crates/server/tests/live_deps.rs`. | unset (suite skips) |
| `LIVE_AI_OPENAI_KEY` | API key to opt into live-provider tests in `crates/server/tests/ai.rs`. Never commit a real key. | unset (suite skips) |
| `LIVE_AI_OPENAI_MODEL` | Model name for live-provider tests (e.g. `gpt-4o`). Used alongside `LIVE_AI_OPENAI_KEY`. | unset (suite skips) |
