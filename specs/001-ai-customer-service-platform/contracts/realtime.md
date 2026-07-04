# Contract: Realtime Protocol

WebSocket primary, SSE fallback (research R-04). Two surfaces share one frame
format: **widget** (customers) and **inbox** (agents/managers).

## Connections

| Surface | Endpoint | Auth |
|---------|----------|------|
| Widget | `WSS /api/v1/widget/ws` | widget token (query-less; first frame auth) |
| Inbox | `WSS /api/v1/ws` | session token (first frame auth) |
| SSE fallback | `GET /api/v1/{widget/}events?cursor=` | same tokens; send via REST POST |

Connection lifecycle: connect → `auth` frame ≤5 s (else server closes) →
`ready` (includes resume state) → heartbeat `ping`/`pong` every 25 s (two
missed ⇒ close) → reconnect with backoff + jitter.

## Frame format

```json
{ "type": "message.created", "seq": 4182, "conversation_id": "c_...", "data": { ... } }
```

`seq` is the per-conversation monotonic message sequence (Message.seq in the
data model) — it is the **replay cursor**.

## Resume & replay (zero-loss deploys, NFR-AVAIL-002)

Client sends on auth: `{ "resume": [{ "conversation_id": "c_..", "last_seq": 4180 }] }`.
Server replays missed frames per conversation in order before `ready`.
Server-side buffer: durable (messages are rows; replay is a query), so node
death/rolling deploys lose nothing. Duplicate delivery possible ⇒ clients
dedupe by (conversation_id, seq).

## Event types

### To widget
- `message.created` (agent/AI/system public messages)
- `message.delta` — AI streaming: `{ "execution_id", "delta_text", "done": false }`;
  terminal delta carries `done: true` + final `message.created` follows with
  citations
- `conversation.status_changed`, `agent.joined`, `agent.typing`,
  `csat.requested`, `system.notice` (business-hours/offline/incident banner)

### To inbox
- `message.created` (incl. internal notes), `message.delta` (live AI view)
- `conversation.status_changed`, `customer.typing`,
  `escalation.queued` (drives ≤5 s alert, FR-NOTIF-002), `escalation.assigned`,
  `escalation.requeued`, `presence.changed` (agent availability),
  `notification.created`
- `timeline.step` (optional live execution-timeline streaming for the open
  conversation)

### From clients (both directions validated server-side)
- `auth`, `pong`, `typing.start/stop`, `message.ack` `{conversation_id, seq}`
- Sends go over REST (`POST .../messages` with idempotency), not WS — keeps
  write path uniform, idempotent, and rate-limited.

## Fan-out

Nodes are stateless; Redis pub/sub channel per conversation
(`conv:{tenant}:{id}`) and per user (`user:{identity}`) bridges nodes. Any
node serves any connection (rolling deploys drain connections; clients
reconnect-and-resume elsewhere).

## Backpressure & limits

- Per-connection send queue cap; slow consumers dropped with `close: 1013`
  (client resumes later — durable replay makes this safe).
- Widget: max 1 msg/s sustained, burst 5 (429-equivalent close code 4429).
- Frame size ≤64 KB.
