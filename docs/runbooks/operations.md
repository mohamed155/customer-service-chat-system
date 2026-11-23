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

This covers rotation of `APP_AI_KEY_ENCRYPTION_KEY`, `APP_INTEGRATION_SECRETS_KEY`, and S3
credentials.

### Steps

1. Generate the new value:

   ```bash
   # For keys requiring base64 of 32 bytes:
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

### Expected outcome

- The application starts successfully with the new credential.
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

## Password recovery limits

`POST /auth/password/forgot` answers `202` after queuing a request. When users report missing
emails, check the worker logs for `password reset email delivery failed` and verify SMTP and
`PUBLIC_DASHBOARD_URL`. The worker retries delivery up to three times. Use the response request
ID to correlate a delivery failure without collecting the link. Monitor queue depth
(`outbox_backlog{queue="password_reset"}`) and oldest request age
(`outbox_oldest_age_seconds{queue="password_reset"}`).

For `429`, check `Retry-After` and the
[`APP_RATE_LIMIT_PASSWORD_RESET_*` budgets](../configuration.md#optional-with-defaults).
Email throttling applies only to requesting links. For `503` from either endpoint, check
Redis connectivity. Password recovery fails closed while Redis is unavailable.

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

---

## 10. The Backend Says It Is Not Ready

`/health` is liveness: it answers `200` while the process runs and asks nothing else. `/ready` is
readiness (MOH-81): it checks **PostgreSQL** (`database`), **Redis** (`cache`) and **object storage**
(`storage`) at once, each inside `READY_PROBE_TIMEOUT_MS`, and answers `503` when any fails.
Three things read it:

- `ops/deploy.sh` waits for `200` before it considers a release up, and rolls back otherwise.
- The container health check (`server healthcheck`, which asks `/ready` from inside the container)
  makes `docker compose ps` show the backend `healthy` or `unhealthy`. In the supplied Compose
  files this only reports state: nothing waits on it and nothing removes an unhealthy backend from
  Caddy, which has a single backend and no readiness checks of its own.
- A readiness-aware load balancer or orchestrator in front of several instances, which is what
  takes an instance out of rotation (and what the shutdown order below is for).

The body names each dependency and a reason, and nothing else, because anyone who can reach the
port can read it:

```json
{"status": "not_ready", "checks": [
  {"name": "database", "status": "ok"},
  {"name": "cache", "status": "error", "error": "unavailable"},
  {"name": "storage", "status": "ok"},
  {"name": "shutdown", "status": "ok"}]}
```

`error` is `unavailable` (the check failed), `timed out` (it did not answer in time) or
`shutting down` (the `shutdown` check, below).

### Steps

1. Read which check is `error` in the body of `curl -s http://localhost:8080/ready`.
2. Find the cause in the backend log: every failure is logged with the check's name and the real
   error, `readiness check failed` (or `readiness check timed out`), at `warn`.
   In Loki: `{container=~".*backend.*"} |= "readiness check"`.
3. Fix the dependency (start Redis, restore the database connection, check the S3 endpoint and
   credentials). The instance becomes ready again on its own at the next probe.

### Things to know

- **`storage` fails although S3 is configured and reachable**: either the process could not create
  its S3 client at startup and fell back to in-memory storage (it logs `failed to create S3
  storage`; in-memory storage loses every upload on restart, so readiness fails on purpose; fix the
  S3 settings and restart), or the credentials lack `s3:ListBucket`, which the `HeadBucket` probe
  needs (`docs/configuration.md`, S3 section).
- **Shutdown order:** when the process gets SIGTERM it fails readiness first (`shutdown` becomes
  `error`, reason `shutting down`), keeps accepting requests for half of `SHUTDOWN_GRACE_SECONDS`
  (at most 5 seconds) so the load balancer can stop routing to it, and only then drains the
  requests in flight. Set the orchestrator's stop timeout above the grace period.
- **A container that stays `unhealthy`**: `docker inspect --format '{{json .State.Health}}' <container>`
  shows the last probe output, for example `healthcheck: ... answered 'HTTP/1.1 503 ...'`; then
  follow the steps above.

### Expected outcome

- `/ready` answers `200` and every check is `ok`; `docker compose ps` shows the backend `healthy`.

### When it fails

- **`/ready` itself does not answer** (connection refused or no answer): the process is down or
  hung, which is a liveness problem, not a dependency problem; check `/health` and the process log.
- **All checks are `ok` but the deploy script still rolls back**: `ops/deploy.sh` polls `/ready`
  for `TIMEOUT` seconds (120, a constant near the top of the script) and a slow start can exceed
  it. Raise the constant in a reviewed change; do not skip the gate.

## 11. Check the Size of the Large Append-Only Tables

None of `messages`, `audit_logs`, `ai_generations`, `ai_usage_records` or `outbox_events` is
partitioned, and [ADR-018](../adr/0018-partition-large-append-only-tables-on-measured-triggers.md)
gives each a trigger that says when that changes. Nothing watches the numbers yet (MOH-202 adds the
metrics and alerts), so this is the check until it does. Run it once a month, and before any
capacity review.

### Steps

1. Connect to the database as the owner role (`just psql` locally) and run:

   ```sql
   SELECT relname AS table_name,
          n_live_tup AS approx_rows,
          pg_size_pretty(pg_total_relation_size(relid)) AS total_size
   FROM pg_stat_user_tables
   WHERE schemaname = 'public'
     AND relname IN ('messages', 'audit_logs', 'ai_generations', 'ai_usage_records', 'outbox_events')
   ORDER BY pg_total_relation_size(relid) DESC;
   ```

   `total_size` counts the heap, every index and TOAST. `approx_rows` is the statistics estimate;
   it reads 0 on a table that autovacuum has not analysed yet.
