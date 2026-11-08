# Operations Runbook

Five common operational tasks for the AI Customer Service Platform.

---

## 1. Promote a Release

### Steps

1. Identify the commit SHA to promote (e.g. from CI on `main`).
2. Verify the SHA has passed CI:

   ```bash
   gh run list --commit <sha> --workflow backend.yml
   gh run list --commit <sha> --workflow frontend.yml
   ```

3. Push the Docker image tag if the registry does not follow Git SHAs:

   ```bash
   docker tag app:<sha> registry.example.com/app:<sha>
   docker push registry.example.com/app:<sha>
   ```

4. On the target host, pull the new image:

   ```bash
   docker compose -f infra/docker-compose.yml pull
   ```

5. Run migrations:

   ```bash
   cd backend && sqlx migrate run
   ```

6. Restart the service:

   ```bash
   docker compose -f infra/docker-compose.yml up -d --no-deps app
   ```

7. Poll the readiness endpoint:

   ```bash
   ok=0; for i in $(seq 1 30); do
     if curl -sf http://localhost:8080/ready; then ok=1; break; fi
     sleep 2
   done; [ "$ok" = 1 ] || { echo "readiness probe timed out"; exit 1; }
   ```

### Expected outcome

- `/ready` returns `200 OK` within 60 seconds.
- `/api/v1/health` returns `200 OK` with all dependencies `"up"`.
- Existing sessions and in-flight requests are not interrupted (graceful shutdown).

### When it fails

- **Migration fails**: The old release is still running. Investigate the
  migration error (schema conflict, timeout, data issue), fix it, commit a new
  SHA, and retry.
- **Readiness probe times out**: Roll back immediately (see §2). Check logs
  with `docker compose -f infra/docker-compose.yml logs app --tail 100`.
- **Image pull fails**: Verify registry credentials and network access. Check
  that the SHA tag exists in the registry.

---

## 2. Roll Back

### Steps

1. Identify the previous known-good SHA.
2. Check whether any migrations ran between the current and target SHA:

   ```bash
   git log --oneline <target-sha>..<current-sha> -- backend/migrations/
   ```

3. **If no migrations ran between the two SHAs**, proceed:

   ```bash
    ops/rollback.sh <target-sha>
   ```

4. **If migrations did run**, the rollback is more involved:
   - The deployment script (`ops/rollback.sh`) **refuses by default** when
     migrations intervened. Use `--force` only after reading
     `docs/backup-restore.md` — a destructive migration may have removed
     columns or data that the old code expects.
   - If the migration was additive only (new columns/tables), rollback is
     safe and you can proceed.
   - If the migration was destructive (dropped columns, deleted data), you
     must restore from backup first (see §3), then deploy the old SHA.
5. Confirm readiness after rollback:

   ```bash
   curl -sf http://localhost:8080/ready
   ```

### Expected outcome

- The previous release serves requests within 60 seconds.
- `/ready` passes.
- No data loss (unless a destructive migration was force-rolled without a
  backup restore).

### When it fails

- **New SHA is also broken**: Iterate further back until you find a green SHA.
- **Database schema mismatch**: The code expects a column or table the current
  schema does not have. You must restore from backup or write a compensating
  migration.
- **Rollback of destructive migration without backup**: This is a critical
  incident. Engage the on-call engineer and the data team. See §3 for restore.

---

## 3. Restore a Backup

### Steps

1. Identify the backup to restore. Backups are named by timestamp:

   ```text
   backup-<YYYYMMDDTHHMMSSZ>.sql.gz.age
   ```

2. Decrypt the backup (requires the `age` private key):

   ```bash
   age --decrypt -i /path/to/age-key.txt \
     backup-20260726T120000Z.sql.gz.age > backup.sql.gz
   ```

3. Verify the backup is not corrupt:

   ```bash
   gzip -t backup.sql.gz
   ```

4. **(Off-host restore preferred)** Restore into a fresh database container:

   ```bash
   createdb temp_restore
   gunzip -c backup.sql.gz | psql temp_restore
   ```

5. Run the migration checker to confirm the schema version matches the current
   code:

   ```bash
    ops/verify-backup.sh
   ```

6. To restore the live database:

   ```bash
   # Stop the app
   docker compose stop app
   # Drop and recreate the database
   dropdb <dbname> && createdb <dbname>
   # Restore
   gunzip -c backup.sql.gz | psql <dbname>
   # Restart
   docker compose up -d app
   ```

### Expected outcome

- All data present and queryable.
- Application connects and serves requests.
- Object storage (S3) is consistent with the restored database (objects were
  mirrored before the database dump in the backup script).

### When it fails

- **Backup is corrupt or missing**: Check the backup script's logs and the
  Prometheus `backup_last_success_timestamp_seconds` metric. The RPO objective
  is 24 hours; if no valid backup exists within that window, data loss is the
  fallback state.
