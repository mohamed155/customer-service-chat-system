# Deployment Guide

## Bootstrap a Fresh Host

1. **Provision the VM** with Docker Engine 24+ and `docker compose` plugin installed.
2. **Clone the repository** and check out the target release tag.
3. **Create `.env.staging`** from `.env.staging.example` with real secrets. The backend uses two
   database roles (ADR-004 §3): set `DATABASE_URL` to the `app_runtime` role and
   `MIGRATION_DATABASE_URL` to the `app_owner` role (see
   [configuration](../configuration.md#database-roles)). Also export `APP_OWNER_PASSWORD` and
   `APP_RUNTIME_PASSWORD` (the same passwords) in the shell, or put them in the Compose `.env`, before
   step 5: the Postgres container creates the roles from them on first start.
4. **Create the node-exporter textfile collector directory:**

   ```bash
   sudo mkdir -p /var/lib/node_exporter/textfile_collector
   ```

5. **Start infrastructure services:**

   ```bash
   docker compose --profile observability -f infra/docker-compose.yml up -d
   ```

6. **Run initial migrations** (as `app_owner`, through `MIGRATION_DATABASE_URL`):

   ```bash
   docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml run --rm backend migrate
   ```

   The server never migrates on startup. `migrate` refuses to run without
   `MIGRATION_DATABASE_URL`.

7. **Start the full stack:**

   ```bash
   APP_RELEASE_SHA=<sha> docker compose --profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml up -d
   ```

8. **Verify:** the `/api/v1/health` and `/ready` endpoints respond, and the dashboard loads at the configured `PUBLIC_DASHBOARD_URL`.

> **Note:** Omitting `--profile observability` from the docker compose commands leaves the platform running with no metrics, dashboards, logs, or alerting.

## Promote a Release

Use `ops/deploy.sh <git-sha>` (see [contract](../../ops/deploy.sh)).

The script runs migrations before touching the running stack. If migrations fail, the old release stays live. If the new stack fails its readiness check within 120 seconds, the old release is restored.

Before migrating, it runs `infra/postgres/init/02-roles.sh` in the Postgres container. The script
is idempotent and makes sure `app_owner`, `app_runtime` and `app_platform` (the login-less owner of
the cross-tenant functions) exist with the right attributes and grants. It fails the deploy if the
container has no `APP_OWNER_PASSWORD` / `APP_RUNTIME_PASSWORD`. Migration 0067 needs `app_platform`,
so a host must run this step before it deploys that release (the script does it every time).

## Run Modes

One image, one binary, four run modes selected with `APP_RUN_MODE` (see
[configuration](../configuration.md)): `api`, `gateway`, `worker`, `all` (the default). The deploy
script and the Compose stack run `all`. Splitting a deployment into separate `api`, `gateway` and
`worker` processes is supported by the code, with two conditions:

- **Don't split yet in production.** Realtime fan-out and agent presence are in-process (ADR-009
  RT1). With a split, an escalation, notification or AI reply produced by a `worker` process
  never reaches an SSE client connected to an `api` or `gateway` process, and each process starts
  with a warning that says so. Move fan-out to Redis first, then split.
- **Run exactly one `worker` per release** until the outbox workers are proven safe to run in
  parallel, and keep the `gateway` and `api` on separate hostnames so dashboard cookies are never
  sent to public endpoints (ADR-010 section 4).

A `gateway` process serves only the widget, integration-hook and WhatsApp-webhook routes. It has
no cookie sessions, no platform or tenant routes and no API docs, and it doesn't load the session-signing secret or
the AI key (`AUTH_JWT_SECRET` and `APP_AI_KEY_ENCRYPTION_KEY` can be left out of its environment). It still needs `APP_INTEGRATION_SECRETS_KEY` to verify channel
webhook signatures, until envelope encryption (ADR-014) replaces it.

## Move an Existing Host to the Two Database Roles (once)

Hosts created before MOH-45 have tables owned by the bootstrap superuser, and the backend
connects as that superuser. Row-Level Security never applies to a superuser, so switch it over:

1. Back up first: `ops/backup.sh`.
2. Create the roles and hand the existing tables to `app_owner`. The script runs as the
   superuser inside the container. Pass the passwords if the container was created without them:

   ```bash
   docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml exec -T \
     -e ROLES_REQUIRED=1 -e APP_OWNER_PASSWORD -e APP_RUNTIME_PASSWORD \
     postgres bash -s < infra/postgres/init/02-roles.sh
   ```

3. In `.env.staging`, set `DATABASE_URL` to `app_runtime` and add `MIGRATION_DATABASE_URL` for
   `app_owner`.
4. Deploy as usual with `ops/deploy.sh <git-sha>`. Migration 0066 grants `app_runtime` its DML
   rights.

## Roll Back

Use `ops/rollback.sh <previous-git-sha>` (see [contract](../../ops/rollback.sh)).

### Rollback Limitation for Destructive Migrations (FR-024)

`ops/rollback.sh` **refuses by default** when it detects migration files between the two SHAs. Use `--force` only after confirming the migrations are backward-compatible (e.g., additive columns, new tables).

**The following changes cannot be reversed by rollback alone:**

| Change | Why it is destructive | What to do instead |
|---|---|---|
| `DROP COLUMN` | Data is lost when the column is dropped | Restore from backup (`docs/runbooks/backup-restore.md`) before deploying |
| `DELETE` or `TRUNCATE` | Referential data may be partially consumed by new code | Restore from backup |
| `ALTER COLUMN TYPE` with lossy cast | Values are truncated or converted irreversibly | Add a preserving column, migrate code, drop the old one |
| No-longer-satisfied `NOT NULL` | Old code cannot insert into the new constraint | Temporarily relax the constraint until old code is updated |

Before any deployment that includes a destructive migration, run a backup (`ops/backup.sh`) and document the rollback path in the release notes.
