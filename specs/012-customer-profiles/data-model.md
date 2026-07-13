# Data Model: Customer Profiles

Six migrations (0025–0030). Conventions follow spec 005: UUID PKs (`gen_random_uuid()`), `created_at`/`updated_at TIMESTAMPTZ NOT NULL DEFAULT now()`, soft delete via `deleted_at TIMESTAMPTZ NULL`, partial unique indexes scoped to live rows, append-only audit, composite FK constraints, cascade trigger.

## Migration 0025 — `customers` + `customer_channel_identifiers`

### Table: `customers`

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | UUID | PK, default `gen_random_uuid()` |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` |
| `display_name` | TEXT | NOT NULL, CHECK `length(display_name) BETWEEN 1 AND 200` |
| `email` | CITEXT | NULL (contact email; format validated app-side, mirroring 0003) |
| `phone` | TEXT | NULL (normalized `+`-prefixed digits; format validated app-side) |
| `metadata` | JSONB | NOT NULL DEFAULT `'{}'::jsonb`, CHECK `jsonb_typeof(metadata) = 'object'` (≤50 keys, key ≤100 chars, string values ≤500 chars enforced app-side; deviation justified in plan.md Complexity Tracking) |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |
| `deleted_at` | TIMESTAMPTZ | NULL (soft delete; no delete API in this feature) |

**Indexes**
- `customers_tenant_cursor_idx` btree `(tenant_id, created_at DESC, id DESC)` WHERE `deleted_at IS NULL` — list/search keyset cursor.
- `customers_display_name_trgm_idx` GIN `display_name gin_trgm_ops` — infix search (`CREATE EXTENSION IF NOT EXISTS pg_trgm`).
- `customers_email_trgm_idx` GIN `(email::text) gin_trgm_ops` — infix search on contact email.

**Invariants (app-enforced)**
- Create requires `display_name` + at least one of contact email, contact phone, or one channel identifier (FR-007).
- Reads and writes always filter `tenant_id = <resolved tenant>` AND `deleted_at IS NULL` (FR-001/FR-011).
- Update is a partial UPDATE; last write wins (spec edge case); `updated_at` refreshed on every change (FR-008).

### Table: `customer_channel_identifiers`

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | UUID | PK, default `gen_random_uuid()` |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` |
| `customer_id` | UUID | NOT NULL, FK → `customers(id)` |
| `channel` | TEXT | NOT NULL, CHECK `channel IN ('email','phone','web_chat','whatsapp','telegram')` |
| `identifier` | TEXT | NOT NULL, CHECK `length(identifier) BETWEEN 1 AND 320` (normalized app-side: trimmed; email lowercased; phone/WhatsApp E.164-style) |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |

**Indexes**
- `customer_channel_identifiers_unique_idx` UNIQUE `(tenant_id, channel, identifier)` — FR-003/FR-014 conflict source; also the future inbound-matching lookup path.
- `customer_channel_identifiers_customer_idx` btree `(customer_id)` — profile fetch + EXISTS search arm.

**Notes**
- Identifier rows are hard-deleted on update (replaced set semantics with the customer PATCH); the parent customer soft-deletes. `tenant_id` is denormalized onto the child (matches platform convention: every tenant-owned table carries `tenant_id`, Constitution II) and must always equal the parent's — enforced by writing both in one transaction from the same resolved tenant.
- Relationship: `customers 1 — N customer_channel_identifiers`; a (channel, identifier) pair maps to at most one customer per tenant.

## Migration 0026 — `conversations` (minimal summary)

Owned by the `conversations` module; this feature ships read-only summary fields as the extension point for future messaging features (clarification Q1).

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | UUID | PK, default `gen_random_uuid()` |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` |
| `customer_id` | UUID | NOT NULL, FK → `customers(id)` |
| `channel` | TEXT | NOT NULL, CHECK `channel IN ('email','phone','web_chat','whatsapp','telegram')` |
| `status` | TEXT | NOT NULL, CHECK `status IN ('open','escalated','closed')` (matches existing dashboard `ConversationStatus` vocabulary; future features extend by migration) |
| `last_activity_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |
| `deleted_at` | TIMESTAMPTZ | NULL |

