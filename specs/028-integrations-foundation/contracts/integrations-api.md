# API Contract: Integrations Foundation

All tenant endpoints: standard auth (session cookie) + tenant context (`X-Tenant-ID`), standard `ApiError` envelope, registered in the utoipa OpenAPI document (enforced by `openapi_coverage.rs`). Permissions: `integrations.view` for reads, `integrations.manage` for writes.

## JSON casing convention (VERIFIED against code — follow exactly)

The codebase is mixed, so this feature pins one convention: **copy the audit (026) + notifications (027) style**, since those are the two most recent modules and are structurally identical (cursor-paginated list + detail).

- DTO structs derive `Serialize, ToSchema` with **NO `#[serde(rename_all)]`** ⇒ fields are **snake_case on the wire** (verified: `backend/crates/modules/audit/src/model.rs`, `backend/crates/modules/notifications/src/model.rs`).
- List responses use the envelope `{ "data": [...], "pagination": { "next_cursor": …, "has_more": … } }`.
- Single-resource responses are the bare DTO object (no wrapper).
- Do **not** copy the `widgets` module's camelCase DTOs for this feature.

Frontend note: `ApiService` sets `ApiResponse<T>.data` to the entire raw HTTP body, so `api.get<IntegrationListWire>('/tenant/integrations')` yields `{ data: { data: [...] } }`. Wire interfaces mirror the snake_case body; `*FromWire` mappers convert to camelCase frontend models (same as `auditListFromWire`).

## Tenant endpoints

### `GET /tenant/integrations` — list catalog with tenant status

Permission: `integrations.view`. No pagination (catalog is small).

Response `200`:

```json
{
  "data": [
    {
      "slug": "generic-webhook",
      "name": "Generic Webhook",
      "description": "Receive events from any system that can send signed webhooks.",
      "category": "automation",
      "is_available": true,
      "status": "connected"
    },
    {
      "slug": "slack",
      "name": "Slack",
      "description": "Coming soon.",
      "category": "messaging",
      "is_available": false,
      "status": "not_connected"
    }
  ]
}
```

`status` ∈ `not_connected` | `connected` | `error` | `disconnected` (derived, never stored).

### `GET /tenant/integrations/{slug}` — detail

Permission: `integrations.view`

Response `200` (bare object):

```json
{
  "slug": "generic-webhook",
  "name": "Generic Webhook",
  "description": "…",
  "category": "automation",
  "is_available": true,
  "status": "connected",
  "config_schema": [
    { "key": "source_label", "label": "Source label", "kind": "text", "required": true },
    { "key": "signing_secret", "label": "Signing secret", "kind": "secret", "required": true }
  ],
  "connection": {
    "config": { "source_label": "Billing system" },
    "secrets": [ { "field_key": "signing_secret", "hint": "3XYZ" } ],
    "webhook_url": "https://api.example.com/hooks/v1/<token>",
    "connected_at": "2026-07-22T10:00:00Z",
    "disconnected_at": null
  }
}
```

Rules: `connection` is `null` when status is `not_connected`. `secrets[].hint` is the last ≤4 characters — the **only** readable remnant of a secret; raw secret values are never returned. `webhook_url` is non-null whenever the connection is active and `null` once disconnected; the token inside it is stored hashed (for intake lookup) **and** encrypted (so this endpoint can decrypt and redisplay it). *Build note*: during User Story 1 this field is hardcoded to `null` because no connect endpoint exists yet to mint a token; task T035 in User Story 2 implements the behaviour described here.

`404` unknown slug.

### `POST /tenant/integrations/{slug}/connect`

Permission: `integrations.manage`

Request:

```json
{ "config": { "source_label": "Billing system" }, "secrets": { "signing_secret": "whsec_abc123XYZ" } }
```

