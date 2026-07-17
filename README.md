# AI Customer Service Platform

## Quickstart

Prerequisites: Rust stable, sqlx-cli, Node/pnpm, and Docker Compose.

```bash
docker compose -f infra/docker-compose.yml up -d
cd backend && sqlx migrate run && cargo run -p server
cd frontend && pnpm install --frozen-lockfile && pnpm exec ng serve dashboard
# In another terminal, to serve the widget:
cd frontend && pnpm exec ng serve widget
```

The API listens on http://localhost:8080 (`GET /api/v1/health`), the dashboard on port 4200, Mailhog on port 8025, and MinIO's console on port 9001. Copy `.env.example` to `.env` and export its values before starting the backend.

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
