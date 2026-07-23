# Data Model: Integrations Foundation

Migration: `backend/migrations/0056_integrations_foundation.sql`. Conventions per 005: UUID PKs (`gen_random_uuid()`), `created_at`/`updated_at` timestamptz, `tenant_id` on every tenant-owned table, indexes on production query paths.

## Tables

### `integration_catalog` (global — platform-managed, no tenant_id)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | uuid | PK | |
| slug | text | NOT NULL, UNIQUE | stable identifier used in URLs (`generic-webhook`) |
| name | text | NOT NULL | display name |
| description | text | NOT NULL | |
| category | text | NOT NULL | e.g. `messaging`, `crm`, `automation` |
| is_available | boolean | NOT NULL DEFAULT false | false ⇒ "coming soon", connect rejected |
| config_schema | jsonb | NOT NULL DEFAULT '[]' | array of `{key, label, kind: "text"\|"secret", required}` |
| created_at / updated_at | timestamptz | NOT NULL DEFAULT now() | |

Seed (in migration): `generic-webhook` (available, schema: `source_label` text required, `signing_secret` secret required) + `slack`, `microsoft-teams`, `crm` (is_available = false, empty schema).

### `integration_connections` (tenant-owned)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | uuid | PK | |
| tenant_id | uuid | NOT NULL, FK tenants | |
| catalog_id | uuid | NOT NULL, FK integration_catalog | |
| is_active | boolean | NOT NULL | lifecycle: connected (true) / disconnected (false) |
| config | jsonb | NOT NULL DEFAULT '{}' | non-secret field values only |
| webhook_token_hash | bytea | NOT NULL | SHA-256 of the intake token — used for the intake lookup; rotated on reconnect |
| webhook_token_ciphertext | bytea | NOT NULL | AES-256-GCM of the same token, so the detail page can redisplay the URL |
| webhook_token_nonce | bytea | NOT NULL | 12 bytes |
| connected_at | timestamptz | NOT NULL | last (re)connect time |
| connected_by_membership_id | uuid | NULL | actor of last (re)connect |
| disconnected_at | timestamptz | NULL | |
| disconnected_by_membership_id | uuid | NULL | |
| created_at / updated_at | timestamptz | NOT NULL DEFAULT now() | |

Constraints/indexes:
- `UNIQUE (tenant_id, catalog_id)` — one connection record per tenant+integration, forever (reconnect reactivates; spec FR-004).
- `UNIQUE (webhook_token_hash)` — token resolves to exactly one connection.
- Index `(tenant_id)` for list join.

### `integration_secrets` (tenant-owned)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | uuid | PK | |
| tenant_id | uuid | NOT NULL | denormalized for isolation checks/sweeps |
| connection_id | uuid | NOT NULL, FK integration_connections ON DELETE CASCADE | |
| field_key | text | NOT NULL | matches a `kind:"secret"` key in config_schema |
| ciphertext | bytea | NOT NULL | AES-256-GCM |
| nonce | bytea | NOT NULL | 12 bytes |
| hint | text | NOT NULL | last ≤4 chars of plaintext, the only readable remnant |
| created_at / updated_at | timestamptz | NOT NULL DEFAULT now() | updated on rotation |

Constraints: `UNIQUE (connection_id, field_key)`. Rotation = upsert of the row (old ciphertext replaced). Disconnect deletes rows (secrets "no longer usable", FR-006); reconnect requires fresh values.

### `integration_webhook_deliveries` (tenant-owned)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | uuid | PK | |
| tenant_id | uuid | NOT NULL | |
| connection_id | uuid | NOT NULL, FK integration_connections ON DELETE CASCADE | |
| payload | jsonb | NOT NULL | accepted raw body (JSON only; non-JSON rejected) |
| received_at | timestamptz | NOT NULL DEFAULT now() | |

Indexes: `(connection_id, received_at DESC)`; `(received_at)` for retention sweep. Only **accepted** deliveries are stored (rejections exist as events only). Retained 90 days.

### `integration_events` (tenant-owned)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | uuid | PK | |
| tenant_id | uuid | NOT NULL | |
| connection_id | uuid | NOT NULL, FK integration_connections ON DELETE CASCADE | |
| event_type | text | NOT NULL | see enum below |
| outcome | text | NOT NULL | `success` \| `failure` |
| reason | text | NULL | failure reason category — never secret material |
| actor_membership_id | uuid | NULL | for lifecycle events |
| created_at | timestamptz | NOT NULL DEFAULT now() | |

Indexes: `(connection_id, created_at DESC, id)` — keyset pagination + status derivation; `(created_at)` — retention sweep. Retained 90 days.

## Enums (Rust `model.rs`, serialized as snake_case strings)

- **EventType**: `connected`, `config_updated`, `secret_rotated`, `disconnected`, `delivery_accepted`, `delivery_rejected`
- **RejectionReason** (event `reason` for `delivery_rejected`): `invalid_signature`, `inactive_connection`, `payload_too_large`, `rate_limited`, `malformed_payload`
- **ConnectionStatus** (derived, never stored): `not_connected` (no row), `connected`, `error`, `disconnected`

## State transitions

```
(no row) --connect--> connected --disconnect--> disconnected --reconnect--> connected
connected --update config / rotate secret--> connected (events recorded)
connected --3 most recent deliveries in 24h all rejected--> error (derived, read-time)
error --any accepted delivery / config fix--> connected (derived)
```

Rules:
- Connect on active connection → 409 (use update endpoint instead).
- Connect on `is_available = false` catalog entry → 422.
- Reconnect reactivates the same row: `is_active = true`, new `webhook_token_hash`, fresh secrets mandatory, `connected_at`/`connected_by` updated; event history untouched.
- Disconnect: `is_active = false`, secrets rows deleted, token stays but resolves to an inactive connection (rejected with generic 404 + `inactive_connection` event).

## Validation rules

- Connect/update payload validated against `config_schema`: unknown keys rejected; `required` fields must be present (non-empty); `kind:"secret"` values accepted only via write-only fields and stored encrypted; non-secret values stored in `connections.config`.
- Webhook intake: body ≤ 256 KB, must parse as JSON, HMAC-SHA256 over raw body must match `X-Webhook-Signature: sha256=<hex>` computed with the connection's `signing_secret`.
- Cursor params validated exactly like audit logs (`limit` clamp 1–100, opaque `(created_at,id)` cursor).

## Audit trail (existing `audit_logs` table — no schema change)

Actions written transactionally with the state change: `integration.connected`, `integration.config_updated`, `integration.secret_rotated`, `integration.disconnected`. The audit module's category derivation gains the `integration.` prefix mapping (new `integrations` category). Metadata carries catalog slug + connection id — never config values or secrets.
