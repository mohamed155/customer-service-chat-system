# Contract: AI Message Events (SSE, additive to `GET /tenant/events`)

Additive event family on the existing per-tenant SSE stream (014). Existing consumers that ignore unknown event types are unaffected (Principle V). All events are tenant-scoped by the stream itself; clients additionally filter by `conversationId`.

## Event envelope

Same envelope as existing stream events: `event:` line = event type, `data:` line = JSON payload, monotonically increasing per-connection `id:`.

## Event types

### `ai.message.started`

Generation picked up for a conversation. Drives the thinking indicator.

```jsonc
{
  "conversationId": "…",
  "generationId": "…",          // ai_generations.id; stable across all events of one run
  "triggerMessageId": "…",
  "startedAt": "2026-07-18T10:00:00Z"
}
```

### `ai.message.delta`

Progressive response text. Throttled server-side (~4/s); each delta is an **append** fragment, not a snapshot.

```jsonc
{
  "conversationId": "…",
  "generationId": "…",
  "text": "Our enterprise plan "
}
```

### `ai.message.completed`

Terminal success. Carries the full stored message object (same shape as the timeline `Message`, including `citations` and `confidence`) so clients can append without a refetch.

```jsonc
{
  "conversationId": "…",
  "generationId": "…",
  "message": { /* Message object — see contracts/message-confidence.md */ }
}
```

### `ai.message.superseded`

Run aborted (newer customer message, or escalation/human claim mid-generation). Clients MUST discard any buffered delta text for `generationId`. If a newer message caused it, a new `ai.message.started` (new `generationId`) follows from the regeneration.

```jsonc
{
  "conversationId": "…",
  "generationId": "…",
  "reason": "newer_message" | "escalated"
}
```

### `ai.message.failed`

Run exhausted retries; fallback path ran. Payload intentionally carries only a sanitized category — no provider error detail reaches browsers. The fallback `system` message itself arrives via normal timeline mechanisms.

```jsonc
{
  "conversationId": "…",
  "generationId": "…",
  "category": "unavailable" | "timeout" | "rate_limited" | "authentication" | "invalid_request" | "internal"
}
```

## Contract rules

1. **Ordering per generation**: exactly one `started`, zero or more `delta`, then exactly one terminal event (`completed` | `superseded` | `failed`).
2. **Display-only deltas**: clients must never persist delta text into timeline caches; the timeline fetch / `completed.message` is the source of truth (guarantees reconnect coherence, SC-003).
3. **Mid-join**: a client connecting mid-generation may receive deltas without `started`; it must tolerate this (render from first delta or wait for terminal event).
4. **Confidence privacy**: these events flow only on the authenticated staff stream; no customer-facing surface consumes them (FR-010).
5. **Unknown-type tolerance**: consumers must ignore event types they don't recognize (forward compatibility for future channels).
