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

An erasure between the mirror and the database dump can also leave objects in the backup
whose database locators are already gone. Reconcile the restored object store before
resuming access.

## Documented RPO and RTO

| Objective | Target | Measurement |
|-----------|--------|-------------|
| RPO (Recovery Point Objective) | ≤ 24 hours | Time since the latest backup timestamp |
| RTO (Recovery Time Objective) | ≤ 4 hours | Time to restore and verify a backup in a throwaway container |

RPO/RTO are measured and reported by `ops/verify-backup.sh`, which exits non-zero if
either objective is missed.

## Restore Procedure

Use `ops/restore.sh` to restore from a specific backup:

Before the live restore, close public ingress at the load balancer or network rule and stop the API,
gateway and every background worker, including processes running with `APP_RUN_MODE=worker` or
`APP_RUN_MODE=all`. For this procedure, “stopped” means public ingress stays closed; a controlled,
internal-only replay window may run after the restore. Keep ingress closed until every check under
[Before resuming access](#before-resuming-access) passes.

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

### Replay customer erasures

Before serving restored PostgreSQL data or restored objects, reconcile the restore point with the
customer erasures in the durable request ledger outside the restored snapshot. Erasure steps run in
the privacy worker, which starts in `worker` and `all` modes. The erase endpoint needs an API
process, so keep public ingress closed and open a controlled replay window after the restore:

1. Start one internal-only API process with `APP_RUN_MODE=api` and one internal-only worker with
   `APP_RUN_MODE=worker`, connected to the restored database, Redis and object store. Restrict both
   processes so only the operator can reach them.
2. The worker mode starts all background workers, not only privacy: invitation and password-reset
   delivery, login-lockout notifications, escalation/agent/notification outbox work, knowledge
   indexing, and widget/session/tool/notification/integration sweepers. When
   `APP_INTEGRATION_SECRETS_KEY` is configured, it also starts the WhatsApp sender and media
   workers. Leave that key unset for this replay window; `main.rs` logs that the WhatsApp workers
   are disabled. Configure the restored environment's other outbound integrations for safe,
   non-production delivery before starting the worker.
3. For every ledger erasure whose request is missing from the restored database or whose status is
   not `completed`, POST `/api/v1/tenant/customers/{id}/erase` with
   `{"confirm":"<customer-id>"}`, using a tenant-admin session and completing the required
   step-up. An existing receipt is returned; the worker continues queued, running or failed
   requests, and expired leases are reclaimed automatically. Keep the worker running until each
   request reports `completed`.
4. Before continuing, run this query against the restored database and require a result of `0` for
   every ledger customer:

   ```sql
   SELECT count(*) FROM privacy_requests
   WHERE kind = 'erase' AND status <> 'completed';
   ```

5. Stop the replay-window API and worker processes. Continue to
   [Reconcile restored privacy objects](#reconcile-restored-privacy-objects) while public ingress
   remains closed, even when every erasure receipt already shows completion. Keep backup copies
   under the configured retention schedule.

### Reconcile restored privacy objects

After the replay window has stopped, keep the API, gateway and all background workers stopped while
reconciling objects. Public ingress must remain closed through this step and until every check under
[Before resuming access](#before-resuming-access) passes. Use the restored database's bootstrap
superuser and AWS CLI v2 with credentials for the restored application bucket. Set the usual `PG*`
variables for the restored database, `APP_S3_ENDPOINT` and `APP_S3_BUCKET` for the restored object
store, and the corresponding AWS CLI credentials and region. With Bash, `psql` and `jq` installed,
generate a deletion manifest. Listing from both prefix roots includes every tenant prefix in the
restored bucket, including tenants absent from the restored database.

Before building the manifest, check the bucket's versioning state. The command below exits non-zero
for `Enabled` or `Suspended` versioning. Those buckets need a separate version-aware procedure that
uses `list-object-versions` and deletes every object version and delete marker by `VersionId`; a
plain `delete-object` without `VersionId` only adds a delete marker and leaves noncurrent versions.
That noncurrent-version limitation is recorded in [ADR-020](../adr/0020-customer-data-export-and-erasure.md#known-limitations).

```bash
set -euo pipefail
: "${APP_S3_ENDPOINT:?Set the restored S3 endpoint}"
: "${APP_S3_BUCKET:?Set the restored application bucket}"
versioning_status=$(aws --no-cli-pager --endpoint-url "$APP_S3_ENDPOINT" s3api get-bucket-versioning \
    --bucket "$APP_S3_BUCKET" --query 'Status' --output text)
if [[ "$versioning_status" == Enabled || "$versioning_status" == Suspended ]]; then
    printf 'Refusing privacy-object reconciliation: bucket versioning is %s; use the version-aware procedure.\n' \
        "$versioning_status" >&2
    exit 1
fi
privacy_dir=$(mktemp -d)
trap 'rm -rf "$privacy_dir"' EXIT

psql -XAt -v ON_ERROR_STOP=1 > "$privacy_dir/referenced.json" <<'SQL'
SELECT COALESCE(json_agg(key), '[]'::json)
FROM (
    SELECT COALESCE(storage_key, 'whatsapp-media/' || tenant_id || '/' || id) AS key
      FROM message_attachments
    UNION
    SELECT object_key AS key
      FROM privacy_requests
     WHERE kind = 'export' AND status = 'completed'
       AND expires_at > now() AND object_key IS NOT NULL
) AS referenced;
SQL

list_orphans() {
    for prefix in whatsapp-media/ privacy-exports/; do
        aws --no-cli-pager --endpoint-url "$APP_S3_ENDPOINT" s3api list-objects-v2 \
            --bucket "$APP_S3_BUCKET" --prefix "$prefix" --output json |
            jq -c --arg bucket "$APP_S3_BUCKET" --slurpfile referenced "$privacy_dir/referenced.json" '
                ($referenced[0] | INDEX(.)) as $keep
                | .Contents[]?.Key
                | select($keep[.] == null)
                | {Bucket: $bucket, Key: .}
            ' || return 1
    done
}

list_orphans > "$privacy_dir/orphans.jsonl"
cat "$privacy_dir/orphans.jsonl"
```

Confirm the target bucket and manifest, then run the following in the same shell.
If the final check fails, inspect the remaining objects before allowing access.

```bash
while IFS= read -r object; do
    aws --no-cli-pager --endpoint-url "$APP_S3_ENDPOINT" s3api delete-object --cli-input-json "$object"
done < "$privacy_dir/orphans.jsonl"

list_orphans > "$privacy_dir/remaining.jsonl"
test ! -s "$privacy_dir/remaining.jsonl"
```

### Before resuming access

- Verify the restored schema matches the intended application version.
- Verify every post-backup ledger erasure is completed and the non-completed erase query above
  returns `0`.
- Verify the restored bucket has no orphaned WhatsApp media or privacy export objects.
- Start the normal application processes, then reopen public ingress.

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
