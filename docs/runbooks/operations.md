# Operations Runbook

Six common operational tasks for the AI Customer Service Platform.

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
     `docs/runbooks/backup-restore.md` — a destructive migration may have removed
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
   these are the keys of secrets sealed before envelope encryption. Do not
   replace them until `server secrets reencrypt` has reported nothing left
   (section 7); replacing one earlier makes the secrets still sealed under it
   unreadable. To rotate the key that protects tenant secrets going forward,
   rotate the root key (section 7), not these.
5. **For `AUTH_JWT_SECRET`**: All existing sessions signed with the old secret
   become invalid on restart. Coordinate rotation with a maintenance window.
   Users will need to re-authenticate.

### Expected outcome

- The application starts successfully with the new credential.
- For JWT: existing sessions are invalidated; users sign in again.
- For `APP_AI_KEY_ENCRYPTION_KEY` and `APP_INTEGRATION_SECRETS_KEY`: only safe
  after section 7 has moved every secret off them.

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

---

## 6. A Circuit Breaker Is Open

The backend refuses calls at once to an AI provider or to WhatsApp that keeps failing, instead of
tying up workers on it (MOH-40, architecture §10.2 and §15). The breakers are `ai:openai`,
`ai:anthropic`, `ai:gemini` (chat), `ai:openai:embed`, `ai:gemini:embed` (knowledge indexing),
`whatsapp:graph` (sending and media lookup) and `whatsapp:media` (media download).

What a customer sees while one is open:

- **AI chat provider:** the next provider in the tenant's fallback chain answers. With none left, the
  customer gets the "having trouble responding" message and the conversation is escalated to a
  human ("AI assistant unavailable").
- **WhatsApp:** the sender and media workers stop claiming work. Messages stay in the queue and go
  out once the circuit closes. Nothing is marked failed without a try.

### Steps

1. Find the open circuit. In the dashboard, platform admins see System health, with one card per
   breaker (`ai:openai circuit`, …). Or read the metrics:

   ```bash
   curl -sf http://localhost:8080/metrics | grep -E 'circuit_breaker|bulkhead_in_flight'
   ```

   `circuit_breaker_state` is 0 closed, 1 half-open, 2 open. `circuit_breaker_trips_total` counts
   openings; `circuit_breaker_rejections_total{reason}` counts refused calls (`circuit_open`, or
   `bulkhead_full` when every slot to that provider stayed busy for 2 seconds).

2. Find out whether the provider itself is down: its status page, and the logs. The circuit logs
   `circuit opened` (warn) with the breaker name and `circuit closed` (info) on recovery.

   ```bash
   docker compose logs app --since 30m | grep -E 'circuit (opened|half-open|closed)'
   ```

3. Do nothing else if the provider is down. After 30 seconds the circuit lets one call through
   (half-open). If it succeeds the circuit closes; if it fails the circuit stays open for another
   30 seconds. The breaker needs no manual reset.

4. If the provider is up but the circuit keeps opening, the failures are real from this side:
   check outbound network access and DNS from the backend container, and the request timeout
   (60 seconds for AI providers).

### What counts as a failure

Only `unavailable` and `timeout` AI errors, and transient WhatsApp errors (5xx, connection
errors). A rejected API key, a rate limit or a bad request means the provider answered, so one
tenant's expired key never opens the circuit for the others.

### Settings

The limits are constants for now, not environment variables: open after 5 consecutive failures
within 60 seconds, stay open for 30 seconds, one probe call in half-open, at most 16 concurrent
calls per AI provider and 8 per WhatsApp host, 2 seconds to wait for a free slot. They live in
`resilience::Policy::default()` and `whatsapp::guarded::default_policy()`.

### Expected outcome

- The circuit closes by itself after the provider recovers; `circuit_breaker_state` returns to 0.
- Conversations escalated during the outage are in the human queue; answer them as usual.

### When it fails

- **Circuit stays open and the provider is up:** `circuit_breaker_trips_total` rising every
  30 seconds means each probe fails. Treat it as an outbound connectivity problem (step 4).
- **State resets after a restart:** breaker state is in memory per process, so a restart starts
  closed and re-learns within five failed calls.

---

## 7. Rotate Secret Keys and Move Secrets to Envelope Encryption

