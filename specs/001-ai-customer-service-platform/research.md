# Phase 0 Research: AI Customer Service Platform

**Date**: 2026-07-03 | **Plan**: [plan.md](./plan.md)

The Constitution fixes the technology stack, so no NEEDS CLARIFICATION markers
existed in Technical Context. Research therefore records the *architectural
decisions within the fixed stack*, each with rationale and alternatives
considered.

## R-01: Module boundaries as Cargo workspace crates

- **Decision**: One crate per domain module under `backend/crates/modules/`,
  with shared kernel crates; modules expose only application-service APIs and
  domain events.
- **Rationale**: Crate boundaries make module isolation compile-time-enforced
  (Constitution I). `pub(crate)` internals cannot be reached across modules,
  so "no cross-module table access" is not a convention but a build error.
  Extraction to a microservice later means lifting a crate, its migrations,
  and its events — not disentangling a shared codebase.
- **Alternatives considered**: Single crate with Rust modules (no compiler
  enforcement, rejected); separate services now (violates Constitution's
  modular-monolith mandate, operationally premature).

## R-02: Domain events via in-process bus + transactional outbox

- **Decision**: Synchronous in-process event dispatch for same-transaction
  consumers (audit), plus a transactional outbox table drained by a background
  worker for async consumers (webhooks, analytics, notifications, metering).
- **Rationale**: Guarantees audit events are never lost (same transaction as
  the action), gives at-least-once delivery for async work without operating a
  broker, and the outbox is the natural seam for a broker later (tech-debt
  entry acknowledged in plan).
- **Alternatives considered**: Kafka/NATS now (operational cost unjustified at
  v1 scale); Postgres LISTEN/NOTIFY only (no durability, lost events on
  disconnect); Redis streams (durability and replay semantics weaker than an
  outbox owned by the source-of-truth database).

## R-03: Tenant isolation mechanism

- **Decision**: Application-layer enforcement via a typed `TenantScope` that
  every tenant-scoped repository method requires as a parameter, extracted
  once per request from the authenticated context; **plus** Postgres
  row-level security (RLS) policies on tenant-owned tables as
  defense-in-depth, with the session tenant set per connection checkout.
- **Rationale**: The typed parameter makes forgetting isolation a compile
  error at the API level; RLS catches any query that slips through review.
  Constitution II demands isolation "where it cannot be bypassed by a
  compromised or buggy client" — two independent layers deliver that.
- **Alternatives considered**: RLS only (harder to test/debug, connection-
  pool pitfalls if used alone); schema-per-tenant (migration fan-out and
  connection churn at 1,000 tenants); database-per-tenant (operationally
  prohibitive at this scale, revisit only for regulated enterprise tier).

## R-04: Realtime transport

- **Decision**: WebSocket as primary for widget and inbox, SSE fallback for
  restrictive networks; Redis pub/sub for cross-node fan-out; client-side
  reconnect with replay from a last-acked message cursor stored per
  conversation.
- **Rationale**: Bidirectional needs (typing, presence, acks) favor WS;
  cursor-replay makes rolling deploys and node loss invisible to users
  (NFR-AVAIL-002); Redis pub/sub keeps nodes stateless so any node serves any
  conversation.
- **Alternatives considered**: SSE-only + POST (simpler but typing/presence
  become chatty polling); long-polling (latency budget risk); sticky sessions
  (breaks zero-interruption deploys and horizontal scaling).

## R-05: AI provider abstraction shape

- **Decision**: A capability trait in the `ai-providers` crate —
  `chat`, `chat_stream`, `tool_call` support inside chat, `embed` — with a
  normalized message/tool/usage model, a provider-agnostic error taxonomy
  (retryable / rate-limited / invalid-request / provider-down), and a
  `FailoverExecutor` that applies routing policy. Vendor HTTP APIs called
  directly via `reqwest`-style clients; no vendor SDKs.
