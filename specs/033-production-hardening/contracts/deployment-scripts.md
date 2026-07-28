# Contract: Operations Scripts

**Feature**: 033-production-hardening | **Requirements**: FR-022 – FR-033, FR-040 – FR-044

CLI contracts for `ops/` and `loadtest/`. These are the interfaces an operator uses under
time pressure, so failure modes are specified as tightly as success paths.

**Shared conventions**: `set -euo pipefail`; exit `0` success, `1` failure, `2` misuse;
all destructive actions require either `--yes` or an interactive confirmation; every script
supports `--dry-run` printing what it would do without doing it.

---

## `ops/deploy.sh <git-sha> [--dry-run] [--skip-migrations]`

Promotes a commit to staging (FR-022, FR-023, FR-027).

1. Verify `<git-sha>` exists and images are published for it — **fail before touching
   staging** if not.
2. Pull images by immutable SHA tag.
3. Run migrations in a one-shot container.
4. Start the new stack; poll `/ready` until ready or a 120 s timeout.
5. Switch Caddy upstream; stop the old stack.

**Failure behaviour**: a migration failure aborts with the **old release still serving**
(FR-023). A readiness timeout aborts and leaves the old release serving. The script never
leaves a partially-migrated environment accepting traffic.

Requires `APP_RELEASE_SHA` to be passed into the container so errors carry a release
identifier (FR-017).

## `ops/rollback.sh <previous-git-sha> [--yes]`

Returns the previous release to service (FR-024). Same steps minus migrations.

**Refuses by default when migrations ran between the two SHAs**, printing which ones and
directing the operator to `docs/backup-restore.md`. FR-024 requires the procedure to state
which changes it cannot reverse — refusing is stronger than documenting a caveat and
hoping it is read. `--force` overrides for the operator who has confirmed the migrations
are backward-compatible.

Target: under 15 minutes (SC-005).

## `ops/backup.sh [--target db|objects|all]`

Nightly via cron (FR-028, FR-031).

**Order is significant**: objects are mirrored **first**, then the database is dumped. A
restored row referencing an object then always finds it. The reverse order can produce a
row pointing at an object that was never captured (FR-029 consistency window).

- Objects: `mc mirror` local MinIO → off-host bucket
- Database: `pg_dump --format=custom` → `age` encryption → off-host bucket
- Destination **must** be off-host; the script rejects a destination resolving to the
  local MinIO endpoint (FR-028)
- Writes `backup_last_success_timestamp_seconds` to the node-exporter textfile collector
- Prunes artifacts older than `BACKUP_RETENTION_DAYS` (default 30)
- `nice`/`ionice`-wrapped so it does not destabilise the co-located database (spec edge
  case)

**Alerting is on staleness, not on failure events** (FR-032): a backup that never ran emits
no failure, so absence is the only signal that catches the silent case.

## `ops/restore.sh <backup-timestamp> --target <env> --yes`

Restores into a fresh environment (FR-029, FR-033).

- **Refuses to target the live staging stack without `--force-live`** — the most dangerous
  operation in the repository.
- Compares the dump's schema version against the target's migration state. On mismatch:
  applies intervening migrations if the dump is older, **fails loudly** if the dump is
  newer than the code. Never silently restores a shape the application misreads (FR-033).
- Restores objects, then the database — the mirror of the backup order.

## `ops/verify-backup.sh`

The FR-030 drill, runnable unattended. Restores the latest backup into a throwaway
container, runs the migration checker, counts rows per tenant, compares against source,
and reports measured RPO and RTO against the documented 24 h / 4 h objectives (SC-010).
Non-zero exit if either objective is missed.

## `loadtest/*.js` (k6)

`k6 run loadtest/dashboard-read.js` with `K6_BASE_URL` set.

**`K6_BASE_URL` has no default — the script exits `2` if unset**, and refuses any host not
matching `LOADTEST_ALLOWED_HOSTS`. FR-043 requires that load tests cannot accidentally run
against a real tenants' environment; a `localhost` default would be one careless variable
away from pointing at production.

Thresholds encode SC-014 in the script (`http_req_failed < 1%`,
`http_req_duration p(95) < 500ms`), so k6's own exit code is the pass/fail verdict rather
than a human's reading of a report.

---

## CI artifact (FR-040)

`backend.yml` publishes `release-manifest.json` on success:

```json
{ "sha": "<git-sha>", "built_at": "<iso8601>", "images": ["<repo>/backend:<sha>", "<repo>/frontend:<sha>"], "tests_executed": 812 }
```

`tests_executed` is the count asserted against the committed floor (FR-035), and `sha` is
the argument `ops/deploy.sh` takes — the artifact is what connects the gate to the
deployment.