Tenant secrets (AI provider keys, tool credentials, integration tokens) are sealed
under their own data key, wrapped by a per-tenant key (KEK), wrapped by the root key
(ADR-014). Rotating a key re-wraps keys; it never re-encrypts the secrets. All commands
run as the application role with the same environment as the server:
`server secrets <action>` (`cargo run -p server -- secrets <action>`, or the same
binary in the container). Each prints what it did and exits non-zero if anything
was left undone.

### Move earlier secrets to envelope encryption (once per environment)

After migration `0076` is applied and the new release runs, secrets written before it
are still sealed under `APP_AI_KEY_ENCRYPTION_KEY` or `APP_INTEGRATION_SECRETS_KEY`.
The server reads both kinds, so nothing is down.

1. Make sure the process has the old keys **and** a root key: set `APP_SECRETS_ROOT_KEY`
   (`openssl rand -base64 32`), or leave it unset to use `APP_AI_KEY_ENCRYPTION_KEY`.
   Every process, the `gateway` included, needs the same root key.
2. Back up first (`ops/backup.sh`).
3. Run `server secrets reencrypt`. For each secret it opens it, seals it under a new data
   key, checks the new seal opens to the same plaintext, and stores it only if the row
   is unchanged since it was read. A secret changed meanwhile is counted as "changed while
   running" and keeps the user's new value.
4. Run it again. The migration is finished when `failed` is 0 and a second run
   changes nothing. A secret that is counted as failed did not open with its scope (it was
   already unusable); the log names the row, never the value.

### Rotate a tenant's (or the platform's) key

1. `server secrets rotate-kek <tenant-id|platform>` starts a new key version. New secrets use
   it at once; old ones keep opening under the old version.
2. `server secrets rewrap` wraps every data key under the active version. Secret
   ciphertexts are not touched.

### Rotate the root key

1. Set `APP_SECRETS_ROOT_KEY` to the new key and `APP_SECRETS_ROOT_KEY_PREVIOUS` to the old
   one, then restart every process (rolling restart).
2. `server secrets rewrap-keks` wraps every tenant key under the new root key.
3. Remove `APP_SECRETS_ROOT_KEY_PREVIOUS` and restart. Keep the old key somewhere safe until
   a backup older than the rotation is no longer needed: a restore of such a backup needs it.

### Delete a tenant (crypto-shredding)

Deleting a tenant, soft or hard, destroys its key versions in the same transaction. Every
secret under them is unrecoverable from then on, backups included. Processes drop the key from
memory within five minutes. Restoring the tenant's row does not bring the key back; a
restored tenant must have its secrets entered again.

### Expected outcome

- `reencrypt`: `failed 0`, then a second run with `changed 0`.
- `rotate-kek` then `rewrap`: secrets still open; their ciphertext is unchanged.
- `rewrap-keks`: a second run moves 0.

### When it fails

- **`no root key is configured`**: set `APP_SECRETS_ROOT_KEY`, or the AI or integration key.
- **`failed` greater than 0**: read the log for the row and kind. The usual cause is a secret
  sealed under a key that is no longer set (put the old key back), or a row whose scope
  changed (a tool renamed after its credential was sealed): enter the secret again.
- **Opening fails with "the root key for this key is not available"** after a root rotation:
  `APP_SECRETS_ROOT_KEY_PREVIOUS` is missing on that process. Set it, restart, and run
  `rewrap-keks`.
- **Opening fails with "the key for this secret was destroyed"**: the tenant was deleted. That is
  the intended result.

---

## 8. A User Lost Their Authenticator (Reset Two-Factor Authentication)

A user who has lost both their authenticator app and their recovery codes cannot sign in. Check
who they are before you reset anything: a reset lets whoever knows the password sign in and set up
a new authenticator.

### Steps

1. Confirm the person out of band (a call, a known contact), not from the email they sign in with.
2. Sign in yourself and, if the dashboard asks, enter a code (a reset needs a code proven in the
   last five minutes).
3. Reset the factor:
   - **Tenant owner**, for a member who belongs to no other workspace: the member's actions menu in
     Team (with the dashboard screens, MOH-57), or `POST /api/v1/tenant/members/{membershipId}/mfa/reset`.
   - **Platform administrator**, for anyone (a member of several workspaces, or platform staff):
     `POST /api/v1/platform/users/{userId}/mfa/reset`.