- **Rationale**: Constitution IV requires adding a provider to touch only the
  abstraction. Direct HTTP keeps dependency surface small and streaming
  behavior uniform. The normalized error taxonomy is what makes failover
  policy provider-independent.
- **Alternatives considered**: Vendor SDKs per provider (inconsistent
  streaming/tool semantics leak upward); LiteLLM-style external proxy
  (another service to run; hides cost/latency detail the timeline needs);
  trait-per-capability-object instead of one trait (over-abstracted for
  three launch providers).

## R-06: Deterministic prompt assembly

- **Decision**: A pure-function context assembler: inputs are (published
  prompt version, ordered behavior constraints, whitelisted customer
  attributes, conversation window + rolling summary, ranked retrieval results,
  current-turn tool results). Output is a canonical, versioned context
  structure; the full input snapshot and output hash are persisted in the
  execution timeline.
- **Rationale**: Constitution IV mandates deterministic construction. Making
  the assembler a pure function of explicit inputs allows a byte-equality CI
  test and makes timelines fully reproducible for debugging.
- **Alternatives considered**: Template-engine-with-callbacks (hidden
  nondeterminism via clock/random/DB access inside rendering — rejected);
  assembling inside each provider adapter (would fork behavior per vendor).

## R-07: RAG retrieval approach

- **Decision**: Hybrid retrieval — pgvector cosine similarity + Postgres FTS
  keyword search, fused with reciprocal-rank fusion, relevance threshold below
  which the AI takes the "honest fallback" path. HNSW index on embeddings.
  Embeddings generated through the provider abstraction's `embed` capability.
- **Rationale**: Hybrid consistently beats pure-vector on product/support
  corpora (exact terms: SKUs, error codes); everything stays in Postgres
  (operational simplicity, transactional consistency with source lifecycle,
  tenant isolation via the same RLS/TenantScope machinery). Meets ≤1 s p95 at
  v1 scale with per-tenant partial indexes.
- **Alternatives considered**: Dedicated vector DB (another stateful system;
  isolation would need re-solving); pure vector search (misses exact-match
  queries); external search engine (deferred to post-GA per tech-debt list).

## R-08: Knowledge ingestion pipeline

- **Decision**: Async pipeline as Tokio background workers polling a
  Postgres-backed job queue (`FOR UPDATE SKIP LOCKED`), stages: fetch/extract
  → segment → embed → index swap (atomic visibility flip). Files in S3;
  extraction for PDF/Word/HTML/MD/text via Rust-native parsers.
- **Rationale**: Postgres-backed jobs give durability, retries, and
  observability with zero new infrastructure; atomic visibility flip
  satisfies "old content serves until new is ready" (spec 7.4).
- **Alternatives considered**: Redis-based queue (jobs lost on Redis loss;
  Redis stays cache/pubsub-only by design); separate ingestion service
  (violates monolith-first); synchronous ingestion (blocks UX, times out on
  large files).

## R-09: Session/auth token model

- **Decision**: Opaque server-side sessions in Postgres (cache in Redis) for
  dashboard users with idle/absolute timeouts and revocation; hashed scoped
  API keys for programmatic access; short-lived signed widget tokens minted
  per widget session, carrying tenant + customer claims.
- **Rationale**: Server-side sessions make "revoke now" and "role change ≤1
  min" (FR-RBAC-005) trivially true — no waiting for JWT expiry. Widget
  tokens must be stateless-verifiable at high volume, so those are signed
  tokens with short TTL + refresh.
- **Alternatives considered**: JWT everywhere (revocation and role
  propagation require denylists that recreate server state anyway);
  third-party auth service (control and audit-surface loss for a
  security-core product; SSO integration point kept for enterprise IdPs).

## R-10: Angular workspace architecture

