# API Contract: Platform Analytics & System Health (032)

Conventions: JSON snake_case; error envelope + `X-Request-Id` per workspace contract; utoipa-documented via `routes!()` co-registration (required by `openapi_coverage.rs`); GET-only surface (FR-030 — read-only, so every operation is idempotent by construction).

**Mounting**: all three endpoints register inside `platform_routes()` (`backend/crates/server/src/router.rs:262`), which already carries `authentication_middleware` + `platform_permission_middleware`. Platform routes receive **no** `X-Tenant-ID` and are never narrowed by the caller's switched tenant (FR-005); the dashboard's tenant-context interceptor already excludes `/platform/` paths.

**Distinct from the public probes**: `/health`, `/ready`, and `/metrics` are mounted on the bare router outside `api_routes` and remain unauthenticated and unchanged (FR-019a). The endpoints below are additional, not replacements. No queue backlog, storage backend identity, or probe duration is added to the public probes (FR-019b).

---

## GET `/api/v1/platform/analytics/summary`

Global, cross-tenant business metrics.

**Auth**: session + platform role + permission `platform.analytics.view` (**new** — granted to all five platform roles, FR-001).

**operation_id**: `get_platform_analytics_summary` — tag `platform-analytics`

### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `from` | date `YYYY-MM-DD` | `to` − 30 days | inclusive UTC start (FR-009) |
| `to` | date `YYYY-MM-DD` | today (UTC) | inclusive UTC end |

Invalid date format, `from > to`, or a span exceeding 365 days → `422` (FR-009).

### Response `200`

```json
{
  "range": { "from": "2026-06-25", "to": "2026-07-25" },
  "tenants": { "total": 3, "active": 2, "suspended": 1, "created_in_range": 1 },
  "active_conversations": 47,
  "ai_usage": {
    "total_calls": 12840,
    "successful_calls": 12612,
    "failed_calls": 228,
    "input_tokens": 4821004,
    "output_tokens": 1120553
  },
  "generated_at": "2026-07-25T09:14:02Z"
}
```

Soft-deleted tenants are excluded from every count (FR-006). `active_conversations` counts `status IN ('open','pending')` across all tenants and is a *current* figure, not range-scoped (FR-007). Zero-state returns the same shape with zeros — never a null or an omitted field (Edge Cases).

### Errors

`401` unauthenticated · `403` missing `platform.analytics.view`, including every tenant-role caller (FR-003) · `422` invalid range.

---

## GET `/api/v1/platform/health`

Three-state system health across database, cache, queue, and storage.

**Auth**: session + platform role + permission `platform.diagnostics.view` (**existing**, grant unchanged: Super Admin + Developer, FR-002).

**operation_id**: `get_platform_health` — tag `platform-health`

No query parameters. Every reading is computed live at request time (FR-031); the client re-requests every 30 s (FR-019).

### Response `200`

Returns `200` regardless of verdict — an unhealthy *system* is a successful *report*. Non-2xx is reserved for auth and server faults, so the page can render an unhealthy state rather than an error (FR-028).

```json
{
  "overall": "degraded",
  "dependencies": [
    { "name": "database", "verdict": "healthy",   "reason": null, "probe_ms": 3,  "detail": null },
    { "name": "cache",    "verdict": "healthy",   "reason": null, "probe_ms": 1,  "detail": null },
    {
      "name": "queue", "verdict": "degraded",
      "reason": "150 events waiting; oldest waiting 12m",
      "probe_ms": 8,
      "detail": { "kind": "queue", "pending_count": 150, "oldest_pending_age_seconds": 720, "stuck_retry_count": 0 }
    },
    {
      "name": "storage", "verdict": "healthy", "reason": null, "probe_ms": 42,
      "detail": { "kind": "storage", "backend": "s3", "s3_configured": true }
    }
  ],
  "read_at": "2026-07-25T09:14:02Z"
}
```

`overall` is the worst dependency verdict (FR-012), ordered `healthy < degraded < unhealthy`.

**Verdict derivation**

