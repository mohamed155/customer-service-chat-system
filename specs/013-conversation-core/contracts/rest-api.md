# REST API Contract: Conversation Core

All endpoints are tenant-scoped, mounted under the existing `/tenant` family (tenant resolved by middleware from the authenticated session + `X-Tenant-ID` contract), registered deny-by-default via `.guarded()`. Envelope, pagination, and error formats follow `specs/001-ai-customer-service-platform/contracts/rest-api.md`: success bodies are `ApiResponse<T>` (`{ "data": … }`), lists are `PaginatedResponse<T>` (`{ "data": [...], "pagination": { "next_cursor": string|null, "has_more": bool } }`), errors use the standard envelope with `code`/`message`/`details[]`, and every response carries `X-Request-Id`.

Vocabularies: `channel ∈ email | phone | web_chat | whatsapp | telegram` · `status ∈ open | pending | resolved | closed` · `message kind ∈ customer | reply | note`.

> Supersedes the status note in `specs/012-customer-profiles/contracts/rest-api.md` (`open|escalated|closed`): migration 0033 remaps `escalated → open` and all conversation responses — including the 012 history endpoint — now use the vocabulary above.

## Resource representations

### Conversation (inbox item)

```json
{
  "id": "uuid",
  "customer": { "id": "uuid", "display_name": "Sara Ali" },
  "channel": "web_chat",
  "status": "open",
  "assignee": { "membership_id": "uuid", "display_name": "Omar Farouk", "active": true },
  "last_message": { "kind": "reply", "preview": "Thanks — checking now…" },
  "last_activity_at": "2026-07-13T09:30:00Z",
  "created_at": "2026-07-12T14:00:00Z"
}
```

`assignee` and `last_message` are `null` when unassigned / no messages. `preview` is the body truncated server-side to 140 chars. `assignee.active: false` signals a deactivated assignee (UI flags for reassignment).

### Conversation (detail)

Inbox item fields plus:

```json
{
  "participants": [
    { "type": "customer", "id": "uuid", "display_name": "Sara Ali" },
    { "type": "member", "membership_id": "uuid", "display_name": "Omar Farouk", "active": true }
  ]
}
```

### Message

```json
{
  "id": "uuid",
  "kind": "customer",
  "sender": { "type": "customer", "display_name": "Sara Ali" },
  "logged_by": { "membership_id": "uuid", "display_name": "Omar Farouk" },
  "body": "My order hasn't arrived.",
  "created_at": "2026-07-13T09:30:00Z"
}
```

- `kind: "reply" | "note"` → `sender` is `{ "type": "member", "membership_id": "uuid", "display_name": "…" }`, `logged_by` is `null`.
- `kind: "customer"` → `sender.type` is `customer`; `logged_by` is `null` (inbound/seeded) or the logging member (manually logged — Q4).

## Endpoints

### `GET /tenant/conversations` — inbox list

**Permission**: `conversations.view`

| Query param | Type | Notes |
|-------------|------|-------|
| `status` | string, optional | One of the status set, or `all`. **Default `open`** (Q2) |
| `assignee` | string, optional | `me` \| `unassigned` \| membership uuid |
| `channel` | string, optional | One of the channel set |
| `cursor` | string, optional | Opaque keyset cursor from previous page |
| `limit` | int, optional | Default 25, max 100 |

**200** → `PaginatedResponse<Conversation>` ordered `last_activity_at DESC, id DESC`. Unknown `status`/`channel`/`assignee` values → `422 validation_failed`. Empty result → `data: []` (never an error).

### `POST /tenant/conversations` — create (US5)

**Permission**: `conversations.manage`

```json
{
  "customer_id": "uuid",              // required; must exist in tenant
  "channel": "web_chat",              // required; channel set
  "message": { "body": "Hello!" }     // required; first entry, kind "reply", 1–10000 chars after trim
}
```

