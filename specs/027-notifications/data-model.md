# Phase 1 Data Model: Notifications

Conventions follow `specs/005-db-migration-foundation` (UUID PK, `tenant_id` on tenant-owned tables, `created_at`/`updated_at`, partial indexes).

---

## Table: `notifications`

Migration `0054_notifications.sql`. One row per **recipient per event** — a role-targeted event fans out to N rows (spec: Key Entities).

| Column | Type | Null | Notes |
|---|---|---|---|
| `id` | `uuid` | NO | PK, `gen_random_uuid()` |
| `tenant_id` | `uuid` | NO | FK → `tenants(id)`. Every query filters on it (FR-002) |
| `recipient_membership_id` | `uuid` | NO | FK → `tenant_memberships(id)` ON DELETE CASCADE |
| `kind` | `text` | NO | CHECK in (`escalation.new`, `conversation.assigned`, `ai.response_failed`, `tool.approval_required`) |
| `state` | `text` | NO | CHECK in (`unread`, `read`, `resolved`); default `unread` |
| `title` | `text` | NO | Display string, snapshotted at creation |
| `body` | `text` | YES | Optional secondary line |
| `subject_type` | `text` | NO | CHECK in (`conversation`, `escalation`, `tool_request`) |
| `subject_id` | `uuid` | NO | The entity the notification is about (FR-008) |
| `dedupe_key` | `text` | NO | See R4; unique per recipient |
| `actor_membership_id` | `uuid` | YES | Who caused it, when a person did. NULL for system-caused |
| `created_at` | `timestamptz` | NO | `now()` |
| `updated_at` | `timestamptz` | NO | `now()` |
| `read_at` | `timestamptz` | YES | Set when state leaves `unread` by user action |

**Recipient is a membership, not a user.** A user in two tenants has two memberships and therefore two independent inboxes, which is what makes FR-002 and the "current tenant only" rule fall out naturally instead of needing a filter that could be forgotten.

**Why the display text is snapshotted**: `title`/`body` are written at creation rather than rendered from the subject at read time. A notification is a record of *what happened then*; re-rendering would make the list depend on joins across four modules and would mutate history when an entity is renamed. It also means FR-011b (resolved notifications still render) holds even if the subject is gone.

### State transitions

```
                  user opens / marks read
        unread ──────────────────────────────▶ read
           │
           │ someone else claims or decides the work
           ▼
        resolved        (terminal; FR-011a)
```

- Only `unread` counts toward the badge (FR-006).
- `resolved` is terminal and set by the system, never by the recipient.
- `read` is terminal from the user's side. A resolve does **not** rewrite an already-`read` row — the user has seen it, and flipping it would silently rewrite what they read.
- There is no `dismissed`/`deleted` state — clarified out of scope (FR-007a); rows leave only via retention (FR-016).

### Indexes

| Index | Definition | Serves |
|---|---|---|
| `notifications_dedupe_uq` | `UNIQUE (recipient_membership_id, dedupe_key)` | FR-010 dedup + the 15-min AI-failure window (R4) |
| `notifications_inbox_idx` | `(recipient_membership_id, created_at DESC, id DESC)` | FR-005 list + cursor pagination, SC-004 |
| `notifications_unread_idx` | `(recipient_membership_id) WHERE state = 'unread'` | FR-006 unread count |
| `notifications_resolve_idx` | `(tenant_id, subject_type, subject_id) WHERE state = 'unread'` | FR-011a set-based resolve (R2) |
| `notifications_retention_idx` | `(created_at)` | FR-016 sweep |

Every write path is set-based; there is no per-recipient loop (Principle X).

---

## Outbox event contracts (internal)

Rows in the existing `outbox_events` table. Consumed **only** by the notifications worker — see R1 for why they must be private.

### `notification.requested`

```jsonc
{
  "tenantId":    "uuid",
  "kind":        "escalation.new | conversation.assigned | ai.response_failed | tool.approval_required",
  "subjectType": "conversation | escalation | tool_request",
  "subjectId":   "uuid",
  "actorMembershipId": "uuid | null",   // suppressed as a recipient (FR-009)
  "targetMembershipId": "uuid | null",  // set = deliver to exactly this member;
                                        // null = fan out by role (see below)
  "dedupeKey":   "string",
  "title":       "string",
  "body":        "string | null"
}
```

`targetMembershipId` is what distinguishes a directed notification (escalation routed to a specific agent; conversation assigned to someone) from a fan-out (queued escalation, tool approval). When null, the worker resolves recipients via `recipients.rs`.

### `notification.resolved`