Responses:
- `201` → detail body above (including freshly generated `webhook_url`). Creates or reactivates the single connection row; records a `connected` event; writes an `integration.connected` audit row in the same transaction.
- `409` already actively connected.
- `422` catalog entry not available (a "coming soon" entry or one the platform has retired), unknown/missing field keys, or empty required values. This availability gate applies to connect/reconnect **only** — an existing connection to a since-retired entry keeps working and stays updatable and disconnectable.
- `404` unknown slug.

### `PUT /tenant/integrations/{slug}/config`

Permission: `integrations.manage`

Request: same shape as connect. `secrets` is optional — only the keys present are rotated (old ciphertext replaced).

Responses: `200` detail body; `409` if not actively connected; `422` validation failure. Records `config_updated` and/or `secret_rotated` events plus matching audit rows.

### `POST /tenant/integrations/{slug}/disconnect`

Permission: `integrations.manage`

Responses: `200` detail body with `status: "disconnected"`, `connection.secrets: []`, `connection.webhook_url: null`; `409` if not actively connected. Deletes secret rows, sets `is_active = false`, records a `disconnected` event + audit row.

### `GET /tenant/integrations/{slug}/events?cursor=&limit=`

Permission: `integrations.view`. Cursor pagination identical to `GET /tenant/audit-logs` (`limit` clamped 1–100, default 50; opaque keyset cursor over `(created_at, id)`; newest first).

Response `200`:

```json
{
  "data": [
    {
      "id": "…",
      "event_type": "delivery_rejected",
      "outcome": "failure",
      "reason": "invalid_signature",
      "actor_membership_id": null,
      "created_at": "2026-07-22T10:05:00Z"
    }
  ],
  "pagination": { "next_cursor": null, "has_more": false }
}
```

## Public endpoint (no auth, mounted beside the widget public router)

### `POST /hooks/v1/{token}` — webhook intake

Headers: `Content-Type: application/json`, `X-Webhook-Signature: sha256=<hex HMAC-SHA256 of the raw body, keyed by the connection's signing_secret>`.

Behavior: resolve SHA-256(token) → active connection; verify HMAC in constant time; enforce the 256 KB body cap and the 60/min per-connection rate limit; store the payload in `integration_webhook_deliveries`; record a `delivery_accepted` event; acknowledge. No synchronous downstream processing.

Responses:
- `202` `{"status":"accepted"}`
- `404` generic, byte-identical body for: unknown token, inactive connection, bad signature — no existence leak. Inactive-connection and bad-signature cases on a real connection additionally record a `delivery_rejected` event (`inactive_connection` / `invalid_signature`). Unknown tokens record nothing.
- `413` payload too large (records `payload_too_large` when the connection is identifiable)
- `429` rate limited (records `rate_limited`, throttled — see below)
- `422` non-JSON body on a signature-verified connection (records `malformed_payload`)

**Rejection-event throttling**: the `rate_limited` and `inactive_connection` reasons are reachable without a valid signature, so at most **one event row per connection per reason per minute** is written; the HTTP status is always returned regardless. Without this, a flood of rejected requests would generate unbounded database writes precisely when the rate limit is meant to shed load. The `invalid_signature` and `malformed_payload` reasons are not throttled — they are only reachable after the 60/min limit has already been applied.

## RBAC matrix (existing — unchanged)

| Role | list / detail / events | connect / update / disconnect |
|------|------------------------|-------------------------------|
| Owner, Admin, Manager | ✅ | ✅ |
| Viewer, staff-production Developer | ✅ | ❌ 403 |
| Agent | ❌ 403 | ❌ 403 |

## Frontend wire contract

In `core/api/tenant-api.models.ts`: `IntegrationListItemWire`, `IntegrationListWire`, `IntegrationDetailWire`, `IntegrationConnectionWire`, `IntegrationSecretRefWire`, `IntegrationConfigFieldWire`, `IntegrationEventWire`, `IntegrationEventListWire` (all snake_case, mirroring the bodies above) plus mappers `integrationListFromWire`, `integrationDetailFromWire`, `integrationEventListFromWire` producing camelCase frontend models.
