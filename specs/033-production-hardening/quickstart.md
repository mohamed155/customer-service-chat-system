# Quickstart: Validating Production Hardening

**Feature**: 033-production-hardening | **Date**: 2026-07-26

Runnable validation scenarios proving each user story works end to end. One section per
story, ordered by priority. Each states prerequisites, commands, and the expected outcome,
and maps to the success criteria it discharges.

See `contracts/` for the interface details and `data-model.md` for the schema; neither is
duplicated here.

---

## Prerequisites

```bash
docker compose -f infra/docker-compose.yml up -d
cd backend && sqlx migrate run
export APP_HSTS_MAX_AGE=0        # do not pin localhost to HTTPS in your browser
cargo run -p server
```

Redis must be running for the shared limiter — it is already in the compose stack, but
`shared/cache` currently only pings it, so verify it is actually reachable before
concluding a limiter test failed for a logic reason.

---

## US1 — Security (P1) · SC-001, SC-002, SC-003

### Sign-in throttle (FR-001)

```bash
for i in $(seq 1 15); do
  curl -s -o /dev/null -w "%{http_code} " -X POST localhost:8080/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -H 'Origin: http://localhost:4200' -H 'X-Requested-With: XMLHttpRequest' \
    -d '{"email":"owner@acme.test","password":"wrong"}'
done; echo
```

Login needs an allowed `Origin` (see `CORS_ALLOWED_ORIGINS`) and a custom header since MOH-36; without them
it answers `403` before the throttle runs.

**Expected**: ten `401`, then `429` for the rest. The `429` carries `Retry-After`.

**Account enumeration check** — repeat with a non-existent address:

```bash
for i in $(seq 1 15); do
  curl -s -o /dev/null -w "%{http_code} " -X POST localhost:8080/api/v1/auth/login \
    -H 'Content-Type: application/json' \
    -H 'Origin: http://localhost:4200' -H 'X-Requested-With: XMLHttpRequest' \
    -d '{"email":"nobody@nowhere.test","password":"wrong"}'
done; echo
```

**Expected**: an identical status sequence. Any divergence means the counter increments
after the account lookup and leaks account existence — see `contracts/rate-limit-responses.md`.

### Shared budget across instances (FR-003, SC-002)

```bash
PORT=8080 cargo run -p server &
PORT=8081 cargo run -p server &
# alternate requests between both ports
for i in $(seq 1 30); do
  P=$([ $((i % 2)) -eq 0 ] && echo 8080 || echo 8081)
  curl -s -o /dev/null -w "%{http_code} " -X POST localhost:$P/api/v1/widget/v1/sessions \
    -H 'Content-Type: application/json' -d '{"widget_id":"<seeded-id>"}'
done; echo
```

**Expected**: ~10 allowed **in total across both ports**, not 10 per port. Twenty
successes means the limiter is still per-process and FR-003 is not met.

### Degraded mode (FR-007)

```bash
docker compose -f infra/docker-compose.yml stop redis
curl -i -X POST localhost:8080/api/v1/auth/login -d '{"email":"a@b.c","password":"x"}'   # expect 503
curl -i -X POST localhost:8080/api/v1/widget/v1/sessions -d '{"widget_id":"<id>"}'       # expect served
docker compose -f infra/docker-compose.yml start redis
```

**Expected**: login fails closed with `503`; widget traffic continues under a local budget.
Both emit a `WARN` and increment `rate_limit_degraded_total`.

### Headers (FR-009 – FR-013, SC-003)

```bash
curl -sI localhost:8080/api/v1/health | grep -iE 'x-content-type|referrer|strict-transport|x-frame'
curl -sI localhost:8080/api/v1/widget/v1/config?widget_id=<id> | grep -iE 'x-frame|content-security'
```

**Expected**: the API response carries `X-Frame-Options: DENY`. The **widget JSON response
must not carry `X-Frame-Options` at all**, and must carry only the Scope A headers — **no
`frame-ancestors`**. Framing directives govern documents being embedded, not JSON fetched
by script inside an already-framed page, so a per-instance allowlist on this response would
be a no-op that also puts a database read on the hot path. See `contracts/security-headers.md`
Scope C. If the widget response has `DENY`, the embeddable channel is broken — this is the
regression this feature is most likely to ship.

The framed document is the static `frontend/apps/widget` `index.html`, served by Caddy:

