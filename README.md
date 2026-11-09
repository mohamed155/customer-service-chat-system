# AI Customer Service Platform

A multi-tenant SaaS platform for AI-driven customer service. Businesses (tenants) configure AI
agents, knowledge bases, prompts and integrations, and serve their own customers through an
embeddable web chat widget and messaging channels such as WhatsApp. When the AI can't help, the
conversation is escalated to a human agent. Platform operators manage tenants, billing and system
health. The full product scope is in the
[software requirements specification](specs/001-ai-customer-service-platform/spec.md).

## Architecture

The backend is a Rust modular monolith (one deployable, hexagonal modules). The frontend is an
Angular workspace with a dashboard and a chat widget. PostgreSQL with pgvector, Redis and
S3-compatible storage sit underneath.

- [System architecture](docs/architecture/system-architecture.md): style, modules, tenancy,
  security, scalability and the AI runtime
- [Architecture Decision Records](docs/adr/README.md): the decisions behind it
- [Constitution](.specify/memory/constitution.md): the rules every change follows

## Repository map

| Path | What it holds |
| -- | -- |
| `backend/` | Rust Cargo workspace. `crates/server` is the HTTP binary, `crates/modules/*` the business modules (conversations, knowledge, tenancy, …), `crates/shared/*` shared crates (db, config, kernel, …), `crates/ai-providers` the LLM provider adapters, `migrations/` the SQL migrations |
| `frontend/` | Angular + pnpm workspace. `apps/dashboard` (tenant and platform admin) and `apps/widget` (embeddable chat), shared code in `libs/`, Playwright tests in `e2e/`. Conventions are in [`frontend/CLAUDE.md`](frontend/CLAUDE.md) |
| `infra/` | Docker Compose stack for local development and staging, Dockerfiles, and Prometheus, Grafana and Loki configuration |
| `ops/` | Staging deploy, rollback, backup and restore scripts |
| `loadtest/` | k6 load tests |
| `docs/` | Architecture, ADRs, security notes, configuration, deployment and runbooks |
| `specs/` | One folder per feature (spec, plan, tasks) from the Spec Kit workflow |
| `.specify/` | Spec Kit setup: the constitution, templates and scripts |
| `.github/` | CI workflows, Dependabot, the PR template and CODEOWNERS |
| `justfile` | Recipes for the local services (`just up`, `just down`, …) |

Contributing: see [CONTRIBUTING.md](CONTRIBUTING.md).

## Quickstart

This guide takes you from a clean machine to a running backend, dashboard and widget.

### Prerequisites

- **Rust** — Install `rustup` from <https://rustup.rs>. The first `cargo` command in `backend/`
  installs the pinned toolchain from `backend/rust-toolchain.toml` (currently 1.97.0).
