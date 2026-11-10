# Backup and Restore

## Schedule

A full backup runs nightly via `ops/backup.sh`, invoked from the host cron:

```cron
0 2 * * * /opt/app/ops/backup.sh --target all >> /var/log/backup.log 2>&1
```

The backup captures:

| What | Method | Destination |
|------|--------|-------------|
| Object storage (MinIO) | `mc mirror` | Off-host S3 bucket |
| Database (PostgreSQL) | `pg_dump --format=custom` → `age` encryption | Off-host S3 bucket |

## Which database role dumps

`ops/backup.sh` connects with the standard `PG*` variables, and **`PGUSER` must be the bootstrap
superuser** (`POSTGRES_USER`). Tables with forced Row-Level Security (ADR-004) cannot be dumped by a
role the policies apply to, which includes `app_owner` and `app_runtime`: `pg_dump` stops with an
error. The script checks this before dumping and exits with a message when the role would fail.
Restores run as the superuser as well.

## Ordering

Objects are mirrored **first**, then the database is dumped. This ordering preserves
referential ordering between rows and objects, but with live writes a row may still
reference an object captured after the mirror. For strict consistency, quiesce writes
or take a consistent snapshot before running the backup. The reverse order can produce
a row pointing at an object that was never captured (FR-029).

## Documented RPO and RTO

| Objective | Target | Measurement |
|-----------|--------|-------------|
| RPO (Recovery Point Objective) | ≤ 24 hours | Time since the latest backup timestamp |
| RTO (Recovery Time Objective) | ≤ 4 hours | Time to restore and verify a backup in a throwaway container |

RPO/RTO are measured and reported by `ops/verify-backup.sh`, which exits non-zero if
either objective is missed.

## Restore Procedure

Use `ops/restore.sh` to restore from a specific backup:

```bash
# Identify the backup timestamp from the off-host S3 bucket
mc ls remote/BACKUP_S3_BUCKET/db/

# Restore into a fresh environment (not the live stack)
ops/restore.sh 20260726T020000Z --yes

# Restore into the live stack (requires --force-live)
ops/restore.sh 20260726T020000Z --force-live --yes
```

### Schema version handling

The restore script compares the backup's schema version against the target database's
migration state:

1. **Backup is older** than the target: intervening migrations are applied automatically
   before the data restore.
2. **Backup is newer** than the code: the script **fails loudly** (FR-033). You must
   deploy a release matching the backup's schema version before restoring.

### Manual restore steps

If the automated script is unavailable, follow these steps:

1. Download and decrypt the backup:

   ```bash
   mc cp remote/BUCKET/db/backup-20260726T020000Z.dump.age .
   age --decrypt -i /path/to/age-key.txt backup-20260726T020000Z.dump.age > backup.dump
   ```

2. Restore objects first:

   ```bash
   mc mirror --overwrite remote/BUCKET/objects/20260726T020000Z/ local/APP_S3_BUCKET/
   ```

3. Restore the database:

   ```bash
    pg_restore --clean --if-exists --no-owner --no-privileges \
      --dbname postgresql://<user>:<password>@<host>:5432/<dbname> backup.dump
   ```

4. Apply any pending migrations if the backup was older:

   ```bash
   sqlx migrate run
   ```

## Drill Procedure

Run `ops/verify-backup.sh` to perform the FR-030 drill:

```bash
# Requires: Docker (for throwaway PostgreSQL container)
# Requires: mc, age, psql, pg_restore on the PATH
ops/verify-backup.sh
```

The drill:

1. Finds the latest backup in off-host storage.
2. Downloads and decrypts it.
3. Starts a throwaway PostgreSQL container.
4. Restores the backup into the throwaway database.
5. Checks the schema version matches the source.
6. Counts rows per tenant and compares against the source.
7. Reports measured RPO and RTO against the 24h/4h objectives.
8. Exits non-zero if any objective is missed.

Run the drill weekly to confirm backups are restorable (SC-010).

## Destructive-Migration Path (Rollback Safety)

`ops/rollback.sh` **refuses by default** when migrations ran between the current and
target SHAs. It prints the migration list and directs the operator here.

### Assessing safety

1. Identify the migrations that ran between the two SHAs:

   ```bash
   git log --oneline <target-sha>..<current-sha> -- backend/migrations/
   ```

2. For each migration, determine if it is:

   - **Additive only** (new tables, new columns with defaults, new indexes): Safe to
     roll back. Use `ops/rollback.sh <target-sha> --force`.
   - **Destructive** (column drops, data deletions, table drops, column type changes
     that are not backward-compatible): Unsafe to roll back without a restore.

3. For destructive migrations, restore from backup first, then deploy the target SHA.

### Restore-before-rollback procedure

If a destructive migration must be rolled back:

1. Restore the latest pre-migration backup using `ops/restore.sh` with `--force-live`.
2. Deploy the target SHA:

   ```bash
   ops/deploy.sh <target-sha>
   ```

3. Confirm data integrity with application health checks.

### Preventive measures

- Every migration should be backward-compatible for at least one release cycle.
- Drop columns or tables in a **separate, later migration** — never in the same
  release that stops writing to them.
- Use `sqlx migrate info` to review pending/applied migrations before any deployment.

## Monitoring

Backup success is exported as a Prometheus gauge metric:

```prometheus
# HELP backup_last_success_timestamp_seconds Unix timestamp of last successful backup
# TYPE backup_last_success_timestamp_seconds gauge
backup_last_success_timestamp_seconds 1721980800
```

The directory `/var/lib/node_exporter/textfile_collector` must exist on the host — it is bind-mounted read-only into the `node-exporter` container which exposes the metric to Prometheus.

A Prometheus alert rule fires when this metric is stale (older than 26 hours),
covering the 24-hour RPO plus a 2-hour grace window.

## Key Files

| File | Purpose |
|------|---------|
| `ops/backup.sh` | Nightly backup script |
| `ops/restore.sh` | Restore from backup |
| `ops/verify-backup.sh` | Automated drill and RPO/RTO measurement |
| `infra/prometheus-alerts.yml` | Staleness alert on backup metric |
