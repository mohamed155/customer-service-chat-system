# ADR-007: Asynchronous processing — PostgreSQL job queue, transactional outbox and inbox

- **Status:** Accepted (MOH-17)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §8](../architecture/system-architecture.md#8-asynchronous-processing),
  ADR-001 (modules communicate through events), ADR-004 (tenant context in job payloads),
  ADR-009 (realtime fan-out), ADR-010 (`worker` deployment unit)

## Context

Much of the platform's work happens outside the request: AI turns, document ingestion and
embeddings, outbound channel delivery (WhatsApp, email), escalation routing, notifications,
webhooks and analytics roll-ups. Modules also react to each other's domain events (ADR-001).

The decision has to balance these forces:

- **No lost or phantom events.** If a conversation is marked escalated, the escalation event must
  exist; if the transaction rolls back, the event must not. Writing to a database and a broker
  separately (dual write) breaks this.
- **At-least-once reality.** Workers crash mid-job. Every delivery path has to tolerate repeats.
- **Fairness.** One tenant importing 10,000 documents must not starve every other tenant's AI
  replies (architecture §5.4).
- **Operational cost.** A broker (Kafka, RabbitMQ, NATS) is another stateful system to run,
  secure, back up and monitor. The team is small.
- **Throughput.** Expected volume for the first years is thousands of jobs per second at most,
  which PostgreSQL handles comfortably with the right indexes.

## Decision

### 1. Transactional outbox for domain events

- A producer writes its state change and an `outbox` row **in the same transaction**. Columns:
  `id` (UUIDv7), `tenant_id uuid` (NULL only for platform events), `event_type`, `payload jsonb`,
  `trace_context`, `created_at`, `dispatched_at`.
- A **dispatcher** claims undispatched rows in id order with
  `SELECT … FOR UPDATE SKIP LOCKED LIMIT n`, hands each event to its subscribed handlers (which
  usually enqueue jobs), and marks it dispatched. Dispatched rows are kept for a retention window
  (e.g. 7 days) for replay and debugging, then deleted by the scheduler.
- Trace context travels with the event so a trace continues from the request to the worker.

### 2. PostgreSQL job queue

- One `jobs` table, separate from the outbox: `id`, `tenant_id`, `queue`, `kind`, `payload`,
  `priority`, `run_at`, `attempts`, `max_attempts`, `locked_by`, `locked_until`, `last_error`,
  `status` (`queued | running | succeeded | failed | dead`).
- **Claiming:** `UPDATE jobs SET status='running', locked_until=now()+lease … WHERE id IN
  (SELECT id FROM jobs WHERE queue=$1 AND status='queued' AND run_at<=now() ORDER BY priority, run_at
  FOR UPDATE SKIP LOCKED LIMIT n) RETURNING …`.
- **Leases and heartbeats:** long jobs extend `locked_until`. A sweeper re-queues jobs whose
  lease expired (worker died).
- **Retries:** exponential backoff with **full jitter**, capped (e.g. 5 s × 2ⁿ, max 10 min), up to
  `max_attempts`, then `dead`. Dead jobs are listed and replayable from the platform UI.
- **Fairness:** workers pick across tenants with **weighted round-robin (deficit round-robin)**
  weighted by plan, not global FIFO. Per-tenant concurrency caps for AI turns and ingestion.
- **Wake-up:** `LISTEN/NOTIFY` on enqueue wakes idle workers immediately; a slow poll (e.g. 5 s)
  stays as a fallback so a missed notification only adds latency.
- Job payloads carry `tenant_id`, and the worker re-establishes `TenantContext` (and the RLS
  setting, ADR-004) before touching data.

### 3. Inbox for idempotent consumers

- Every consumer records `(consumer, event_id)` in a `processed_events` inbox table **in the same
  transaction** as its side effects. A repeat delivery hits the unique key and is skipped.
- External side effects that can't share the transaction (sending a WhatsApp message, calling a
  tool) use an **idempotency key** derived from the event/job ID and pass it to the provider when
  the provider supports one.

### 4. Observability

