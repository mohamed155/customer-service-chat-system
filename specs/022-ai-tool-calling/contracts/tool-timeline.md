# Contract: Conversation Tool Activity & SSE Events

## `GET /tenant/conversations/{id}/tool-activity`

Staff-only (conversation read access). Paged, newest-first; single joined query (no N+1).

```json
{
  "items": [
    {
      "id": "uuid",
      "generation_id": "uuid",
      "tool_name": "lookup_customer",
      "tool_source": "builtin",
      "arguments": { },
      "status": "succeeded",
      "approval_required": false,
      "chain_index": 0,
      "decided_by": { "membership_id": "uuid", "display_name": "Dana A." },   // null unless decided
      "decided_at": null,
      "expires_at": null,
      "started_at": "...", "finished_at": "...",
      "duration_ms": 412,
      "result": { },              // staff-visible success payload; null otherwise
      "error": null,              // sanitized failure detail; null otherwise
      "created_at": "..."
    }
  ],
  "next_cursor": null
}
```

Guarantees: customers have no route to this data (staff-auth only — FR-020); `result`/`error` never contain credentials (SC-008); terminal `denied`/`expired`/`cancelled` items always show `started_at: null` (never executed).

## SSE: `GET /tenant/events` — new event family

New `Event::ConversationTool` variant on the existing tenant stream (021 conventions). Two event types:

### `tool.request.created`

```json
{
  "type": "tool.request.created",
  "payload": {
    "id": "uuid",
    "conversation_id": "uuid",
    "tool_name": "update_customer_contact",
    "tool_source": "builtin",
    "arguments": { },
    "approval_required": true,
    "expires_at": "2026-07-18T12:05:00Z",
    "chain_index": 0,
    "created_at": "..."
  }
}
```

Emitted when a request is created (auto requests included, so the timeline animates live). For approval-required requests this is the signal that renders the approval card.

### `tool.request.updated`

```json
{
  "type": "tool.request.updated",
  "payload": {
    "id": "uuid",
    "conversation_id": "uuid",
    "status": "succeeded",       // any transition: refused|awaiting_approval→approved|denied|expired|cancelled|executing|succeeded|failed|timed_out
    "decided_by_display_name": "Dana A.",   // present on approve/deny
    "duration_ms": 412,                      // present on terminal executed states
    "has_result": true,
    "error": null                            // sanitized, present on failure states
  }
}
```

Clients fold updates into timeline entries by `id`; the approval card resolves (and disables its actions) on any terminal/decided update — including decisions made by another staff member (FR-014 race rendering). Full `result` payloads are fetched via the REST endpoint, not pushed over SSE (keeps events small; result display is on-demand).

## Interim holding message

The two-phase approval flow posts the interim message as a normal `kind: "ai"` message via existing message contracts — no new message shape. It is distinguishable in the timeline context by the adjacent `tool.request.created` (approval_required) event; no customer-visible difference from a regular AI message (FR-020).