- **Schema version mismatch**: If the backup's schema is older than the
  current code, run intervening migrations first. If the backup's schema is
  **newer** than the code, you must deploy the SHA that matches the backup.
- **Decryption fails**: Verify the `age` private key is correct and matches the
  public key used during encryption. The key must be stored off-host.

---

## 4. Rotate a Credential

This covers rotation of `AUTH_JWT_SECRET`,
`APP_AI_KEY_ENCRYPTION_KEY`, `APP_INTEGRATION_SECRETS_KEY`, and S3
credentials.

### Steps

1. Generate the new value:

   ```bash
   # For keys requiring base64 of 32 bytes:
   openssl rand -base64 32
   # For AUTH_JWT_SECRET (also base64 of 32+ bytes):
   openssl rand -base64 32
   ```

2. Update the secret in the deployment environment (Docker secrets, Vault,
   Kubernetes secret, or `.env` file).
3. Restart the service without downtime (rolling restart if multi-instance):

   ```bash
   docker compose up -d --no-deps app
   ```

4. **For `APP_AI_KEY_ENCRYPTION_KEY` and `APP_INTEGRATION_SECRETS_KEY`**:
   Existing encrypted values become **unreadable** after rotation. The system
   uses a single active key with no previous-key fallback or transparent
   re-encryption. Plan rotation during a maintenance window and accept that
   existing ciphertext will be re-encrypted on next write.
5. **For `AUTH_JWT_SECRET`**: All existing sessions signed with the old secret
   become invalid on restart. Coordinate rotation with a maintenance window.
   Users will need to re-authenticate.

### Expected outcome

- The application starts successfully with the new credential.
- For JWT: existing sessions are invalidated; users sign in again.
- For encryption keys: existing encrypted data becomes unreadable after
  rotation. Re-encrypt secrets under the new key on next write.

### When it fails

- **Application fails to start**: The credential format is invalid (wrong
  length, not valid base64). Revert to the old value and restart, then
  regenerate.
- **Data becomes undecryptable**: The old key was discarded before re-encryption
  completed. Restore from backup if available. This is why rotation procedures
  should **never** delete the previous key until all data has been re-encrypted.

---

## 5. Investigate a Reported Error

### Steps

1. Determine the scope:
   - **One user affected**: Likely a client-side or permission issue.
   - **All users affected**: Likely a deployment, infrastructure, or
     dependency issue.
   - **One feature affected**: Likely an application bug.

2. Check the health endpoint:

   ```bash
   curl -sf http://localhost:8080/api/v1/health | jq .
   ```

   If a dependency is `"down"`, address that first.

3. Check application logs:

   ```bash
   docker compose logs app --tail 200 --since 10m
   ```

   If structured logging (`LOG_FORMAT=json`) is on, pipe through `jq`:

   ```bash
   docker compose logs app --since 10m | grep -v "^[a-z]" | jq 'select(.level == "ERROR")'
   ```

4. Check Sentry (if configured) for grouped errors with stack traces:

   ```bash
   # Sentry UI at https://<org>.sentry.io
   # Filter by release SHA or environment
   ```

5. Check metrics:

   ```bash
   curl -sf http://localhost:8080/metrics | grep -E '(http_requests_total|http_request_duration_seconds|rate_limit_refusals_total)'
   ```

   Look for error rate spikes or latency degradation.

6. Trace a specific request through the system using the correlation ID
   (`X-Request-Id`). Every log line and span carries this ID. If OTLP trace
   export is configured, find the trace in the observability backend.
   - Query Loki for the request ID: `{container="backend"} | json | request_id="<id>"`
   - In Grafana Explore, the `request_id` derived field in the Loki datasource
     provides a one-click TraceQL link to the correlated Tempo trace.

7. Check the `RUST_BACKTRACE` environment variable — if set to `1` or `full`,
   the process prints a backtrace on panic.

### Expected outcome

- Root cause identified and actionable.
- If a bug: a GitHub issue is filed with the SHA, error message, trace ID,
  and reproduction steps.
- If infrastructure: the on-call engineer is notified.

### When it fails (investigation itself fails)

- **No logs, no traces, no metrics**: The error may have occurred before
  observability initialised (e.g. config parse failure). Check startup logs
  with `docker compose logs app --tail 50`.
- **Error not reproducible**: Ask the reporter for the exact timestamp and
  `X-Request-Id`. Cross-reference with the load balancer access logs and
  the application logs at that precise time.
- **Metrics show nothing unusual but errors persist**: The error may be in a
  downstream dependency that returns success codes for its own health but
  fails on the actual operation (e.g. S3 writes succeed but reads are slow).
  Check each dependency's own metrics endpoint if available.