- **Decision**: Single Angular workspace: `apps/dashboard` (one app,
  role-driven shell serving both tenant and platform surfaces via lazy-loaded
  route groups) and `apps/widget` (separate minimal app, custom-element/iframe
  embed, strict bundle budget). Shared `libs/ui` (tokens→primitives→patterns),
  `libs/data-access` (generated API types + interceptors), `libs/realtime`,
  and `libs/feature-*` per domain. Signals for state, RxJS at
  realtime/stream boundaries.
- **Rationale**: One dashboard app avoids duplicating shell/auth/nav across
  platform and tenant surfaces (RBAC already gates routes); the widget's
  ~50 KB budget forbids sharing anything heavier than tokens. Library-first
  layout enforces Constitution IX ordering (tokens before components before
  pages).
- **Alternatives considered**: Separate platform-admin app (duplicated shell,
  drift risk; revisit only if release cadences diverge); Nx monorepo tooling
  (nice-to-have, not required; plain Angular workspace keeps toolchain lean —
  can adopt later without restructuring); widget as plain TS without Angular
  (faster bundle but forks the component model; Angular custom element with
  aggressive budget chosen, budget gate protects the decision).

## R-11: Metering idempotency

- **Decision**: Usage events written with a deterministic idempotency key
  (e.g., AI interaction = execution ID) into an append-only usage table with
  a unique constraint; aggregation jobs are pure rollups. Billing consumes
  aggregates only.
- **Rationale**: Retries anywhere in the pipeline (provider retry, outbox
  redelivery, API retry) can never double-bill (SC-010) because the unique
  key makes the write idempotent at the database level.
- **Alternatives considered**: Counter increments (not idempotent under
  redelivery); dedupe-in-consumer memory (lost on restart).

## R-12: Observability stack

- **Decision**: `tracing` with JSON output and a redaction layer; OpenTelemetry
  export for traces; Prometheus metrics endpoint; request-ID middleware
  propagating into all spans, logs, error envelopes, and outbound provider
  calls. Local dev runs an OTel collector + Jaeger/Grafana via compose.
- **Rationale**: Constitution VI requires request ID, structured logs,
  tracing, metrics on every request from day one; this is the standard,
  vendor-neutral Rust stack; redaction layer enforces NFR-LOG-002 at source.
- **Alternatives considered**: Vendor APM agent (lock-in, weaker Rust
  support); logs-only start (violates Constitution VI; retrofit cost high).

## R-13: Open item carried from /speckit-clarify (documented default)

- **Item**: Customer session window for conversation resumption (clarify Q4
  was presented but the session ended before an answer).
- **Decision (default adopted)**: 30 minutes of inactivity ends the widget
  session for resumption purposes; returning within the window resumes the
  conversation, after it a new conversation starts linked to the same
  customer profile. Tenant-configurable in Settings (FR-SET-001 policy set).
- **Rationale**: Matches common live-chat products; cheap to change; exposed
  as tenant configuration so the default is low-stakes.
- **Action**: Resolved 2026-07-03 — default recorded in spec Clarifications
  and Assumption A-11 via /speckit-analyze remediation.

## R-14: Confidence & sentiment signal definitions

- **Decision**: Confidence is a normalized 0.0–1.0 score computed as a weighted
  blend of (a) top-retrieval relevance score, (b) retrieval-answer grounding
  overlap, and (c) model self-assessment token (requested in the assembled
  context's output-format instructions). Default thresholds: escalate <0.35,
  clarify 0.35–0.55, caveat 0.55–0.75, answer ≥0.75 — tenant-tunable
  (FR-AI-005). Sentiment/frustration = lightweight classifier over the last 3
  customer messages via the provider abstraction (chat capability, fixed
  rubric prompt), emitting {neutral, frustrated, angry}; `angry` or 2×
  consecutive `frustrated` triggers the escalation rule (FR-AI-006).
- **Rationale**: deterministic inputs, cheap to compute, testable as a matrix
  (tasks T056); avoids a dedicated ML service at v1.
- **Alternatives considered**: logprob-based confidence (not uniformly exposed
  across providers); dedicated sentiment model (new infrastructure, deferred
  post-GA).