Each queue exposes depth, age of oldest job, throughput, failure rate and dead count, labelled by
queue (and by tenant for the top N). Alerts fire on age and dead-letter growth (constitution VII).

### 5. When to move to a broker

Move a **specific hot queue** (not everything) to Redis Streams or NATS JetStream only when an ADR
shows one of:

- sustained queue throughput above ~2,000 jobs/s, or claim latency p99 above 1 s, after index and
  batch tuning;
- the queue's writes measurably hurt primary database latency (e.g. > 10% of write IOPS or WAL
  volume);
- a consumer outside the PostgreSQL network boundary needs the stream (e.g. an extracted service,
  ADR-001 §5).

The outbox stays in PostgreSQL even then: the dispatcher publishes to the broker, which keeps the
"same transaction" guarantee.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Kafka / RabbitMQ from day one** | Another stateful system to operate, secure and back up. Writing to it from a request is a dual write unless an outbox is added anyway. Current volume doesn't need it. |
| **Redis Streams / lists as the queue** | Fast, but jobs aren't transactional with business data, and Redis is treated as disposable (§7.3). Losing Redis must not lose work. |
| **Outbox only, no separate job table** (one table for events and jobs) | Mixes two lifecycles: events fan out to many handlers; jobs have retries, leases and priorities per consumer. Indexes and retention become a compromise. This is roughly the current state (see below). |
| **In-process `tokio::spawn` for background work** | Work is lost on crash or deploy, there are no retries, and it ties background work to the API process. |
| **Cloud queues (SQS, Pub/Sub)** | Vendor lock-in and still a dual write without an outbox. Local development needs emulators. |
| **pg-boss / graphile-worker style libraries** | Good designs, but they're Node.js. In Rust, the queue is small enough to own (~500 lines) and match our tenancy and fairness rules. |

## Consequences

**Positive**

- No dual writes: events and jobs commit or roll back with the data that caused them.
- No new infrastructure; backups, PITR and RLS cover queue data too.
- Retries, dead letters, fairness and replay are explicit and observable.

**Negative and costs**

- Queue tables are write-heavy and need care: partial indexes, aggressive autovacuum settings,
  and retention deletes in small batches.
- Polling plus `LISTEN/NOTIFY` adds a little latency compared with a push broker.
- We own the queue code (claiming, leases, fairness) and its tests.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| Q1 | **One table does both jobs.** `outbox_events` is used as an event log and as a work queue, with claim tokens, `available_at`, `attempts` and `dead_lettered_at` added later. There is no separate `jobs` table. | `backend/migrations/0002_outbox.sql`, `0022_outbox_delivery_claims.sql` | Introduce `jobs`; move the work-queue uses (invitation delivery, indexing, agent responder, WhatsApp sender, notifications) onto it one worker at a time. |
| Q2 | **Each module has its own polling worker** that claims `outbox_events` rows by `event_type` with `FOR UPDATE SKIP LOCKED` and sleeps between polls. There's no shared dispatcher, no `LISTEN/NOTIFY`, and no per-tenant fairness. | `escalations/src/events.rs`, `knowledge/src/indexer.rs`, `ai/src/agent_responder.rs`, `notifications/src/worker.rs`, `tenancy/src/invitations.rs`, `whatsapp/src/sender.rs` | Build the shared dispatcher and job runner in the platform layer with fairness and wake-up; migrate workers onto it. |
| Q3 | **Processed rows are deleted** (`DELETE FROM outbox_events WHERE id = $1`), so there's no history to replay or debug. | same workers | Mark dispatched instead; delete after the retention window. |
| Q4 | **Backoff is ad hoc per worker.** For example, the knowledge indexer uses `5 × 2ⁿ` seconds capped at 300, without jitter. | `knowledge/src/indexer.rs` | One backoff policy (full jitter) in the shared runner. |
| Q5 | **No consumer inbox.** Duplicate delivery relies on each handler being naturally idempotent. | — | Add `processed_events` and use it in every consumer. |
| Q6 | `outbox_events.tenant_id` is `TEXT NULL`, not `uuid`, and has no FK. | `0002_outbox.sql` | Use `uuid` with an FK in the new tables. |