```bash
curl -sI https://<widget-host>/index.html | grep -iE 'x-frame|content-security'
```

**Expected**: no `X-Frame-Options`, and `Content-Security-Policy: frame-ancestors *`.
Embedding authorization is enforced server-side at session creation by spec 023's domain
allowlist — that, not the header, is the security boundary.

Then confirm the widget still embeds: load `frontend/apps/widget` in an iframe from an
allowlisted origin and verify it renders.

### Refusal records (FR-008)

After triggering the sign-in throttle above:

```bash
docker compose -f infra/docker-compose.yml logs backend | grep rate_limit_refused | head
```

**Expected**: structured `WARN` lines carrying `scope` and a hashed `identifier_hash` —
**never a raw email or IP**. Then confirm the anonymous refusals did *not* reach the audit
table, which cannot be pruned:

```sql
SELECT count(*) FROM audit_logs
WHERE action = 'security.rate_limit.refused' AND tenant_id IS NULL;
```

**Expected**: `0`. A non-zero count means unauthenticated refusals are writing unprunable
rows and the audit trail is an amplification target.

---

## US2 — Staging deployment (P2) · SC-004, SC-005

Requires a provisioned host and registry credentials; see `docs/deployment.md`.

First confirm the gate actually published something to promote — a deploy script with no
images to pull fails at the first step:

```bash
gh run download --name release-manifest && cat release-manifest.json | jq .
docker manifest inspect ghcr.io/<owner>/<repo>-backend:<git-sha>   # and -dashboard, -widget
```

**Expected**: a manifest naming all three images at the same SHA, and all three resolvable
in the registry. **No tag may be `latest`** — rollback and FR-040 are unverifiable with
mutable tags.

```bash
./ops/deploy.sh <git-sha> --dry-run   # inspect the plan first
./ops/deploy.sh <git-sha>
curl -s https://staging.<host>/api/v1/health
curl -s https://staging.<host>/api/v1/ready | jq .
```

**Expected**: deploy completes in under 30 minutes (SC-004); `/ready` reports every
dependency ok.

**Migration-abort check** — deploy a SHA with a deliberately broken migration:

**Expected**: the script aborts and the **previous release is still serving**. Verify by
curling `/health` throughout; it must never fail.

**Metrics ACL**: `curl https://staging.<host>/metrics` from outside the host **must not**
succeed. From the host, `curl localhost:9090/metrics` must.

```bash
./ops/rollback.sh <previous-sha>
```

**Expected**: previous release serving in under 15 minutes (SC-005). If migrations ran
between the SHAs, the script **refuses** and names them.

---

## US3 — Observability (P3) · SC-006, SC-007

```bash
curl -s localhost:8080/metrics | head -30
```

**Expected**: real Prometheus exposition, not `# no metrics yet`.

```bash
curl -s localhost:8080/metrics | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-' | head
```

**Expected**: **no output.** Any UUID in a label means a tenant/user/session identifier
leaked into the metric label set — see `contracts/metrics-exposition.md`.

### Correlation (FR-014, SC-006)

```bash
RID=$(curl -sI localhost:8080/api/v1/tenant/conversations | grep -i x-request-id | cut -d' ' -f2 | tr -d '\r')
echo "$RID"
```

Bring the observability stack up and resolve that identifier in all three surfaces
**without a shell on the host** — that "without shell access" clause is the part of FR-014
that container stdout alone does not satisfy:

```bash
docker compose -f infra/docker-compose.yml --profile observability up -d
```

1. **Logs** — in Grafana, query Loki with `{container="backend"} | json | request_id="$RID"`.
   The stream selector stays bounded (`container`, `level`); the identifier is a line filter
   over the parsed JSON, **not a label**. A `{request_id="…"}` selector would mean promtail
   is creating one Loki stream per request — see T104.
2. **Traces** — click the derived field on that log line; it links to the trace.
3. **Errors** — search the same identifier in Sentry.

**Expected**: all three resolve to the same request, in under 5 minutes of operator effort.

### Alerting (FR-021)

```bash
docker compose -f infra/docker-compose.yml --profile observability exec prometheus \
  promtool check rules /etc/prometheus/prometheus-alerts.yml
```

