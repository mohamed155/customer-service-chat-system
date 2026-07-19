# Contract: Tenant Analytics API

Base: `/api/v1` · Auth: session cookie + tenant context (`X-Tenant-ID` contract from 006) · Permission: `analytics.view` on both endpoints (Owner/Admin/Manager; Viewer loses this permission in this feature) · Errors: standard envelope (`ApiError`) · All timestamps/dates UTC.

## Shared query parameters

| Param | Type | Default | Validation |
|-------|------|---------|-----------|
| `from` | `YYYY-MM-DD` | today − 29 days | must be ≤ `to` |
| `to` | `YYYY-MM-DD` | today | range ≤ 366 days → else `422 validation_error` |
| `channel` | string | omitted = all | must be one of `email,phone,web_chat,whatsapp,telegram,widget` → else `422 validation_error` |

Range is inclusive of both endpoint days (internally `created_at < to + 1 day`).

## GET /tenant/analytics/summary

`200` response body (`data` envelope per existing convention):

```json
{
  "data": {
    "range": { "from": "2026-06-20", "to": "2026-07-19" },
    "channel": null,
    "conversation_volume": 1240,
    "concluded_count": 1100,
    "ai_resolution_rate": 0.78,
    "handoff_rate": 0.19,
    "avg_first_response_seconds": 4.2,
    "avg_response_seconds": 6.8,
    "satisfaction_avg": 4.3,
    "satisfaction_count": 312,
    "total_tokens": 5482210,
    "unattributed_tokens": 120400,
    "channels": [
      { "channel": "widget", "conversation_count": 1180, "share": 0.952 },
      { "channel": "email", "conversation_count": 60, "share": 0.048 }
    ]
  }
}
```

Semantics:

- `ai_resolution_rate`, `handoff_rate`, `avg_first_response_seconds`, `avg_response_seconds`, `satisfaction_avg` are `null` when their denominator is empty (frontend renders explicit no-data state — FR-012).
- `channels` always reflects the date range across ALL channels (it drives the breakdown widget) even when `channel` filter is set; all other fields honor the filter.
- With a `channel` filter active, `total_tokens` counts only usage attributable to that channel via generation records, and `unattributed_tokens` is `0`.
- Empty tenant/range → `200` with zeros/nulls, never `404`.

## GET /tenant/analytics/timeseries

`200` response body:

```json
{
  "data": {
    "range": { "from": "2026-07-13", "to": "2026-07-19" },
    "channel": "widget",
    "days": [
      {
        "date": "2026-07-13",
        "conversation_volume": 40,
        "ai_resolved": 28,
        "handed_off": 6,
        "satisfaction_avg": 4.5,
        "satisfaction_count": 11,
        "total_tokens": 182000
      }
    ]
  }
}
```

Semantics:

- Exactly one entry per UTC day in the inclusive range, in ascending date order, zero-filled (`satisfaction_avg` is `null` on days without ratings) — FR-013.

## Error cases (both endpoints)

| Status | Condition |
|--------|-----------|
| `401` | No session |
| `403` | Authenticated but lacks `analytics.view` (e.g., Agent, Viewer) |
| `422` | Invalid date format, `from > to`, range > 366 days, unknown `channel` |

## Non-goals

No POST/PUT/DELETE (read-only feature). No pagination (bounded payloads). No export endpoints (out of scope per spec).
