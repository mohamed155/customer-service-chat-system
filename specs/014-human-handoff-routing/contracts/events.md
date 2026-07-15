# Events Contract: SSE Stream, Presence & Outbox

**Feature**: 014-human-handoff-routing | **Date**: 2026-07-14

## SSE stream — `GET /tenant/events`

- **Transport**: `text/event-stream` over plain HTTP; passes the full middleware stack (session cookie, CSRF origin check, `X-Tenant-ID` tenant context, request-id). Permission: `conversations.view`.
- **Client**: fetch-based reader (native `EventSource` cannot send `X-Tenant-ID`), wrapped in an RxJS Observable with exponential-backoff retry (research R1). Multiple tabs = multiple connections; all counted for presence.
- **Heartbeat**: SSE comment (`: ping`) every ~20 s. `retry:` hint set to 3000 ms.
- **Scoping**: events are fanned out per `(tenant, membership)` — a member receives only events addressed to their tenant, and member-addressed events (`escalation.assigned`) only on the target member's streams (SC-006).

### Event schema

Each event: `event: <type>`, `data: <JSON>`, monotonically increasing per-connection `id:`. Version field inside payload (`"v": 1`).

| `event:` | Audience | Payload (`data`) | Purpose |
|----------|----------|------------------|---------|
| `escalation.assigned` | target member only | `{ v, escalationId, conversationId, reason, routingReason, matchedSkills, assignedAt }` | FR-025 real-time assignment notification (browser + in-app) |
| `escalation.queued` | all tenant members with an open stream | `{ v, escalationId, conversationId, escalatedAt, requiredSkills }` | Live queue page updates; topbar queue badge |
| `escalation.removed` | all tenant members | `{ v, escalationId, cause: "claimed" \| "assigned" \| "closed" }` | Queue page removes entry (claim races, drain, close-out) |
| `availability.changed` | the member themselves | `{ v, membershipId, state, cause: "toggle" \| "presence_timeout" \| "startup_sweep" \| "deactivated" }` | Keeps the toggle truthful when the server auto-reverts (FR-017a) |

Clients treat the stream as advisory freshness: on (re)connect, stores refetch their REST source of truth (queue list, availability), then apply events incrementally.

## Presence semantics (research R2)

- A member is **present** while ≥1 of their `/tenant/events` connections is open.
- Last connection closed → grace timer (~45 s). If no reconnect and DB state is `available` → server writes `away`, emits `availability.changed { cause: "presence_timeout" }`, audits `availability.changed`.
- Server startup: sweep reverts `available` rows whose owners aren't connected after the grace window (`cause: "startup_sweep"`).
- **Routing eligibility** = DB `available` ∧ present. Presence alone never makes an agent eligible (signing in doesn't restore `available` — Q5).
- Establishing presence while DB state is `available` (reconnect within grace) triggers a queue-drain pass (research R4).

## Outbox events (module-to-module, research R5)

Transactional outbox (0002 pattern); emitted by `conversations` in the same transaction as the change, consumed by the escalations module's worker. Not exposed to clients.

| Event type | Payload | Consumer behavior |
|------------|---------|-------------------|
| `conversation.status_changed` | `{ tenantId, conversationId, oldStatus, newStatus, actorMembershipId?, origin }` | new status `resolved`/`closed` → close active escalation (FR-015), clear `conversations.escalated_at` via `set_escalated_in_tx`, emit `escalation.removed`; status change freeing load → drain pass |
| `conversation.assignment_changed` | `{ tenantId, conversationId, oldMembershipId?, newMembershipId?, actorMembershipId?, origin }` | if active escalation and `origin ≠ "escalations"` → set assignee + routing reason `manual_reassignment` (FR-023), audit; load freed on old assignee → drain pass |

Loop guard: assignments performed by the routing engine call `conversations::assign_in_tx` with `origin="escalations"`; the consumer ignores its own echoes.