- **sqlx-cli** — `cargo install sqlx-cli --locked`
- **Node.js** — v22 or later (use <https://nodejs.org> or `nvm`).
- **pnpm** — `corepack enable && corepack prepare pnpm@10.34.4 --activate`
- **Docker** with Compose 2.20 or later — Docker Desktop (Windows, macOS) or Docker Engine
  with the Compose plugin (Linux). It runs PostgreSQL, Redis, Mailpit and MinIO.
- **just** — the task runner for the local services: `brew install just`,
  `winget install Casey.Just`, `cargo install just --locked`, or your distro's package
  (<https://github.com/casey/just#installation>).

**Windows:** run the commands from **WSL2** (with Docker Desktop's WSL integration enabled) or
from **Git Bash**. The recipes call `bash`, so PowerShell and cmd do not work.

### 1. Start infrastructure

```bash
just doctor   # optional: checks Docker, Compose, Node, .env and that the host ports are free
just up       # creates .env on first run, starts the services, waits until all are healthy
```

`just up` copies `.env.example` to `.env` the first time (dev-only values, never committed)
and starts PostgreSQL with pgvector (port 5432), Redis (6379), Mailpit (1025 SMTP, 8025 web)
and MinIO (9000 S3, 9001 console). It returns once every service reports healthy. The
`tenants` and `quarantine` buckets are created for you. Ports are bound to `127.0.0.1`.

| Recipe | What it does |
| -- | -- |
| `just up` | Start the services and wait until healthy |
| `just down` | Stop them; data stays in the named volumes |
| `just reset` | Delete **all** local data and start again (asks first) |
| `just logs [service…]` | Follow the logs of everything, or of the services you name |
| `just psql [args…]` | Open psql on the local database, or run `just psql -c "select 1"` |
| `just doctor` | Check Docker, Compose, Node, `.env` and the host ports |
| `just verify-roles` | Prove the database roles cannot bypass Row-Level Security (below) |

The stack runs under its own Compose project name (`customer-service-dev`), so its volumes
are called `customer-service-dev_*` and `just reset` can only ever delete those. Run
`just` with no arguments to list the recipes; `.env.example` documents every variable.

If a port is taken or reserved (`just doctor` says which; Windows reserves ranges that
include 5432 on some machines, and a native PostgreSQL install holds 5432), uncomment the
matching line in `.env` (`POSTGRES_PORT`, `REDIS_PORT`, `MINIO_PORT`, `MINIO_CONSOLE_PORT`,
`MAILPIT_SMTP_PORT`, `MAILPIT_UI_PORT`) and update whichever URL points at that port to match:
`DATABASE_URL`, `REDIS_URL`, or `APP_S3_ENDPOINT` if you set it.
Then run `just up` again. On a first run `.env` doesn't exist yet: either run
`cp .env.example .env` before `just up`, or let `just up` create it (it stops with
`address already in use`), edit it, and run `just up` again. For example, with PostgreSQL on 5433:

```bash
POSTGRES_PORT=5433
DATABASE_URL=postgres://customer_service:customer_service_dev@localhost:5433/customer_service
```

#### Database roles

On first start the Postgres container also creates two non-superuser roles from the
`APP_OWNER_PASSWORD` and `APP_RUNTIME_PASSWORD` values in `.env`:
`app_owner` (owns the schema, runs migrations) and `app_runtime` (read/write rows
only, `NOBYPASSRLS`, owns nothing). Row-Level Security only protects tenants when the
application connects as a role like `app_runtime`; the bootstrap superuser
(`customer_service`) ignores every policy. The backend still connects as that
superuser today and switches to `app_runtime` with the RLS work (MOH-31). If the two
passwords are unset, the roles are skipped and the container logs a notice. Avoid the
dollar sign in `.env` values, because Compose expands it.

`just verify-roles` checks that the split holds: it creates and drops a throwaway table
with forced RLS and tries to get around it as `app_runtime`.

#### Resetting local data

Postgres reads `POSTGRES_USER`, `POSTGRES_PASSWORD` and `POSTGRES_DB` only when its
volume is first created, and the extension setup runs only then too. If you change
those values later, or a first start was interrupted, the `postgres` service stays
unhealthy (its health check logs in with the configured credentials and checks the
extensions). The role passwords are also read only at that point. Recreate the volume:

```bash
just reset   # DELETES all local Postgres, Redis, MinIO and Mailpit data; asks before it does
```

### 2. Configure environment

`.env` already exists from step 1. Edit it and fill in both encryption keys:

```bash
# Generate suitable values:
openssl rand -base64 32
# Paste the output into .env as:
# APP_AI_KEY_ENCRYPTION_KEY=<paste here>
# Also generate the integration secrets key:
head -c 32 /dev/urandom | base64
# Uncomment the line in .env and paste it there:
# APP_INTEGRATION_SECRETS_KEY=<paste here>
```

Then export the variables (from the repository root):

```bash
set -a && source .env && set +a
```

### 3. Run database migrations

```bash
cd backend && sqlx migrate run
```

If you'd rather not install sqlx-cli, `cargo run -p server -- migrate` (also from `backend/`)
applies the same migrations.

### 4. Start the backend

From `backend/`:

```bash
cargo run -p server
```

The first build takes several minutes. The API listens on `http://localhost:8080`, under
`/api/v1`. Verify:

```bash
curl http://localhost:8080/health   # liveness
curl http://localhost:8080/ready    # readiness: database and Redis reachable
```

### 5. Start the dashboard

In a new terminal, from the repository root:

```bash
cd frontend && pnpm install --frozen-lockfile && pnpm exec ng serve dashboard
```

The dashboard is at `http://localhost:4200`.

### 6. Create your first account

The database starts empty: there is no seed data and no default user. Open
<http://localhost:4200/auth/signup> and sign up. This creates a workspace (tenant) with you as
its Owner and signs you in.

Use Chrome or Firefox for this. The session cookie is marked `Secure`, and some Safari versions
don't keep a `Secure` cookie on `http://localhost`, so you'd land back on the sign-in page.

### 7. Start the widget

In another terminal, from the repository root:

```bash
cd frontend && pnpm exec ng serve widget --port 4201
```

The widget dev server is at `http://localhost:4201`. Without `--port` it also tries 4200, which
the dashboard already uses.

This server is for working on the widget's UI. The widget calls the public `/widget/v1` API on
the origin it is served from, and the dev server doesn't forward those calls to the backend, so
on its own it can't load a widget's configuration or start a conversation. To try the widget end
to end, create a widget in the dashboard and embed its snippet, as described in the
[widget quickstart](specs/023-website-chat-widget/quickstart.md).

### Local development notes

- **Testing**: Set `REQUIRE_DB_TESTS=1` for a trustworthy local test run — see
  [docs/configuration.md](docs/configuration.md) (Test-Harness Variables).
- **HSTS**: Set `APP_HSTS_MAX_AGE=0` in `.env` to prevent your browser from
  pinning `localhost` to HTTPS. A non-zero value is painful to unpin.
- **API docs**: Swagger UI is at `http://localhost:8080/swagger-ui/` in
  non-production environments without needing `APP_DOCS_ENABLED=true`.
- **Email**: Unless both `SMTP_URL` and `SMTP_FROM` are set, the backend doesn't send email: it
  writes each message to its log instead. Mailpit (web UI at `http://localhost:8025`, or your
  `MAILPIT_UI_PORT`) catches mail sent to its SMTP port, but the backend's SMTP sender always
  uses TLS (STARTTLS for `smtp://`, implicit TLS for `smtps://`), and Mailpit doesn't offer TLS
  in this setup, so the backend can't deliver to it as configured.
- **MinIO console**: Object storage at `http://localhost:9001` (credentials
  from `.env`).

### What runs where

Infrastructure URLs below use the default ports; use your override if you set one.

| Component | URL |
|-----------|-----|
| Backend API | <http://localhost:8080> |
| Dashboard | <http://localhost:4200> |
| Widget | <http://localhost:4201> |
| Mailpit UI | <http://localhost:8025> |
| MinIO console | <http://localhost:9001> |

## Environment — AI Provider Abstraction

The AI module encrypts provider API keys at rest using AES-256-GCM. The key is
configured via:

```bash
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

```bash
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

```bash
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
