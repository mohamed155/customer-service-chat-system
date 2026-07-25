# Phase 1 Data Model: Platform Analytics & System Health

**Feature**: 032-platform-analytics-health | **Date**: 2026-07-25

**No new tables.** This feature is read-only over existing data (spec Assumptions: "Reuse over new storage"). The only schema change is one additive index migration.

---

## 1. Source tables (existing, unmodified)

### `ai_usage_records` — migration `0040`

Append-only log of AI provider calls. Columns used here:

| Column | Type | Use |
|---|---|---|
| `provider` | TEXT, CHECK IN (`openai`,`anthropic`,`gemini`) | grouping key (FR-020); the CHECK fixes the provider set, so FR-023's "no activity" rows are produced by left-joining against these three literals |
| `model` | TEXT | FR-024 failure list |
| `input_tokens` / `output_tokens` | INTEGER NULL, `>= 0` | FR-008 token totals — nullable, so `COALESCE(SUM(...), 0)` |
| `status` | TEXT, CHECK IN (`success`,`failure`) | FR-008 split, FR-020 counts, FR-022 failure rate |
| `error_category` | TEXT NULL, CHECK IN (`authentication`,`rate_limited`,`unavailable`,`timeout`,`invalid_request`) | FR-021 breakdown — exactly the five categories the spec names |
| `latency_ms` | INTEGER NOT NULL | FR-020 percentiles |
| `created_at` | TIMESTAMPTZ | all window filters |
| `tenant_id` | UUID | **deliberately not filtered** (FR-005) |

Columns deliberately **not** read: `request_content`, `response_content`, `request_id` — these carry customer message content and would violate FR-026/SC-008. This exclusion is a requirement, not an optimization.

### `outbox_events` — migrations `0002`, `0021`, `0022`

| Column | Use |
|---|---|
| `processed_at` | `IS NULL` defines the backlog (FR-014) |
| `created_at` | oldest-wait age (FR-014) |
| `attempts` | stuck-retry count, `> 5` (FR-015) |

### `tenants` — migration `0004`

`status` CHECK IN (`active`,`suspended`), `deleted_at` for soft delete, `created_at` for FR-010. All counts filter `deleted_at IS NULL` (FR-006).

### `conversations` — migration `0033`

`status` CHECK IN (`open`,`pending`,`resolved`,`closed`). FR-007 counts `status IN ('open','pending')` across all tenants.

---

## 2. New migration — `0060_platform_analytics_indexes.sql`

Next number after max (`0059`); `0055` and `0058` are absent from the sequence, so this is next-after-max, not next-contiguous. Additive only — no column, constraint, or table changes.

| Index | Definition sketch | Query shape served |
|---|---|---|
| `ai_usage_records_created_idx` | `(created_at DESC)` | `WHERE created_at BETWEEN $1 AND $2` with no tenant predicate — FR-008, FR-020, FR-021, FR-024. The existing `(tenant_id, created_at DESC)` cannot serve this: its leading column is unconstrained. |
| `outbox_pending_created_idx` | `(created_at) WHERE processed_at IS NULL` | FR-014 backlog `COUNT(*)` and `MIN(created_at)`. The existing partial index is scoped to `event_type = 'invitation.email_delivery'`; queue health spans all event types. |
| `outbox_pending_attempts_idx` | `(attempts) WHERE processed_at IS NULL` | FR-015 `COUNT(*) WHERE attempts > 5` |
| `conversations_status_active_idx` | `(status) WHERE deleted_at IS NULL` | FR-007 cross-tenant active count. Existing index leads with `tenant_id`. |

Partial indexes are used where the query always carries the same predicate (`processed_at IS NULL`, `deleted_at IS NULL`), keeping them small.

---

## 3. Response entities (DTOs)

Serde/utoipa DTOs in `backend/crates/modules/platform/src/model.rs`. All timestamps RFC3339 UTC (FR-029); all field names `snake_case`, matching existing module DTOs.

### `PlatformUsageSummaryDto` — GET /platform/analytics/summary

```
range:              { from: Date, to: Date }        # resolved UTC dates
tenants:            { total, active, suspended, created_in_range }   # FR-006, FR-010
active_conversations: i64                            # FR-007
ai_usage:           { total_calls, successful_calls, failed_calls,
                      input_tokens, output_tokens }  # FR-008
generated_at:       DateTime<Utc>                    # FR-029, FR-031
```

**Validation** (FR-009): `from`/`to` are `YYYY-MM-DD`; default `to = today`, `from = today - 30d`; reject `from > to` and spans over 365 days with 422, matching the `analytics` module's existing `resolve_query` behaviour and error envelope.

### `SystemHealthDto` — GET /platform/health

