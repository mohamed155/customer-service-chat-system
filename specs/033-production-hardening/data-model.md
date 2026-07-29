# Phase 1 Data Model: Production Hardening

**Feature**: 033-production-hardening | **Date**: 2026-07-26

This feature is predominantly infrastructural. It introduces **one** persisted table.
Everything else it manipulates is either in-memory runtime state, external time-series
data, or configuration — all documented below so the boundaries are explicit and
`/speckit-tasks` does not invent tables that were deliberately avoided.

---

## Persisted entities

### `tenant_rate_limit_overrides` (NEW — one migration)

Satisfies the per-tenant half of FR-006 ("thresholds MUST be adjustable per environment
and per tenant without a code change"). The per-environment half is configuration, not
data.

| Column | Type | Notes |
|--------|------|-------|
| `id` | `uuid` | PK, `gen_random_uuid()` — repo convention (Principle VIII) |
| `tenant_id` | `uuid` | NOT NULL, FK → `tenants(id)`. Required by Principle II |
| `scope` | `text` | NOT NULL. One of `login`, `widget_session`, `widget_message`, `webhook`, `tenant_global` |
| `max_requests` | `integer` | NOT NULL, CHECK > 0 |
| `window_seconds` | `integer` | NOT NULL, CHECK > 0 |
| `created_at` | `timestamptz` | NOT NULL, default `now()` |
| `updated_at` | `timestamptz` | NOT NULL, default `now()` |
| `deleted_at` | `timestamptz` | NULL — soft delete, repo convention |

**Indexes**:
- `UNIQUE (tenant_id, scope) WHERE deleted_at IS NULL` — partial unique index, matching
  the convention established in spec 005. One live override per scope per tenant.
- The unique index also serves the read path; no separate lookup index is needed.

**Validation rules**:
- `scope` is validated against the enum at the application boundary, not by a database
  CHECK, so adding a scope does not require a migration.
- An override may only *narrow or widen* within operator-set bounds; a request to set
  `max_requests` above `APP_RATE_LIMIT_MAX_OVERRIDE` is rejected, so a misconfigured
  override cannot disable protection entirely.

**Access pattern**: Read on the request hot path. Cached in-process for 60 seconds with
the tenant ID as the key — the table changes rarely and a database round-trip per request
would violate Principle X. Cache invalidation is time-based only; an override takes up to
60 seconds to take effect, which is documented behaviour rather than a bug.

**State transitions**: None. Rows are created, updated, and soft-deleted; there is no
lifecycle.

**No new permission is introduced.** Overrides are managed by platform staff through the
existing `platform.admin` permission. This is deliberate — see `plan.md` Test Impact for
why adding a permission would redden `authz/src/matrix.rs` and `server/tests/auth.rs`.

---

## Runtime structures (not persisted)

### `RateLimiter` (trait, `kernel`)

```text
async fn check(&self, scope: Scope, identifier: &str, budget: Budget) -> Decision
```

- `Scope` — enum above; selects the key prefix and the fail mode.
- `Budget` — `{ max_requests: u32, window: Duration }`, resolved from
  override → environment config → compiled default, in that order.
- `Decision` — `Allowed { remaining, reset_after }` | `Refused { retry_after }` |
  `Degraded { .. }` (counter store unreachable; carries the locally-enforced outcome).

Two implementations: `RedisRateLimiter` (`shared/cache`) and the existing
`InMemoryRateLimitStore` (tests, degraded mode). **This trait replaces the current
synchronous `check(&self, key, limit, window) -> bool`**, which is why all 5 call sites
change — see `plan.md`.

### Redis key space (ephemeral, TTL-bounded)

| Key | Value | TTL |
|-----|-------|-----|
| `rl:{scope}:{identifier}:{window_epoch}` | counter (integer) | `window * 2` |

`identifier` is the authenticated subject where one exists, otherwise the client address
(FR-004). Keys carry no tenant-identifying data beyond the tenant UUID already used
internally, and expire automatically — there is no cleanup job and no retention concern.

### Metric series (external, Prometheus)

Emitted, not stored by the application. **Label sets are a hard constraint**, per
`research.md` §3:

| Metric | Type | Labels |
|--------|------|--------|
| `http_requests_total` | counter | `route`, `method`, `status_class` |
| `http_request_duration_seconds` | histogram | `route`, `method` |
| `rate_limit_refusals_total` | counter | `scope` |
| `rate_limit_degraded_total` | counter | `scope` |
| `outbox_backlog` | gauge | `queue` |
| `dependency_up` | gauge | `dependency` |
| `backup_last_success_timestamp_seconds` | gauge | `target` |

**No metric carries `tenant_id`, user ID, session ID, or a raw request path.** Per-tenant
labels would disclose tenant activity to any scraper (Principle II) and grow series count
without bound (Principle X). Per-tenant usage figures already exist in the analytics
module from spec 025 and belong there.

`dependency_up` is populated from the health results **spec 032 already computes** rather
than by re-probing — see the 032 boundary in `plan.md`.

---

## Configuration (not data)

New `AppConfig` fields, all read in `backend/crates/shared/config/src/lib.rs`. Note the
existing crate mixes bare names (`DATABASE_URL`, `PORT`) with an `APP_` prefix
(`APP_S3_*`, `APP_AI_*`); new variables follow the `APP_` convention.

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `APP_RATE_LIMIT_ENABLED` | no | `true` | Master switch; `false` only for load tests |
| `APP_RATE_LIMIT_LOGIN_MAX` | no | `10` | FR-001 threshold |
| `APP_RATE_LIMIT_LOGIN_WINDOW_SECS` | no | `60` | FR-001 window |
| `APP_RATE_LIMIT_MAX_OVERRIDE` | no | `10000` | Ceiling on per-tenant overrides |
| `APP_RATE_LIMIT_FAIL_MODE` | no | `auth_closed` | Degradation policy (research §2) |
| `APP_SECURITY_HEADERS_ENABLED` | no | `true` | Escape hatch for local debugging |
| `APP_CSP_POLICY` | no | *(computed)* | FR-013 per-environment CSP |
| `APP_CSP_REPORT_ONLY` | no | `true` | Report-only during rollout (research §4) |
| `APP_HSTS_MAX_AGE` | no | `31536000` | Set to `0` locally to avoid pinning localhost |
| `APP_OTLP_ENDPOINT` | no | *(unset)* | Unset disables trace export |
| `APP_TRACE_SAMPLE_RATIO` | no | `1.0` staging / `0.1` prod | FR-020 head sampling |
| `APP_SENTRY_DSN` | no | *(unset)* | Unset disables error capture entirely |
| `APP_RELEASE_SHA` | no | *(unset)* | Release identifier in errors (FR-017) |

Every one of these is optional with a working default, so **no existing deployment or test
breaks by omission** — a deliberate constraint given `AppConfig::from_env` is exercised by
the whole test suite.

`docs/configuration.md` (FR-046) must enumerate **all** variables the platform reads —
existing ones included — sourced from `config/src/lib.rs` directly, not from the README,
which is known to be partial.

---

## Data the feature explicitly does *not* persist

Recorded so these are not re-litigated during task generation:

- **Backup run history** — exported as a Prometheus gauge, not a table. A backup record
  stored inside the database being backed up is circular, and alerting on metric staleness
  catches the never-ran case that a failure row cannot.
- **Rate-limit counters** — Redis only, TTL-bounded. Persisting them would add write load
  to the hot path for data that is worthless after one window.
- **Metrics and traces** — owned by Prometheus and the trace backend. Storing time series
  in Postgres on a host that already shares RAM between Postgres, Redis, and MinIO is the
  wrong trade.
- **Error occurrences** — owned by Sentry, which supplies the grouping and deduplication
  that FR-017 describes.
- **Deployment/release history** — Git tags and registry image tags are the record.
