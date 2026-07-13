# Data Model: Conversation Core

**Feature**: 013-conversation-core | **Date**: 2026-07-13

Two migrations: `0033_conversation_core.sql` (evolve `conversations`) and `0034_messages.sql` (new append-only `messages`). Conventions per 005 (UUID PK via `gen_random_uuid()`, `created_at`/`updated_at` + `set_updated_at` trigger, soft delete via `deleted_at`, composite parent-tenant FKs per 0027) — with the `messages` append-only deviation recorded in [plan.md Complexity Tracking](./plan.md#complexity-tracking).

## Entity: Conversation (`conversations` — altered by 0033)

Existing columns (0026/0027) unchanged: `id`, `tenant_id`, `customer_id`, `channel`, `last_activity_at`, `created_at`, `updated_at`, `deleted_at`, composite FK `(tenant_id, customer_id) → customers(tenant_id, id)`, channel CHECK (`email|phone|web_chat|whatsapp|telegram`).

### 0033 changes

| Change | Definition |
|--------|------------|
| Status remap | `UPDATE conversations SET status = 'open' WHERE status = 'escalated'` (before CHECK swap) |
| Status CHECK | Drop `conversations_status_check`; re-add as `status IN ('open','pending','resolved','closed')` |
| New column | `assigned_membership_id UUID NULL` |
| Composite FK | `(tenant_id, assigned_membership_id) REFERENCES tenant_memberships(tenant_id, id)` — cross-tenant assignment unrepresentable |
| Inbox index | `(tenant_id, status, last_activity_at DESC, id DESC) WHERE deleted_at IS NULL` |
| Assignee index | `(tenant_id, assigned_membership_id, last_activity_at DESC) WHERE deleted_at IS NULL` |

### Field semantics

| Field | Rules |
|-------|-------|
| `status` | Exactly one of `open|pending|resolved|closed`; new conversations `open` (FR-003) |
| `assigned_membership_id` | `NULL` = unassigned; write-time rule: target membership must be `active` (422 otherwise); value survives assignee deactivation (edge case: past assignment stays readable, UI flags for reassignment) |
| `last_activity_at` | Bumped in the same transaction as every message insert (FR-011); inbox sort key |
| `customer_id` | Immutable after creation; conversations always render the customer's *current* display name via join (edge case: renames never break the link) |

### State transitions (status)

```
        create                    PATCH (any→any, FR-012)
   ──────────────▶ open ◀───────────────────────────────▶ pending
                    ▲ ▲                                      │
        auto-reopen │ └──────────────────────────────────────┤
   (customer-facing │            PATCH                       ▼
     message while  │ ◀─────────────────────────────▶ resolved / closed
    resolved/closed)│
```

- Manual: any→any among the four values via PATCH by a permitted member.
- Automatic (only rule): message of kind `customer` or `reply` inserted while `resolved|closed` → `open`, audited as `conversation.status_changed` with `auto: true`. Kind `note` never changes status (Q1).

## Entity: Message (`messages` — new in 0034)

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | UUID PK | `DEFAULT gen_random_uuid()` |
| `tenant_id` | UUID NOT NULL | FK → `tenants(id)` |
| `conversation_id` | UUID NOT NULL | FK → `conversations(id)`; composite FK `(tenant_id, conversation_id) → conversations(tenant_id, id)` |
| `kind` | TEXT NOT NULL | CHECK `kind IN ('customer','reply','note')` |
| `sender_membership_id` | UUID NULL | FK; composite FK `(tenant_id, sender_membership_id) → tenant_memberships(tenant_id, id)` |
| `logged_by_membership_id` | UUID NULL | FK; composite FK `(tenant_id, logged_by_membership_id) → tenant_memberships(tenant_id, id)` |
| `body` | TEXT NOT NULL | CHECK `char_length(body) BETWEEN 1 AND 10000` (app additionally rejects whitespace-only, trims trailing whitespace) |
| `seq` | BIGINT | `GENERATED ALWAYS AS IDENTITY` — deterministic tie-breaker |
| `created_at` | TIMESTAMPTZ NOT NULL | `DEFAULT now()` — the send time (FR-006) |

No `updated_at`, no `deleted_at`, no update trigger — append-only, immutable (FR-006; Complexity Tracking).

### Kind-consistency CHECKs

| Kind | `sender_membership_id` | `logged_by_membership_id` | Meaning |
|------|------------------------|---------------------------|---------|
| `customer` | must be NULL | NULL (seeded/future inbound) or set (manually logged, Q4) | Customer-authored, customer-facing |
| `reply` | must be NOT NULL | must be NULL | Member-authored, customer-facing |
| `note` | must be NOT NULL | must be NULL | Member-authored, internal-only (FR-005 — kind is the permanent internal marker) |

Expressed as one CHECK constraint enumerating the three legal column combinations.

### Indexes

| Index | Purpose |
|-------|---------|
| `(tenant_id, conversation_id, created_at DESC, seq DESC)` | Timeline keyset pages (newest-first) and the inbox LATERAL latest-message preview |

### Ordering contract (FR-007)

Display order is `created_at ASC, seq ASC`. API serves pages newest-first (`created_at DESC, seq DESC`) with an opaque keyset cursor over `(created_at, seq)`; the client prepends older pages. Same query, same order, every time — including same-instant rows (seq is unique).

## Derived: Participant (no table)

Computed on the detail read: the conversation's customer + DISTINCT `sender_membership_id` ∪ `logged_by_membership_id` (non-null) from the conversation's messages, joined to membership → user display names and membership status. Rationale: research §4.

## Referenced entities (unchanged)

- **Customer** (`customers`, 012) — conversation parent; existence checked via `customers::customer_exists_in_tx` before create; display name joined into inbox/detail payloads.
- **Tenant membership** (`tenant_memberships`, 005/011) — actor identity for senders, loggers, and assignees; `status = 'active'` gates new assignments.
- **Audit log** (`audit_logs`, 005/010) — append-only rows for `conversation.created`, `conversation.status_changed` (incl. `auto: true` reopens), `conversation.assignment_changed`.

## Validation rules (write paths)

| Operation | Rules |
|-----------|-------|
| Create conversation | customer exists in tenant (else 404 per FR-016 style); channel in fixed set (422); first message body 1–10,000 chars after trim (422); status forced `open`, assignee forced `NULL` |
| Add message | conversation exists in tenant (404); kind in set (422); body 1–10,000 after trim (422); kind `customer` with `logged_by` = acting member; kinds `reply`/`note` with sender = acting member |
| Patch status | value in fixed set (422); no-op patches (same value) allowed, not audited |
| Patch assignment | target membership exists in tenant AND `active` (422 with field detail); `null` = unassign; self-assignment allowed (Q5: any permitted member may assign anyone active) |
| All operations | tenant scope from middleware `TenantContext` only; cross-tenant ids → `not_found` (FR-016); `conversations.manage` required for all writes, `conversations.view` for reads (FR-015) |