**Indexes**
- `conversations_customer_recent_idx` btree `(tenant_id, customer_id, last_activity_at DESC)` WHERE `deleted_at IS NULL` — history query (top 20 + has_more).

**Invariants**
- No write API in this feature; rows are seeded by tests (FR-016).
- History reads filter by resolved tenant AND customer; a cross-tenant customer id yields `not_found` before the history query runs.

## Entity relationships

```text
tenants 1 ──── N customers 1 ──── N customer_channel_identifiers
                    │
                    └──── N conversations (summary; read-only in this feature)

audit_logs (existing, append-only) ← customer.created / customer.updated entries
```

## State transitions

- **Customer**: `live → soft-deleted` (transition itself out of scope; column reserved per platform convention). No other lifecycle states.
- **Conversation summary**: status values are data, not managed transitions, in this feature (no write API).

## Audit records (existing `audit_logs` table)

| Action | When | Payload |
|--------|------|---------|
| `customer.created` | successful POST | actor, tenant, resource = customer id, timestamp |
| `customer.updated` | successful PATCH | actor, tenant, resource = customer id, `changed_fields` (field names only — no values, keeping contact PII out of the append-only log), timestamp |

Both inserts occur inside the write transaction (atomic with the change, per the tenancy audit pattern).

## Migration 0027 — Composite FK customer children

Adds composite foreign key constraints on `customer_channel_identifiers` and `conversations` that pair `tenant_id` with the `customer_id` / `id` reference:

- `customer_channel_identifiers_parent_tenant_fkey`: `(tenant_id, customer_id)` references `customers(tenant_id, id)`.
- `conversations_parent_tenant_fkey`: `(tenant_id, customer_id)` references `customers(tenant_id, id)`.

These prevent cross-tenant child rows at the database level — any insert or update that would create a child referencing a customer from a different tenant is rejected.

## Migration 0028 — Customer search indexes

Adds GIN trigram indexes for infix search on `display_name` and `email` (requires `pg_trgm` extension):

- `customers_display_name_trgm_idx`: GIN `(display_name gin_trgm_ops)`.
- `customers_email_trgm_idx`: GIN `((email::text) gin_trgm_ops)`.

Both cover the `WHERE tenant_id = ? AND deleted_at IS NULL` scoping filter via the `customers_tenant_cursor_idx` btree index; the GIN indexes handle the `ILIKE` / `%` fragment match.

## Migration 0029 — Identifier soft delete

Adds `deleted_at TIMESTAMPTZ NULL` to `customer_channel_identifiers` and replaces the unconditional unique index with a partial unique index that only enforces uniqueness over live (non-deleted) rows:

- Old: `customer_channel_identifiers_unique_idx` UNIQUE `(tenant_id, channel, identifier)`.
- New: `customer_channel_identifiers_live_unique_idx` UNIQUE `(tenant_id, channel, identifier)` WHERE `deleted_at IS NULL`.

This allows identifier rows to be soft-deleted (e.g. when a customer is removed) without violating the uniqueness constraint, while still preventing duplicate live identifiers.

## Migration 0030 — Customer identifier cascade

Adds a database-level trigger function that, on customer soft-delete (`customers.deleted_at` set to non-NULL), automatically stamps `deleted_at = NOW()` on all child `customer_channel_identifiers` rows belonging to that customer. Conversations are **not** cascade-deleted — they are retained for historical reference.

## Migration 0032 — Identifier normalization reconciliation

A one-time data reconciliation migration that normalizes any pre-existing
`customer_channel_identifiers` rows to the canonical form defined in
research.md (R3):

- **Trim**: All identifier values are trimmed of leading/trailing whitespace.
- **Email lowercase**: Email-channel identifiers are lowercased for
  case-insensitive matching.
- **E.164 phone/WhatsApp**: Phone and WhatsApp identifiers have all
  non-digit characters stripped (except the leading `+`).

Also normalizes the `customers.phone` contact field to E.164 form.

This migration is safe to run on an empty or development database — it is
idempotent (no change on already-canonical rows) and applies before the
partial unique index (0029) enforces live-row uniqueness, preventing false
conflicts from formatting differences.