- **database**, **cache** — reuse the existing `HealthCheck` trait objects from `AppState.health_checks`. `Ok` → `healthy`, `Err`/timeout → `unhealthy` with the error as `reason`. No degraded band (spec Assumptions).
- **queue** (FR-014, FR-015) — `healthy`: `pending < 100` **and** oldest `< 5m`. `degraded`: `pending` 100–999 **or** oldest 5–30m **or** `stuck_retry_count > 0`. `unhealthy`: `pending >= 1000` **or** oldest `> 30m`.
- **storage** (FR-016, FR-017) — `unhealthy` when `backend == "in_memory" && s3_configured == true` (silent fallback), or when the read-only `HeadBucket` probe fails. Otherwise `healthy`. A readable-but-unwritable bucket reports healthy by design (FR-017a).

Each probe is bounded by a timeout; a timeout yields `unhealthy` with a timeout `reason` and never suppresses the other dependencies (FR-018).

### Errors

`401` unauthenticated · `403` missing `platform.diagnostics.view` (this includes Sales, Support, and Finance platform roles, not only tenant users).

---

## GET `/api/v1/platform/ai-providers`

Per-provider AI call metrics and recent failures.

**Auth**: session + platform role + permission `platform.diagnostics.view` (FR-002).

**operation_id**: `get_platform_ai_provider_status` — tag `platform-health`

### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `window` | enum `1h` \| `24h` \| `7d` | `24h` | FR-025 |

Any other value → `422`.

### Response `200`

```json
{
  "window": { "from": "2026-07-24T09:14:02Z", "to": "2026-07-25T09:14:02Z", "preset": "24h" },
  "providers": [
    {
      "provider": "openai",
      "verdict": "unhealthy",
      "total_calls": 100, "successes": 75, "failures": 25,
      "failure_rate": 0.25,
      "p50_latency_ms": 820, "p95_latency_ms": 4100,
      "input_tokens": 91002, "output_tokens": 22140,
      "failures_by_category": {
        "authentication": 0, "rate_limited": 19, "unavailable": 4,
        "timeout": 2, "invalid_request": 0
      }
    },
    {
      "provider": "anthropic",
      "verdict": "insufficient_data",
      "total_calls": 3, "successes": 3, "failures": 0,
      "failure_rate": 0.0,
      "p50_latency_ms": 640, "p95_latency_ms": 700,
      "input_tokens": 1200, "output_tokens": 410,
      "failures_by_category": { "authentication": 0, "rate_limited": 0, "unavailable": 0, "timeout": 0, "invalid_request": 0 }
    },
    {
      "provider": "gemini",
      "verdict": "no_activity",
      "total_calls": 0, "successes": 0, "failures": 0,
      "failure_rate": null,
      "p50_latency_ms": null, "p95_latency_ms": null,
      "input_tokens": 0, "output_tokens": 0,
      "failures_by_category": { "authentication": 0, "rate_limited": 0, "unavailable": 0, "timeout": 0, "invalid_request": 0 }
    }
  ],
  "recent_failures": [
    {
      "provider": "openai",
      "model": "gpt-4o",
      "error_category": "rate_limited",
      "latency_ms": 1904,
      "occurred_at": "2026-07-25T09:11:44Z"
    }
  ],
  "generated_at": "2026-07-25T09:14:02Z"
}
```

**All three providers are always present** (FR-023) — a provider with no rows in the window appears with `verdict: "no_activity"` rather than being omitted. `failure_rate` and both percentiles are `null` (not `0`) when `total_calls == 0`, so the client never renders a rate derived from an empty denominator (Edge Cases).

**Verdict ladder** (FR-022, FR-023), evaluated in order: `no_activity` (0 calls) → `insufficient_data` (< 20 calls) → `unhealthy` (rate ≥ 10%) → `degraded` (rate ≥ 2%) → `healthy`.

`recent_failures` is capped at 50, newest first (FR-024), and carries **no** `request_content`, `response_content`, or credential field (FR-026, SC-008).

### Errors

`401` unauthenticated · `403` missing `platform.diagnostics.view` · `422` unknown `window` value.

---

## Inventory updates required by this contract

`openapi_coverage.rs` `EXPECTED` gains exactly three entries; `rbac.rs` `PLATFORM_OPERATIONS` and `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` gain the same paths:

```
("GET", "/api/v1/platform/analytics/summary")
("GET", "/api/v1/platform/health")
("GET", "/api/v1/platform/ai-providers")
```

Omitting these fails `openapi_coverage.rs` in the "extra documented path" direction, which is the intended guard rail — see research R6 for the full exact-inventory file list.
