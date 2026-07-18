# Contract: Tool Approvals API

Tenant-context routes. RBAC: conversation-handling roles (Agent+) decide; Viewers read nothing actionable.

## `GET /tenant/tool-requests?status=awaiting_approval`

Paged list (default page size 20, newest first) for surfacing pending approvals beyond the open conversation.

```json
{
  "items": [
    {
      "id": "uuid",
      "conversation_id": "uuid",
      "tool_name": "update_customer_contact",
      "tool_source": "builtin",
      "arguments": { "email": "new@example.com" },
      "status": "awaiting_approval",
      "approval_required": true,
      "expires_at": "2026-07-18T12:05:00Z",
      "created_at": "2026-07-18T12:00:00Z",
      "chain_index": 1
    }
  ],
  "next_cursor": null
}
```

## `POST /tenant/tool-requests/{id}/decide`

Body: `{ "decision": "approve" | "deny" }`.

Semantics (FR-012, FR-014, research §6):
- Conditional transition from `awaiting_approval` only. Success → `200` with the updated request (status `approved` — execution and the follow-up generation proceed asynchronously — or `denied`).
- Already settled (raced decision, expired, cancelled) → `409 conflict` with the settled request in the body — idempotent-safe: clients render the settled state, no retry.
- Decider recorded (`decided_by_membership_id`, `decided_at`).
- Same transaction emits `ai.tool_decision` on the outbox → responder worker runs the follow-up generation (approve: execute then generate with result; deny: generate with denial context).

`403` for Viewer role or non-member; `404` outside tenant.

## Expiry (no endpoint)

Sweep transitions past-due `awaiting_approval` → `expired` and emits the same outbox event with outcome `expired` (treated as declined by the follow-up generation). Cancellation on escalation/claim or in-flight generation supersede follows FR-015 with outcome `cancelled` (no follow-up generation on cancellation — the human has taken over or a newer generation owns the conversation).
