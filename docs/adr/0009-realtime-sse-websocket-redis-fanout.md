# ADR-009: Realtime — SSE for streams, WebSocket for bidirectional chat, Redis pub/sub fan-out

- **Status:** Accepted (MOH-17)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §9 and §7.3](../architecture/system-architecture.md#9-realtime),
  ADR-005 (session revocation closes streams), ADR-006 (tenant authorization), ADR-007 (events),
  ADR-010 (`gateway` serves customer streams)

## Context

Realtime matters in three places:

- **Agents** watch the inbox, escalation queue, presence and notifications update live, and see AI
  replies stream token by token.
- **Customers** in the widget see agent and AI replies arrive, typing indicators, and status
  changes (handoff, closed).
- **Agents and customers both type**, so some traffic flows client → server at high frequency.

The decision has to balance these forces:

- **Multiple instances.** `api` and `gateway` run several instances behind a load balancer. An
  event produced on instance A must reach a client connected to instance B.
- **Reconnects.** Mobile networks drop. Clients must resume without missing events or reloading
  everything.
- **Simplicity and proxies.** Corporate proxies, CDNs and HTTP/2 handle plain HTTP streams better
  than WebSocket upgrades. Fewer protocols means less code.
- **Security.** Every stream is tenant-scoped and must stop when access is revoked.
- **Backpressure.** A slow client must not make the server buffer without limit.

## Decision

### 1. Transport per use

| Use | Transport | Why |
| -- | -- | -- |
| Server → client streams: AI token streaming, inbox/queue updates, notifications, presence, widget message delivery | **Server-Sent Events** | One-way is all these need. Plain HTTP, works over HTTP/2 multiplexing, auto-reconnect with `Last-Event-ID`, easy through proxies. |
| Bidirectional, high-frequency: typing indicators, read receipts, live widget chat input | **WebSocket**, added only where SSE + normal `POST` requests are measurably not enough | Avoids one HTTP request per keystroke-level event. |
| Client → server commands (send message, claim, mark read) | **Normal HTTP `POST`** | Keeps commands idempotent, authorised and rate-limited like any other API call. |

SSE is the default. A WebSocket endpoint needs a stated reason in its spec.

### 2. Fan-out across instances: Redis pub/sub

- Producers publish events after commit (from the outbox dispatcher, ADR-007) to Redis channels
  named by tenant and topic: `rt:t:{tenant_id}:inbox`, `rt:t:{tenant_id}:conv:{conversation_id}`,
  `rt:t:{tenant_id}:presence`.
- Each instance subscribes to the channels its connected clients need and forwards events to
  their streams through **bounded** per-connection channels.
- Redis is disposable (§7.3): if it's down, clients fall back to periodic resync (refetch), so
  correctness doesn't depend on pub/sub delivery.

### 3. Resume and replay

- Every event has a monotonically increasing ID per stream scope (e.g. per conversation, from the
  outbox UUIDv7).
- Clients reconnect with `Last-Event-ID`. The server replays from a short per-scope buffer (Redis
  stream capped by length/time, e.g. 5 minutes). If the gap is older than the buffer, it sends a
  `resync` event and the client refetches state over REST.
- Clients reconnect with exponential backoff and full jitter.

### 4. Security

- Authorization runs **on subscribe**: session (ADR-005), tenant membership and permission
  (ADR-006). Channel names come from the server-side tenant context, never from the client.
- The stream records its session ID and is **closed on revocation**, logout, or tenant
  suspension, and the client must re-authorise on reconnect.
- Widget streams authenticate with the widget session token, scoped to one conversation.

### 5. Backpressure and limits

- Bounded per-connection buffers; a client that falls behind gets `resync` and is disconnected
  rather than buffered indefinitely.
- Heartbeat comments every 20 s keep proxies from closing idle streams.
- Per-tenant and per-user caps on concurrent streams.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **WebSocket for everything** | More code (custom framing, reconnect, resume) and worse proxy/CDN behaviour, for flows that are one-way anyway. Commands over WebSocket lose standard HTTP rate limiting, idempotency and auth middleware. |
| **Long polling** | Higher latency and request overhead; still needs the same fan-out and resume logic. |
| **In-process broadcast only** (no Redis) | Works with one instance only. With two or more, clients miss events produced on other instances. |
| **PostgreSQL `LISTEN/NOTIFY` for fan-out** | Holds a connection per instance per channel set, payloads are limited to 8 KB, and it adds load to the primary database. Acceptable for waking workers (ADR-007), not for per-client fan-out. |
| **Managed realtime services** (Pusher, Ably) | Vendor lock-in, per-message cost, and tenant data leaves our infrastructure. |
| **WebTransport / HTTP/3 streams** | Not yet broadly supported by browsers, proxies and our stack. |

## Consequences

**Positive**

- Most realtime code is plain HTTP; one protocol to secure, test and observe.
- Works with any number of instances.
- Reconnects resume without full reloads in the common case.

**Negative and costs**

- Redis pub/sub and a replay buffer to run and monitor.
- Every stream holds an open connection; instance limits and load-balancer timeouts must be
  configured for long-lived connections.
- WebSocket endpoints, when added, need their own auth, rate limiting and tests.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| RT1 | **Fan-out is in-process only.** Events go through a per-tenant `tokio::sync::broadcast` channel inside one process. With more than one instance, a client misses events produced on other instances. Presence is also tracked per process. | `backend/crates/modules/escalations/src/presence.rs` (`connect`, `broadcast`) | **Blocker for horizontal scaling.** Publish through Redis pub/sub; move presence to Redis TTL keys (§7.3). |
| RT2 | **No resume.** There's no `Last-Event-ID` handling or replay buffer; a lagged receiver drops events. | `escalations/src/events.rs`, `widgets/src/public_events.rs` | Add event IDs, `Last-Event-ID` replay and the `resync` event; update the dashboard and widget SSE clients. |
| RT3 | SSE streams aren't closed on session revocation (also ADR-005 A8). | same | Record the session ID on subscribe and close on revocation. |
| RT4 | Two SSE endpoints exist (`GET /tenant/events`, `GET /widget/v1/conversations/{id}/events`) with keep-alive; there's no WebSocket. This matches the "SSE by default" rule. | — | None. |
