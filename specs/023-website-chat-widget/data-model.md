# Data Model: Website Chat Widget

Migration: `backend/migrations/0050_website_chat_widget.sql`. Conventions per spec 005: UUID PKs (`gen_random_uuid()`), `created_at`/`updated_at` timestamptz, soft delete via `deleted_at`, partial unique indexes excluding soft-deleted rows, `tenant_id` on every tenant-owned table (constitution II).

## widget_instances

One row per embeddable widget a tenant owns (US5 — multiple instances per tenant).

| Column | Type | Constraints | Notes |
|---|---|---|---|
| id | uuid | PK | |
| tenant_id | uuid | NOT NULL, FK → tenants | |
| public_id | text | NOT NULL | `wgt_` + 22 base62 chars; partial UNIQUE where `deleted_at IS NULL` (globally unique — it is the tenant lookup key) |
| name | text | NOT NULL | Internal label shown in dashboard ("Marketing site") |
| display_name | text | NOT NULL DEFAULT 'Support' | Shown in widget header |
| primary_color | text | NOT NULL DEFAULT '#4F46E5' | Validated `#rrggbb` |
| welcome_message | text | NOT NULL DEFAULT 'Hi! How can we help?' | |
| position | text | NOT NULL DEFAULT 'bottom-right' | CHECK in ('bottom-right','bottom-left') |
| theme | text | NOT NULL DEFAULT 'light' | CHECK in ('light','dark') |
| enabled | boolean | NOT NULL DEFAULT true | Disabled → config endpoint returns disabled → widget renders nothing (FR-005) |
| allowed_domains | text[] | NOT NULL DEFAULT '{}' | Empty = any origin; entries are hosts, `*.` prefix allowed (FR-026) |
| created_at / updated_at | timestamptz | NOT NULL DEFAULT now() | |
| deleted_at | timestamptz | NULL | Soft delete |

Indexes: `(tenant_id)` where `deleted_at IS NULL`; partial unique `(public_id)` where `deleted_at IS NULL`.

Validation (service layer): name/display_name 1–80 chars; welcome_message ≤ 500; allowed_domains entries valid hostnames (optionally `*.`-prefixed), ≤ 20 entries.

## widget_sessions

Anonymous visitor sessions (US4). No PII.

| Column | Type | Constraints | Notes |
|---|---|---|---|
| id | uuid | PK | Also the session's public reference used as the customer channel identifier |
| tenant_id | uuid | NOT NULL, FK → tenants | Denormalized from instance for direct scoping |
| widget_instance_id | uuid | NOT NULL, FK → widget_instances | |
| token_hash | bytea | NOT NULL, UNIQUE | SHA-256 of the opaque bearer token; raw token never stored |
| customer_id | uuid | NULL, FK → customers | Set lazily when the first conversation is created (R8) |
| last_seen_at | timestamptz | NOT NULL DEFAULT now() | Refreshed on each authenticated public call |
| expires_at | timestamptz | NOT NULL | Sliding window: `last_seen_at + 24h`; expired sessions reject with 401 → widget silently mints a new session (FR-010) |
| created_at | timestamptz | NOT NULL DEFAULT now() | |

Indexes: unique `(token_hash)`; `(tenant_id, widget_instance_id)`; `(expires_at)` for the sweep job.

## conversations (altered)

| Change | Notes |
|---|---|
| ADD COLUMN `widget_instance_id` uuid NULL FK → widget_instances | Attribution per FR-032; NULL for non-widget channels. Index `(tenant_id, widget_instance_id)` where not null. |

Widget conversations use existing fields: `channel = 'widget'`, existing `status` lifecycle, existing `ai_handling`. No parallel conversation table (R8).

## customers (no schema change)

Anonymous visitor → one `customers` row per session at first conversation: `display_name = 'Visitor ' || short_code`, no email/phone, channel identifier row `(channel='widget', identifier=<session id>)` via the existing channel-identifier mechanism.

## Relationships

```text
tenants 1─* widget_instances 1─* widget_sessions *─1 customers (lazy, nullable)
widget_instances 1─* conversations (attribution, nullable FK)
widget_sessions ~ conversations: session's conversations = conversations of its customer_id
                                  (current conversation = newest with status not in resolved/closed)
```

## State transitions (visitor-facing, derived — no new state columns)

`handling` in the public conversation view is derived per R9:

```text
ai      : ai_handling says AI is handling, status open/pending
human   : escalated/assigned to human (widget shows handoff; + team_online=false → away variant)
closed  : status resolved|closed (locked per FR-027; next visitor message → create new conversation)
```

Transitions are owned entirely by existing modules (021 engine decisions, 014 escalation/routing, dashboard status changes); the widgets module only reads them and relays changes over SSE (`conversation.updated`).

## Session lifecycle

```text
mint (POST /sessions, rate-limited per IP)
  → active (each call: verify token hash + expires_at, refresh last_seen_at/expires_at)
  → expired (expires_at < now(): 401; periodic sweep deletes rows; widget re-mints and starts fresh)
```

Expired sessions orphan nothing: customers/conversations persist for the tenant dashboard; only the visitor's ability to resume ends (matches FR-010 and spec assumption on continuity).