```
overall:      HealthVerdict                          # FR-012 — worst of dependencies
dependencies: [DependencyHealthDto]                  # FR-013
read_at:      DateTime<Utc>                          # FR-019
```

`HealthVerdict` = `healthy | degraded | unhealthy` (serde `snake_case`). Ordering for "worst" is `healthy < degraded < unhealthy`.

### `DependencyHealthDto`

```
name:        "database" | "cache" | "queue" | "storage"
verdict:     HealthVerdict
reason:      Option<String>       # present unless healthy (FR-013)
probe_ms:    u64                  # FR-013
detail:      Option<DependencyDetail>
```

`DependencyDetail` is a tagged union carrying only the evidence that dependency has:

- **queue** → `{ pending_count, oldest_pending_age_seconds: Option<i64>, stuck_retry_count }` (FR-014, FR-015)
- **storage** → `{ backend: "s3" | "in_memory", s3_configured: bool }` (FR-016)
- **database**, **cache** → `None` (binary + `probe_ms` only; spec Assumptions define no degraded band for them)

### `ProviderStatusDto` — GET /platform/ai-providers

```
window:    { from: DateTime<Utc>, to: DateTime<Utc>, preset: "1h"|"24h"|"7d" }  # FR-025
providers: [ProviderMetricsDto]     # always all three (FR-023)
recent_failures: [ProviderFailureDto]   # <= 50, newest first (FR-024)
generated_at: DateTime<Utc>
```

### `ProviderMetricsDto`

```
provider:        "openai" | "anthropic" | "gemini"
verdict:         ProviderVerdict
total_calls, successes, failures: i64
failure_rate:    Option<f64>      # None when total_calls == 0 — never 0/0 (Edge Cases)
p50_latency_ms, p95_latency_ms: Option<i64>   # None when no calls
input_tokens, output_tokens: i64
failures_by_category: { authentication, rate_limited, unavailable, timeout, invalid_request }  # FR-021
```

`ProviderVerdict` = `healthy | degraded | unhealthy | insufficient_data | no_activity`.

**Derivation (FR-022, FR-023)** — evaluated in this order:

1. `total_calls == 0` → `no_activity`
2. `total_calls < 20` → `insufficient_data`
3. `failure_rate >= 0.10` → `unhealthy`
4. `failure_rate >= 0.02` → `degraded`
5. otherwise → `healthy`

Encoding the ladder as an ordered function keeps the two zero-denominator edge cases (steps 1–2) unreachable by the rate comparisons, satisfying the "no division by zero" edge case structurally rather than by guard.

### `ProviderFailureDto`

```
provider, model: String
error_category:  Option<String>
latency_ms:      i64
occurred_at:     DateTime<Utc>
```

No `request_content`, `response_content`, or credential field is ever populated (FR-026, SC-008).

---

## 4. Query inventory

One SQL statement per bullet; none loops per tenant or per provider (Principle X — no N+1).

| # | Purpose | Shape |
|---|---|---|
| Q1 | Tenant counts (FR-006, FR-010) | single `SELECT` over `tenants` with `FILTER (WHERE ...)` aggregates for active/suspended/created-in-range |
| Q2 | Active conversations (FR-007) | `COUNT(*) FROM conversations WHERE status IN ('open','pending') AND deleted_at IS NULL` |
| Q3 | AI usage totals (FR-008) | `COUNT(*)` + `COALESCE(SUM(...),0)` with `FILTER` clauses for the success/failure split, one window predicate |
| Q4 | Per-provider metrics (FR-020–FR-022) | `GROUP BY provider` with `FILTER` aggregates and `percentile_cont(0.5\|0.95) WITHIN GROUP (ORDER BY latency_ms)`; providers absent from the window are filled in application code from the fixed three-value set |
| Q5 | Recent failures (FR-024) | `WHERE status='failure'` + window, `ORDER BY created_at DESC LIMIT 50` |
| Q6 | Queue health (FR-014, FR-015) | one `SELECT` over `outbox_events WHERE processed_at IS NULL`: `COUNT(*)`, `MIN(created_at)`, `COUNT(*) FILTER (WHERE attempts > 5)` |

Q4 uses `FILTER` aggregates so the category breakdown, counts, and percentiles come from a single pass rather than five correlated subqueries.

---

## 5. Non-persisted runtime values

`Health Reading` and `Dependency Result` are computed per request and never stored (spec Key Entities; FR-031). There is no health history table — by design, since SC-004 was rewritten as a request-time property precisely because no continuous monitoring exists.

`StorageBackend { kind, s3_configured }` is process-lifetime state, fixed at startup by the centralized selection function (research R2) and injected as an `Extension`.
