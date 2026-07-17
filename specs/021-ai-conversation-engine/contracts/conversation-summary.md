# Contract: Conversation Summary (REST)

## `POST /tenant/conversations/{id}/summary`

Generates a staff-only, on-demand summary of the conversation. Side-effect-free (nothing persisted; safe to repeat — Principle V idempotency in effect). Documented in OpenAPI (`utoipa`) and covered by the OpenAPI coverage test.

### Auth & isolation

- Requires authenticated session + `X-Tenant-ID` tenant context (existing middleware).
- Requires active tenant membership with conversation read access (same `authz` gate as `GET /tenant/conversations/{id}`); all tenant roles that can view conversations can request a summary.
- Conversation must belong to the tenant context — otherwise `404` (never `403`, no cross-tenant existence leak; matches existing conversation route behavior).

### Request

No body. `{id}` = conversation UUID.

### Response `200`

```jsonc
{
  "summary": "The customer wants a refund for order #1234. The AI explained the 30-day policy and confirmed eligibility; the customer is now waiting for the refund to be issued. Open point: customer asked whether the refund covers shipping.",
  "generatedAt": "2026-07-18T10:00:00Z",
  "messageCount": 23            // messages included in the summarized window (≤ 50)
}
```

### Errors (standard `ErrorEnvelope`)

| Status | When |
|---|---|
| `401` | no session |
| `403` | no active membership in tenant |
| `404` | conversation not found in tenant |
| `422` | conversation has no messages to summarize |
| `502` | provider generation failed (after AiService internal retries); client shows non-blocking error (spec US5-AS3) |
| `503` | AI not configured for tenant and no platform default available |

### Behavior notes

- Window: last 50 messages, existing timeline ordering; note/internal messages excluded (customer + ai + reply + system only reflect the customer-visible exchange; staff notes are not summarized).
- Deterministic neutral summary prompt (no agent persona); provider/model resolved by the same chain as the engine (tenant override if credential resolves → platform default).
- Response is not cached and not stored; repeated calls may produce different wording (documented; acceptable for staff tooling).
- Usage recorded to `ai_usage_records` like any provider call.