**201** → `ApiResponse<ConversationDetail>` — status `open`, assignee `null`, the message as first timeline entry. Writes `conversation.created` audit row in the same transaction. Unknown/cross-tenant `customer_id` → `404 not_found` (indistinguishable, FR-016). No uniqueness constraint: any number of concurrent conversations per customer/channel (Q3).

### `GET /tenant/conversations/{conversation_id}` — detail

**Permission**: `conversations.view`

**200** → `ApiResponse<ConversationDetail>` (participants included; messages fetched separately below).

### `GET /tenant/conversations/{conversation_id}/messages` — timeline

**Permission**: `conversations.view`

| Query param | Type | Notes |
|-------------|------|-------|
| `cursor` | string, optional | Opaque keyset cursor over `(created_at, seq)` |
| `limit` | int, optional | Default 50, max 200 |

**200** → `PaginatedResponse<Message>` served newest-first (`created_at DESC, seq DESC`); client renders ascending and uses `cursor` to load older pages (FR-010). Ordering is identical on every request, including same-instant messages (FR-007).

### `POST /tenant/conversations/{conversation_id}/messages` — add message (US3)

**Permission**: `conversations.manage`

```json
{
  "kind": "reply",                    // "reply" | "note" | "customer" (customer = manually logged, Q4)
  "body": "Thanks — checking now…"    // 1–10000 chars after trim
}
```

**201** →

```json
{
  "data": {
    "message": { /* Message */ },
    "conversation": { "status": "open", "last_activity_at": "…" }
  }
}
```

Semantics: bumps `last_activity_at`; `kind ∈ customer|reply` while status `resolved|closed` auto-reopens to `open` in the same transaction with a `conversation.status_changed` audit row (`auto: true`) — the response's `conversation.status` reflects it (Q1). `kind: note` never changes status. `kind: customer` records the acting member as `logged_by`. Empty/whitespace-only body → `422` with field detail. Messages are immutable — no PATCH/DELETE routes exist.

### `PATCH /tenant/conversations/{conversation_id}` — status / assignment (US4)

**Permission**: `conversations.manage`

```json
{
  "status": "resolved",                   // optional; any value in the status set (any→any, FR-012)
  "assigned_membership_id": "uuid|null"   // optional; null = unassign; must be an active tenant membership
}
```

At least one field required (else `422`). Omitted fields unchanged; last write wins (spec edge case — no version precondition).

**200** → `ApiResponse<ConversationDetail>`. Audit rows in the same transaction: `conversation.status_changed` (`from`, `to`, `auto: false`) when status changes; `conversation.assignment_changed` (`from_membership_id`, `to_membership_id`) when assignment changes; no-op values write no audit row. Inactive/unknown-in-tenant membership → `422` with `{ "field": "assigned_membership_id" }` detail (FR-013).

### `GET /tenant/customers/{customer_id}/conversations` — profile history (unchanged route, 012)

**Permission**: `customers.view` — shape, window (20, `has_more`), and semantics unchanged from 012; `status` values now come from the new vocabulary (FR-018).

## Errors

| Status / code | Trigger |
|---------------|---------|
| `401 unauthenticated` | No valid session |
| `403 unauthorized` | Authenticated but lacking the route permission (e.g., Viewer on any POST/PATCH — FR-015) |
| `404 not_found` | Conversation/customer id absent **or belongs to another tenant** (indistinguishable, FR-016); also soft-deleted conversations |
| `422 validation_failed` | Bad body/params: empty or >10,000-char body, unknown kind/status/channel/filter value, missing required create fields, inactive or unknown-in-tenant assignee — with field-level `details[]` |

## Audit actions (append-only, same transaction as the write)

| Action | When | Detail payload |
|--------|------|----------------|
| `conversation.created` | POST /tenant/conversations | `customer_id`, `channel` |
| `conversation.status_changed` | PATCH status change; auto-reopen on message | `from`, `to`, `auto` |
| `conversation.assignment_changed` | PATCH assignment change (assign, reassign, unassign) | `from_membership_id`, `to_membership_id` |

Message sends are not separately audited — the immutable `messages` table is the record (FR-017 scope).