```jsonc
{
  "tenantId":    "uuid",
  "subjectType": "conversation | escalation | tool_request",
  "subjectId":   "uuid",
  "resolvedByMembershipId": "uuid | null"  // their own row is left alone
}
```

Handled as one statement:

```sql
UPDATE notifications
   SET state = 'resolved', updated_at = now()
 WHERE tenant_id = $1 AND subject_type = $2 AND subject_id = $3
   AND state = 'unread'
   AND recipient_membership_id IS DISTINCT FROM $4;
```

The `state = 'unread'` predicate is what keeps a resolve from rewriting rows the user already read.

---

## Recipient resolution

Encapsulated in `recipients.rs` — the single place that reads `tenant_memberships` (see plan.md Complexity Tracking).

| Kind | Recipients | Source |
|---|---|---|
| `escalation.new` (routed) | the routed agent | `targetMembershipId` |
| `escalation.new` (queued) | active members holding `conversations.manage` | membership + role → `authz::matrix` |
| `conversation.assigned` | the new assignee | `targetMembershipId` |
| `tool.approval_required` | active members holding `conversations.manage` | membership + role → `authz::matrix` |
| `ai.response_failed` | Owners + Admins, plus the conversation's assignee if set | membership + role |

`conversations.manage` was verified as the correct predicate: it guards both `escalations::routes::claim` (`server/src/router.rs:446-447`) and `tools::routes::decide_tool_request` (`server/src/router.rs:568-570`) — exactly the two actions these fan-outs ask a recipient to take. Under the current matrix it resolves to Owner, Admin, Manager, and Agent — Viewer holds `conversations.view` only.

Two rules apply to every kind after resolution:

1. **Never the actor** (FR-009) — `actorMembershipId` is removed from the recipient set.
2. **Only active memberships** — soft-deleted or deactivated memberships are excluded, so a removed member accrues nothing (spec Edge Cases).

---

## Delivery-channel extensibility (the SC-008 model review)

SC-008 requires demonstrating, at plan time, that adding email later needs **no change to event recording or recipient resolution**. Walking the future change through this model:

A second channel is purely additive — one new table and one new worker:

```sql
-- FUTURE, not in this feature
notification_deliveries (
  id, notification_id → notifications(id) ON DELETE CASCADE,
  channel      text,   -- 'email' | future channels
  status       text,   -- 'pending' | 'sent' | 'failed' | 'skipped'
  attempts     int, last_error text, sent_at timestamptz, created_at timestamptz
)
```

An email worker selects notification rows lacking an `email` delivery row, resolves the recipient's address by joining the membership to `users`, and hands the message to the existing `EmailSender` port (the crate renamed in R7 — already an abstraction with SMTP and no-op impls, so no transport work either).

What stays untouched, and why:

| Layer | Changes for email? | Why not |
|---|---|---|
| Emit sites (5 triggers) | **No** | They emit `notification.requested`, which names *what happened*, never how it should be delivered. No channel field to add. |
| `notification.requested` payload | **No** | Channel-agnostic by construction — no `channels: []` field to extend. |
| `recipients.rs` | **No** | It answers *who should know*, which is identical regardless of channel. Email reuses the resolved recipient set verbatim. |
| `notifications` table | **No** | Delivery state lives in the child table, one row per channel attempt. Adding a `sent_via_email` column instead is exactly the design this avoids. |
| Notifications worker | **No** | It stops at persisting rows and broadcasting; the email worker is a separate consumer of the same rows. |

**In-app needs no delivery row**: the `notifications` row *is* the in-app delivery. Channels that leave the system get delivery tracking; the inbox does not, which is why no backfill is needed when email arrives.

The one thing this model does **not** cover, deliberately: per-user channel preferences and digest batching. Those are out of scope (spec Assumptions) and would add a preferences table consulted by the email worker — still additive, still no change to recording or resolution.

## Retention

`FR-016` — a sweeper deletes rows older than 90 days, modelled on the existing `widget_session_sweeper` (`server/src/main.rs:109-125`, 6-hour interval). Hard delete, not archive: the audit log is the durable record (spec Assumptions).

---

## Entity relationships

```
tenants ──1:N── notifications ──N:1── tenant_memberships   (recipient)
                     │
                     └── subject_id ─▶ conversations | escalations | tool_requests
                         (loose reference — no FK, see below)
```

`subject_id` is intentionally **not** a foreign key: it is polymorphic across three tables, and a notification must survive its subject being deleted so the "no longer available" path (FR-008 / SC-005) can be exercised rather than cascade-deleted out of existence. The dashboard resolves the link at click time and shows an unavailable state when the subject is gone.