**Expected**: all four rule groups valid — error rate, latency, `dependency_up`, and backup
staleness. Then stop a dependency (`docker compose stop redis`) and confirm the
`dependency_up` alert transitions to firing in the Prometheus UI and reaches the configured
Alertmanager receiver. An alert rule that fires into an unconfigured receiver does not
satisfy FR-021's "notify an on-call recipient".

### Error capture and scrubbing (FR-017, FR-019, SC-007)

Trigger a deliberate failure with a request carrying a credential-shaped value and a
customer email.

**Expected**: a grouped Sentry issue appears within 2 minutes carrying `request_id`,
release, and environment — and containing **neither** the credential nor the email. The
scrubbing half is the part that fails silently, so check the payload, not just the
presence of the issue.

### Dashboards (FR-018)

Import `infra/grafana/dashboards/*.json` and confirm request rate, error rate, latency
distribution, and backlog render over a selectable range.

**Note**: this is Grafana, **not** a new dashboard page. Spec 032 already owns the
in-product System Health page; 033 adds no route, nav entry, or permission — see the
boundary in `plan.md`.

---

## US4 — Backup and restore (P4) · SC-010, SC-011

```bash
./ops/backup.sh --target all
./ops/verify-backup.sh
```

**Expected**: `verify-backup.sh` restores into a throwaway container, matches per-tenant
row counts against source, and reports measured RPO/RTO inside 24 h / 4 h. Non-zero exit
if either objective is missed.

**Off-host check**: point the destination at the local MinIO endpoint. **Expected**: the
script refuses — a backup on the machine being backed up is not a backup (FR-028).

**Schema-mismatch check**: restore a dump taken *after* a migration into a stack running
code from *before* it. **Expected**: loud failure, not a silent restore (FR-033).

**Staleness alert (FR-032, SC-011)**: stop the cron job and wait past the interval.
**Expected**: the alert fires on staleness. A failure-event alert would not catch this —
the job never ran, so it never failed.

---

## US5 — CI gate (P5) · SC-008, SC-009

```bash
cd backend && cargo fmt --check && cargo clippy --workspace -- -D warnings
REQUIRE_DB_TESTS=1 cargo nextest run --workspace
```

**Non-vacuity (FR-039, SC-008)** — the scenario that matters:

```bash
git checkout -b verify-gate
# break one assertion in any test
git commit -am 'deliberate break' && git push
```

**Expected**: the gate **fails** and names the test. A gate that stays green here is
checking nothing, which is the current defect: `ci.yml` runs no migrations and omits
`REQUIRE_DB_TESTS`, so DB-backed tests self-skip while reporting success.

**Executed-count floor (FR-035)**: drop `REQUIRE_DB_TESTS`. **Expected**: the count falls
below the committed floor and the build fails rather than passing with fewer tests.

**Determinism (SC-009)**: re-run the same SHA three times; all three verdicts identical,
with `rust-toolchain.toml` pinning the toolchain.

---

## US6 — Documentation (P6) · SC-012, SC-013

**The only valid test is a person who has not set up the project following only the written
docs.** Self-review does not discharge SC-012 — the author cannot detect their own assumed
knowledge.

Record every point where they ask or guess; each is a defect to fix (FR-048).
**Expected**: running stack in under 60 minutes, zero questions.

**Config completeness (FR-046, SC-013)**:

```bash
grep -oE 'env::var\("[A-Z_]+"\)' backend/crates/shared/config/src/lib.rs \
  | grep -oE '"[A-Z_]+"' | tr -d '"' | sort -u > /tmp/code-vars.txt
grep -oE '`[A-Z_]{4,}`' docs/configuration.md | tr -d '`' | sort -u > /tmp/doc-vars.txt
comm -23 /tmp/code-vars.txt /tmp/doc-vars.txt
```

**Expected**: empty output — every variable the code reads is documented. Source this from
`config/src/lib.rs`, not the README, which is known to be partial.

---

## Full validation sweep

```bash
cd backend && cargo fmt --check && cargo clippy --workspace -- -D warnings \
  && REQUIRE_DB_TESTS=1 cargo nextest run --workspace
cd ../frontend && pnpm lint && pnpm format:check \
  && pnpm exec ng test dashboard --watch=false --browsers=ChromeHeadless \
  && pnpm test:e2e
./ops/verify-backup.sh
K6_BASE_URL=https://staging.<host> k6 run loadtest/dashboard-read.js
```

Do not pipe these to `tail` — a failing build reads as exit 0. Redirect to a file and check
`$?`.