4. Tell the user to sign in with their password. They are held until they set up a new
   authenticator and save the new recovery codes.

### Expected outcome

- The reset returns `204`; every session the user had stops working at once.
- The audit log has `auth.mfa_reset` with you as the actor and the user as the resource (`by` says
  `tenant` or `platform`).

### When it fails

- **`403` from a tenant owner**: the user belongs to other workspaces or is platform staff. Ask a
  platform administrator.
- **`422`**: you cannot reset your own. Use a recovery code, or ask someone else.
- **`401 mfa_step_up_required`**: enter a code (`POST /api/v1/auth/mfa/step-up`) and try again.
- **The user is locked out after wrong codes** (five in a row lock the second step for fifteen
  minutes): wait, or reset the factor.

## 9. Frontend Errors

Browsers send the errors they could not recover from (an unhandled error, an unhandled promise
rejection, a lazy chunk that will not load) to `POST /api/v1/client-errors`. Each one is a backend
log line with target `frontend` and a count in `frontend_errors_total{kind, release}`. Errors from a
page where nobody is signed in are not sent: the endpoint needs a session.

Two alerts watch the counter (`infra/prometheus-alerts.yml`): `FrontendErrorSpike` (at least 20
errors in 10 minutes and more than three times the usual rate) and `FrontendErrorsAfterRelease` (a
backend release that had no errors an hour ago and now has ten or more in 10 minutes).

### Steps

1. Open the dashboard **Frontend Errors** in Grafana (folder: General). It shows the rate by kind
   and by backend release, and the latest reports.
2. Read the reports in Grafana Explore with the Loki data source:

   ```text
   {container=~".*backend.*"} |= "frontend error" | json | target="frontend"
   ```

   Each line has `kind`, `message`, `stack`, `route` (the route pattern, such as
   `tenant/customers/:id`), `client_release` (the dashboard build: the hash in its main script's
   file name), `client_request_id` (the API call involved, when there was one: search it with
   `| json | request_id="<id>"`), `claimed_tenant_id`, `user_id`, `user_agent` and `count`.
   `claimed_tenant_id` is what the browser says it had open and is **not verified**.
3. Group by `client_release` and `route`. One release or one route is a regression; errors spread
   over many releases are usually a browser or an extension. `kind="chunk_load"` right after a
   deploy is the stale-page case (the page reloads itself once, and shows the update notice if that
   does not help): it is expected for a few minutes and is only a problem if it lasts.
   The counter is fed by signed-in browsers, so any user can send reports. Each user can add at
   most 5 to it in 10 minutes (one per report, whatever count it claims; every report is still
   logged), so an alert needs several users, not one. It counts users and places failing, not the
   total number of errors: read the volume in Loki. A big log volume from one `user_id` is noise or
   abuse, not a regression: count by user with
   `sum by (fields_user_id) (count_over_time({container=~".*backend.*"} |= "frontend error" | json | target="frontend" [10m]))`.
4. To read a stack, get the source maps of that build. They are not in the dashboard image; the
   release job publishes them as a **private** package,
   `ghcr.io/<owner>/<repo>-dashboard-sourcemaps:<commit>`, holding only the `.map` files:

   ```bash
   docker create --name maps ghcr.io/<owner>/<repo>-dashboard-sourcemaps:<commit> true
   docker cp maps:/ ./maps && docker rm maps
   (cd frontend && node tools/symbolicate.mjs ../maps stack.txt)   # or pipe the stack on stdin
   ```

   `client_release` is the hash of `main-<hash>.js`: use the build whose maps hold
   `main-<hash>.js.map`.

### Expected outcome

- The alert clears once the cause is fixed and the rate falls back.
- A fix that touches a route shows in the next release as fewer reports for that `route`.

### When it fails

- **No reports at all, but users say something is broken**: reports need a signed-in user and the
  endpoint (`401`, `403` and `429` pause the browser's reporting for a minute). Check the
  `/api/v1/client-errors` requests in the access log, and that `frontend_errors_total` exists at
  `/metrics`.
- **`429` on the endpoint**: one user sends more than 30 requests a minute. The dashboard
  batches and limits itself, so this points at a loop in a browser; look for one `user_id`.
- **The maps package is missing, or anyone can pull it**: the package is created by the first
  release after MOH-168. Its visibility must be **private** (GitHub, Packages, package settings);
  a public package would publish the source of every build.
