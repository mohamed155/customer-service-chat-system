# Data Model: WhatsApp Channel (029)

Single migration: `backend/migrations/0057_whatsapp_channel.sql`. No changes to `conversations`, `messages`, or `customer_channel_identifiers` schemas — their existing `whatsapp` channel vocabulary (0025/0026) is used as-is. Conventions follow the repo standard: UUID PKs, `tenant_id` on every tenant-owned table, composite FKs to `(tenant_id, id)` targets where the parent supports it, timestamptz timestamps.

## 1. Catalog seed (existing table `integration_catalog`)

Insert one connectable row:

| Column | Value |
|--------|-------|
| slug | `whatsapp` |
| name | `WhatsApp` |
| category | `messaging` |
| is_available | `true` |
| config_schema | `[{key: phone_number_id, kind: text, required}, {key: business_phone, kind: text, required}, {key: access_token, kind: secret, required}, {key: app_secret, kind: secret, required}, {key: verify_token, kind: secret, required}]` |

Connection, secrets (AES-256-GCM), webhook token, health status, and event log all come from existing 028 tables (`integration_connections`, `integration_secrets`, `integration_webhook_deliveries`, `integration_events`) with no schema change. One connection per `(tenant, whatsapp)` is already enforced by `integration_connections_tenant_catalog_uq`, satisfying "one number per tenant".

## 2. `whatsapp_message_meta` (new)

Channel-specific side record for every WhatsApp-channel `messages` row (mirrors how `message_citations` extends messages).

| Column | Type | Constraints |
|--------|------|-------------|
| id | uuid PK | default gen_random_uuid() |
| tenant_id | uuid NOT NULL | REFERENCES tenants(id) |
| message_id | uuid NOT NULL | composite FK `(tenant_id, message_id)` → `messages(tenant_id, id)`* |
| conversation_id | uuid NOT NULL | composite FK `(tenant_id, conversation_id)` → `conversations(tenant_id, id)` |
| direction | text NOT NULL | CHECK IN ('inbound','outbound') |
| wamid | text NULL | provider message id; set on inbound at insert, on outbound once Graph accepts |
| provider_timestamp | timestamptz NULL | inbound only; Meta's message timestamp |
| delivery_status | text NULL | outbound only; CHECK IN ('pending','sent','delivered','read','failed') |
| failure_reason | text NULL | human-readable; set when delivery_status='failed' |
| created_at / updated_at | timestamptz NOT NULL | default now(); updated_at trigger |

CHECK: `(direction='inbound' AND delivery_status IS NULL) OR (direction='outbound' AND delivery_status IS NOT NULL)`.

*Requires `messages_tenant_id_id_uq` UNIQUE (tenant_id, id) on `messages` — added in this migration (same idempotent DO-block pattern 0034 used for conversations).

Indexes:
- `UNIQUE (tenant_id, wamid) WHERE wamid IS NOT NULL` — inbound dedupe (race-safe) and status-webhook lookup.
- `UNIQUE (message_id)` — one meta row per message; batch timeline join path.
- `(tenant_id, conversation_id)` — timeline batch load.

State transitions (outbound `delivery_status`, monotonic — updates may only move rightward; late/duplicate status webhooks that would move leftward are ignored):

```
pending → sent → delivered → read
   └────────┴────────┴→ failed (terminal, with failure_reason)
```

## 3. `message_attachments` (new, channel-generic)

Stored media attached to a message. Deliberately not WhatsApp-named so future channels (email) reuse it.

| Column | Type | Constraints |
|--------|------|-------------|
| id | uuid PK | default gen_random_uuid() |
| tenant_id | uuid NOT NULL | REFERENCES tenants(id) |
| message_id | uuid NOT NULL | composite FK `(tenant_id, message_id)` → `messages(tenant_id, id)` |
| kind | text NOT NULL | CHECK IN ('image','audio','video','document') |
| status | text NOT NULL | CHECK IN ('pending','stored','failed'); default 'pending' |
| provider_media_id | text NULL | Meta media id (fetch handle) |
| storage_key | text NULL | S3 key `whatsapp-media/{tenant_id}/{id}`; set when stored |
| mime_type | text NULL | from Graph media metadata; set when stored |
| size_bytes | bigint NULL | set when stored |
| file_name | text NULL | documents: original filename when Meta provides it |
| fetch_attempts | int NOT NULL | default 0; bounded retry counter |
| created_at / updated_at | timestamptz NOT NULL | default now(); updated_at trigger |

CHECK: `(status='stored' AND storage_key IS NOT NULL AND mime_type IS NOT NULL) OR status IN ('pending','failed')`.

Indexes:
- `(tenant_id, message_id)` — timeline batch load.
- `(status, updated_at) WHERE status='pending'` — media-worker claim scan.

State transitions: `pending → stored` (terminal) or `pending → failed` (terminal after retry budget; message row keeps its typed placeholder body).

## 4. Existing entities touched (no schema change)

| Entity | Use |
|--------|-----|
| `customers` + `customer_channel_identifiers` | Identity resolution (R8): lookup `channel IN ('whatsapp','phone')` by normalized E.164 identifier; insert `whatsapp` identifier rows. Live-unique partial index already enforces `(tenant_id, channel, identifier)`. |
| `conversations` | `channel='whatsapp'` rows via existing `create_conversation_in_tx`; open-conversation reuse per R9. |
| `messages` | Inbound: kind `customer`. Agent replies: kind `reply`. AI: kind `ai`. Body ≤ 10000 chars (existing CHECK) — also under WhatsApp's 4096-char outbound limit check done in validation, not schema. |
| `outbox_events` | Existing `conversation.customer_message` (inbound trigger, unchanged) + new event type `whatsapp.outbound_message` (payload: tenant_id, conversation_id, message_id). No schema change; table is generic. |
| `integration_*` (028) | Connection, secrets, deliveries, events, retention sweeps — reused unchanged. Delivery/event rows for WhatsApp participate in the existing 90-day sweeper automatically. |

## 5. Retention

- `integration_webhook_deliveries` / `integration_events`: existing 90-day sweep covers WhatsApp rows (no work).
- `whatsapp_message_meta`, `message_attachments`, stored media objects: follow conversation/message lifetime (no independent sweep in v1; deleted with tenant/conversation deletion paths if/when those cascade — matches messages' current behavior).
