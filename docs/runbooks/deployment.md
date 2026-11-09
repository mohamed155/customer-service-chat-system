# Deployment Guide

## Bootstrap a Fresh Host

1. **Provision the VM** with Docker Engine 24+ and `docker compose` plugin installed.
2. **Clone the repository** and check out the target release tag.
3. **Create `.env.staging`** from `.env.staging.example` with real secrets.
4. **Create the node-exporter textfile collector directory:**

   ```bash
   sudo mkdir -p /var/lib/node_exporter/textfile_collector
   ```

5. **Start infrastructure services:**

   ```bash
   docker compose --profile observability -f infra/docker-compose.yml up -d
   ```

6. **Run initial migrations:**

   ```bash
   docker compose -f infra/docker-compose.yml -f infra/docker-compose.staging.yml run --rm backend migrate
   ```

7. **Start the full stack:**

   ```bash
   APP_RELEASE_SHA=<sha> docker compose --profile observability -f infra/docker-compose.yml -f infra/docker-compose.staging.yml up -d
   ```

8. **Verify:** the `/api/v1/health` and `/ready` endpoints respond, and the dashboard loads at the configured `PUBLIC_DASHBOARD_URL`.

> **Note:** Omitting `--profile observability` from the docker compose commands leaves the platform running with no metrics, dashboards, logs, or alerting.

## Promote a Release

Use `ops/deploy.sh <git-sha>` (see [contract](../../ops/deploy.sh)).

The script runs migrations before touching the running stack. If migrations fail, the old release stays live. If the new stack fails its readiness check within 120 seconds, the old release is restored.

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
