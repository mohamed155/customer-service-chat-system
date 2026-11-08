# AI Customer Service Platform

## Quickstart

This guide takes you from a clean machine to a running backend, dashboard, and
widget with seed data.

### Prerequisites

- **Rust** — Install via <https://rustup.rs> (stable, minimum 1.84).
- **sqlx-cli** — `cargo install sqlx-cli --locked`
- **Node.js** — v22 or later (use <https://nodejs.org> or `nvm`).
- **pnpm** — `corepack enable && corepack prepare pnpm@10.34.4 --activate`
- **Docker Desktop** (or Docker Compose standalone) — for PostgreSQL, Redis,
  Mailpit, and MinIO.

### 1. Start infrastructure

```bash
docker compose -f infra/docker-compose.yml up -d --wait
```

This starts PostgreSQL with pgvector (port 5432), Redis (6379), Mailpit (1025
SMTP, 8025 web), and MinIO (9000 S3, 9001 console), and returns once every
service reports healthy. The `tenants` and `quarantine` buckets are created for
you. Ports are bound to `127.0.0.1`; if one is taken, set `POSTGRES_PORT`,
`REDIS_PORT`, `MINIO_PORT`, `MINIO_CONSOLE_PORT`, `MAILPIT_SMTP_PORT` or
`MAILPIT_UI_PORT` (in your shell, or in `.env` with `--env-file .env` after
step 2). Data lives in named volumes, so it survives
`docker compose down`; add `-v` to wipe it.

### 2. Configure environment

```bash
cp .env.example .env
```

Edit `.env` and fill in both encryption keys:

```bash
# Generate suitable values:
openssl rand -base64 32
# Paste the output into .env as:
# APP_AI_KEY_ENCRYPTION_KEY=<paste here>
# Also generate the integration secrets key:
head -c 32 /dev/urandom | base64
# Paste as:
# APP_INTEGRATION_SECRETS_KEY=<paste here>
```

Then export the variables:

```bash
set -a && source .env && set +a
```

### 3. Run database migrations

```bash
cd backend && sqlx migrate run
```

### 4. Seed the database

```bash
cargo run -p server --bin seed
```

If no seed binary exists, the server auto-seeds on first start with default
data (admin user, sample tenant, Free plan).

### 5. Start the backend

```bash
cargo run -p server
```

The API listens on `http://localhost:8080`. Verify:

```bash
curl http://localhost:8080/api/v1/health
```

### 6. Start the dashboard

In a new terminal:

```bash
cd frontend && pnpm install --frozen-lockfile && pnpm exec ng serve dashboard
```

The dashboard is at `http://localhost:4200`.

### 7. Start the widget

In another terminal:

```bash
cd frontend && pnpm exec ng serve widget
```

The widget dev server is at `http://localhost:4201`.

### Local development notes

- **Testing**: Set `REQUIRE_DB_TESTS=1` for a trustworthy local test run — see
  [docs/configuration.md](docs/configuration.md) (Test-Harness Variables).
- **HSTS**: Set `APP_HSTS_MAX_AGE=0` in `.env` to prevent your browser from
  pinning `localhost` to HTTPS. A non-zero value is painful to unpin.
- **API docs**: Swagger UI is at `http://localhost:8080/swagger-ui/` in
  non-production environments without needing `APP_DOCS_ENABLED=true`.
- **Mailpit**: Outgoing emails appear at `http://localhost:8025`.
- **MinIO console**: Object storage at `http://localhost:9001` (credentials
  from `.env`).

### What runs where

| Component | URL |
|-----------|-----|
| Backend API | http://localhost:8080 |
| Dashboard | http://localhost:4200 |
| Widget | http://localhost:4201 |
| Mailpit UI | http://localhost:8025 |
| MinIO console | http://localhost:9001 |

## Environment — AI Provider Abstraction

The AI module encrypts provider API keys at rest using AES-256-GCM. The key is
configured via:

```
APP_AI_KEY_ENCRYPTION_KEY=<base64-of-32-bytes>
```

Generate a suitable key with:

```bash
openssl rand -base64 32
```

This variable is **required** in non-test environments (`production`, `staging`,
`development`). In the `test` environment it may be omitted (encryption is
effectively disabled for test helpers).

The integrations module also encrypts per-connection secrets (webhook signing
secrets and per-field user secrets) at rest using AES-256-GCM, via:

```
APP_INTEGRATION_SECRETS_KEY=<base64-of-32-bytes>
```

Generate a suitable key with:

```bash
head -c 32 /dev/urandom | base64
```

This variable is **required** in non-test environments (`production`, `staging`,
`development`) — the integrations crate's `crypto::seal`/`open` path refuses
to run without it. In the `test` environment it may be omitted (encryption is
effectively disabled for test helpers).

Optional per-provider base URL overrides allow routing traffic to proxies or
test servers:

- `APP_AI_OPENAI_BASE_URL`
- `APP_AI_ANTHROPIC_BASE_URL`
- `APP_AI_GEMINI_BASE_URL`

These are unset by default in production; set them to point at a local proxy
or mock in development / CI.

## Environment — Object Storage (Knowledge Base)

The knowledge base feature stores uploaded documents in S3-compatible object
storage. Configure it via these environment variables (all or none):

```
APP_S3_ENDPOINT=http://localhost:9000
APP_S3_REGION=us-east-1
APP_S3_BUCKET=app-dev
APP_S3_ACCESS_KEY_ID=minioadmin
APP_S3_SECRET_ACCESS_KEY=minioadmin
APP_S3_FORCE_PATH_STYLE=true
```

`APP_S3_FORCE_PATH_STYLE` defaults to `true` (required for MinIO). The bucket
named by `APP_S3_BUCKET` must exist before the first upload — the application
does not create it. If no `APP_S3_*` variables are set, the server falls back
to ephemeral in-memory storage (useful for development and CI).