2. Compare each table with its threshold:

   | Table | Look at | Threshold |
   |---|---|---|
   | `messages` | `total_size` | 100 GB |
   | `ai_generations` | `total_size` | 50 GB |
   | `ai_usage_records` | `total_size` | 50 GB (and see below if it grows faster than its row count) |
   | `audit_logs` | `approx_rows` | 50 million |
   | `outbox_events` | `approx_rows` | 100,000: it should be near zero, because processed rows are deleted |

3. Note the date and the numbers in the capacity record. The volume model in ADR-018 rests on
   assumptions; real numbers replace them.

### Expected outcome

- Every table is below its threshold. A table that is above one is a decision, not an incident:
  follow "When a threshold is crossed".

### When a threshold is crossed

- **Do not partition from this runbook.** Follow ADR-018, decision 4, and rehearse it on a restore
  of production first.
- **For `messages`, `ai_generations` and `ai_usage_records`, count the ids that are not UUIDv7
  before anything else.** Rows from before migration 0075 have random ids that no month range
  holds (ADR-018, decision 3):

  ```sql
  SELECT count(*) FILTER (WHERE get_byte(uuid_send(id), 6) >> 4 <> 7) AS non_v7_ids,
         count(*) AS total
  FROM messages;
  ```

  It scans the table, so run it on a restored copy. A result other than 0 changes the method.
- **`ai_usage_records` grows faster than its rows suggest**: a tenant has turned on content
  capture (`request_content` and `response_content` make a row about 22 times larger, 4,875 bytes
  against 219). Partitioning does not fix that; the content needs its own retention (MOH-142).
- **`outbox_events` is above 100,000 rows**: workers are behind or dead letters are piling up. Look
  at `dead_lettered_at IS NOT NULL` rows and the queue-depth metrics; it is not a partitioning
  question while processed rows are deleted (ADR-007 Q3).
- **A restore drill (section 3) takes longer than the 1 h RTO**: this is a trigger for all five
  tables, whatever their size. Open ADR-018 decision 4 for the largest one first.

---

## 12. Rotate the Session Signing Keys

Access tokens are signed with an Ed25519 key (`AUTH_SIGNING_KEYS`, ADR-005 §2). Rotate every 90
days, and at once if a private key may have leaked. A rotation signs nobody out when the new key
reaches every process **before** it starts signing, and the old key stays as a verifier for one
access-token lifetime (10 minutes by default).

### Steps

1. Make the new key: `cargo run -q -p server -- keygen <kid>` (use the date as the `kid`). Put the
   output in the secret store. A private key is never committed and never goes in a ticket or a
   chat.
2. **Add it as a verifier.** Deploy with the new key listed *after* the current one:
   `[current, new]`. The first entry with a `private_key` signs, so nothing changes yet, but every
   API process now accepts tokens of either key.
3. **Make it sign.** Once the release from step 2 has reached every process, deploy `[new, old]`.
   New tokens carry the new `kid`; tokens of the old key still verify.
4. **Drop the old key** after at least one access-token lifetime (`AUTH_ACCESS_TTL_SECONDS`, 10
   minutes by default; wait 15 to be safe). This also covers the five-minute MFA challenges. Deploy
   `[new]`. A browser that still holds an old access token gets a `401` and refreshes.
5. **Compromise:** skip the waiting and deploy `[new]` at step 3. Everyone is refused once, then
   signs in again by refreshing. If the refresh tokens may be exposed as well, end every session
   (run as the schema owner), which makes everybody sign in with a password:

   ```sql
   UPDATE auth_sessions SET revoked_at = now(), revoke_reason = 'key_compromise'
    WHERE revoked_at IS NULL;
   ```

A verify-only process (one that never signs) can list `{"kid": "...", "public_key": "..."}` entries
instead; the base64url public key of a key is the `x` of `KeySet::jwks`.

### Expected outcome

- New tokens have the new `kid` in their header. Sign-ins and refreshes keep working through each
  deploy, and `auth.refresh_reuse_detected` audit rows do not rise.

### When it fails

- **Startup stops with `AUTH_SIGNING_KEYS is required` or `... is not a valid key list`**: the
  variable is missing or malformed (a `kid` twice, a key that is not base64, a public key that is
  not 32 bytes). The old release keeps serving (`ops/deploy.sh` aborts on a readiness timeout).
- **Many `401` after a deploy**: a process verifies with a key list that lacks the signing key of
  another. Roll the list back to the previous release's, then redo steps 2 and 3 in order.
- **An old session cookie remains after cutover**. The API ignores it. Anyone who only has that
  cookie signs in again. The retired `AUTH_JWT_SECRET`, `AUTH_ACCEPT_HS256` and
  `AUTH_SESSION_TTL_SECONDS` variables are ignored, including at startup.

### Before you deploy the HS256 cutover

Deploy migration 0080 only after PR #146 is fully rolled out and its 8-hour transition window
has elapsed. Measure the window from the last instance that could issue an HS256 session if
that instance remained live during the rollout.

Schedule a maintenance window and deploy manually. The normal `ops/deploy.sh` sequence migrates
while the previous API is live and can restart that API on readiness failure. Drain and stop all
previous API processes before running migration 0080 as `app_owner`. Those processes still
reference the retired table during session validation and MFA verification. Start the cutover
release after the migration succeeds. Any MFA challenge already in flight from the previous release
requires restarting sign-in. Restore a compatible database schema before restarting a previous
release in a rollback.
