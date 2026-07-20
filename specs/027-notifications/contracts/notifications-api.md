# API Contract: Notifications (027)

Conventions: JSON snake_case; error envelope + `X-Request-Id` per workspace contract; utoipa-documented via `routes!()` co-registration (required by `openapi_coverage.rs`); cursor pagination matching the audit/conversations surface.

**Auth for every endpoint**: session + `X-Tenant-ID` (TenantContext). **No permission gate** — per FR-012a the inbox is available to every active tenant member. Authorization is row-level instead: every query is filtered by `recipient_membership_id = <caller's membership in this tenant>`, which is strictly narrower than any role check. A caller can only ever address their own rows, so there is no object-level authorization to get wrong.

Tag: `notifications`

---

## GET `/api/v1/tenant/notifications`

The caller's own inbox for the current tenant, newest first (FR-005).

**operation_id**: `list_notifications`

### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `cursor` | string | — | opaque; pass back `pagination.next_cursor` verbatim |
| `limit` | int | 20 | clamped 1..=50 |
| `state` | string enum | — | `unread \| read \| resolved`; omit for all |

Invalid cursor/state/limit → `422`.

### Response `200`

```json
{
  "data": [
    {
      "id": "9d2c…",
      "kind": "escalation.new",
      "state": "unread",
      "title": "New escalation needs an agent",
      "body": "Customer asked for a human on \"Billing question\".",
      "subject_type": "escalation",
      "subject_id": "4a17…",
      "actor": {
        "membership_id": "77bd…",
        "display_name": "Dana Ops"
      },
      "created_at": "2026-07-20T14:03:22Z",
      "read_at": null
    }
  ],
  "pagination": { "next_cursor": "eyJ…", "has_more": true }
}
```

`actor` is `null` for system-caused notifications (AI failure, tool approval). `body` may be `null`.

`subject_type` + `subject_id` are what the dashboard routes on (FR-008); the server deliberately does **not** return a URL, since route shapes are a frontend concern owned by `APP_PATHS`.

---

## GET `/api/v1/tenant/notifications/unread-count`

Badge count (FR-006). Separate from the list so the topbar can hold it on every page without paying for a list query (FR-013).

**operation_id**: `get_unread_notification_count`

### Response `200`

```json
{ "data": { "count": 7 } }
```

Counts `state = 'unread'` only — `read` and `resolved` are both excluded (FR-011a).

---

## POST `/api/v1/tenant/notifications/{id}/read`

Mark one notification read (FR-007).

**operation_id**: `mark_notification_read`

- `200` with the updated notification (same shape as a list item).
- `404` if the id does not exist **or** belongs to another member — deliberately not `403`, so the endpoint cannot be used to probe for other members' notification ids.
- Idempotent: marking an already-`read` row succeeds and returns it unchanged.
- A `resolved` row can be marked read; it moves to `read`. The reverse is system-only.

No request body.

---

## POST `/api/v1/tenant/notifications/read-all`

Mark every unread notification for the caller read (FR-007).

**operation_id**: `mark_all_notifications_read`

### Response `200`

```json
{ "data": { "marked": 7 } }
```

Single set-based `UPDATE … WHERE recipient_membership_id = $1 AND state = 'unread'`. Idempotent — a second call returns `{"marked": 0}`.

---

## SSE: `GET /api/v1/tenant/events` (existing endpoint, new event type)

Live badge updates (FR-014). Delivered on the existing stream; see research.md R5 for why this is reused and for the `conversations.view` nuance.

**Event name**: `notification.created`

```json
{
  "membershipId": "77bd…",
  "notificationId": "9d2c…",
  "unreadCount": 8
}
```

Filtered server-side so a member only ever receives their own, following the `availability.changed` precedent in `GuardedStream`. `unreadCount` is included so the badge updates from the event alone, with no follow-up request.

**Event name**: `notification.cleared`

```json
{
  "membershipId": "77bd…",
  "unreadCount": 6
}
```

Emitted to each member whose row was auto-resolved, so their badge decreases without any action on their part (SC-009). No `notificationId` — a single resolve may affect several of that member's rows.

> **Naming note**: these SSE names (`notification.created`, `notification.cleared`) are deliberately distinct from the internal outbox `event_type` values (`notification.requested`, `notification.resolved`). They are different namespaces on different transports — outbox events are server-internal work items, SSE events are browser-facing badge updates — and reusing `notification.resolved` for both would invite an implementer to wire one to the other.

Clients must ignore unknown event types on this stream; it is shared with escalation, availability, and conversation events.

---

## Error envelope

Standard workspace shape. Notification-specific cases:

| Status | When |
|---|---|
| `401` | no session |
| `400` | missing/invalid `X-Tenant-ID`, or caller has no active membership in that tenant |
| `404` | notification id not found, or not owned by the caller |
| `422` | malformed cursor, unknown `state`, out-of-range `limit` |
