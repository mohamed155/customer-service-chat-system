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
