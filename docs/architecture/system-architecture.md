# System Architecture

AI Customer Service Platform — architecture reference v1.0 (2026-11-04)

This document explains **how** the platform satisfies the constitution
(`.specify/memory/constitution.md`). It covers architectural style, module design, tenancy,
security, scalability, reliability, AI runtime, and the design patterns and algorithms used.
Decisions marked **ADR** need a formal Architecture Decision Record before implementation.

---

## 1. Quality attribute priorities

When attributes conflict, resolve in this order:

1. **Security and tenant isolation** — a breach ends the business.
2. **Correctness and data integrity** — wrong answers, lost messages, or double-executed tools
   destroy trust.
3. **Maintainability** — one small team must evolve this for years.
4. **Reliability** — support channels must stay up when an AI provider does not.
5. **Scalability and performance** — design for growth, but don't pay for scale we don't have.
6. **Cost efficiency** — AI tokens are the dominant variable cost; meter and cap them.

---

## 2. Architectural style

The platform combines a small set of well-understood styles. Each one solves a specific problem.

| Style | Why it's used here |
| -- | -- |
| **Modular monolith** | One deployable, one database, simple ops; clear module seams allow later extraction |
| **Domain-Driven Design (bounded contexts)** | Conversations, Knowledge, AI Runtime, Identity, etc. have different language and rules |
| **Hexagonal architecture (ports & adapters)** | Domain logic is independent of Axum, SQLx, Redis, and AI vendors; adapters are swappable and testable |
| **CQRS-lite** | Commands go through aggregates with invariants; read-heavy screens (inbox, analytics) use dedicated query models and projections. Same database, no event sourcing |
| **Event-driven (internal)** | Modules react to domain events (`ConversationEscalated`, `DocumentIngested`) instead of calling each other synchronously |
| **Transactional outbox** | Events are written in the same transaction as state, then dispatched — no lost or phantom events |
| **Background workers** | Ingestion, embeddings, outbound channel delivery, webhooks, analytics roll-ups run outside request handlers |
| **Streaming** | AI responses stream to the inbox and widget over SSE/WebSocket |
| **Cell-based architecture (future)** | Large or regulated tenants can be pinned to dedicated cells (DB + workers) without redesign |

**Explicitly not used (yet):** microservices, event sourcing, Kafka, Kubernetes service mesh,
GraphQL. Each would add operational cost without a current need. Revisit only with an ADR and
measurements.

### 2.1 Deployment units

One Rust codebase, multiple binaries (or one binary with run modes):

| Unit | Responsibility | Scaling signal |
| -- | -- | -- |
| `api` | REST API, auth, dashboard backend, SSE/WebSocket for agents | CPU, p95 latency, connections |
| `gateway` | Public widget + channel webhooks (WhatsApp, Telegram), streaming to end customers | Inbound message rate, open streams |
| `worker` | Outbox dispatch, jobs, ingestion, embeddings, AI turns, outbound delivery | Queue depth and age |
| `scheduler` | Cron-like jobs: SLA timers, retention, roll-ups, cycle cleanup (leader-elected) | n/a (singleton) |

Splitting `gateway` from `api` early isolates untrusted public traffic (widget, webhooks) from
the authenticated dashboard and lets each scale independently, while staying one codebase.
**ADR-010.**

```
                   ┌──────────────┐      ┌──────────────┐
 Customers ───────▶│   gateway    │      │  Angular SPA │◀──── Business users
 (widget, WA, TG)  │ (public edge)│      └──────┬───────┘      Platform users
                   └──────┬───────┘             │ HTTPS /api/v1 + SSE
                          │                     ▼
                          │              ┌──────────────┐
                          └─────────────▶│     api      │
                                         └──────┬───────┘
          ┌──────────────────────┬──────────────┼───────────────┬──────────────┐
          ▼                      ▼              ▼               ▼              ▼
   ┌─────────────┐        ┌───────────┐  ┌────────────┐  ┌────────────┐  ┌──────────┐
   │ PostgreSQL  │        │   Redis   │  │ Object     │  │  worker(s) │  │ AI       │
   │ +pgvector   │◀──────▶│ cache, RL,│  │ storage    │  │  outbox,   │─▶│ providers│
   │ +RLS        │        │ pub/sub   │  │ (S3/R2)    │  │  jobs      │  │ (OpenAI, │
   └─────────────┘        └───────────┘  └────────────┘  └────────────┘  │ Anthropic│
                                                                         │ Gemini)  │
                                                                         └──────────┘
```

---

## 3. Bounded contexts and module map

| Module | Owns | Publishes events | Depends on |
| -- | -- | -- | -- |
| `identity` | users, credentials, sessions, MFA, API keys | `UserRegistered`, `SessionRevoked` | kernel |
| `tenancy` | tenants, memberships, roles, permissions, feature flags | `TenantCreated`, `MemberAdded`, `RoleChanged` | identity |
| `customers` | end-customer profiles, identities per channel, attributes | `CustomerMerged` | tenancy |
| `conversations` | conversations, messages, participants, tags, attachments | `MessageReceived`, `ConversationEscalated`, `ConversationResolved` | tenancy, customers |
| `routing` | agent availability, skills, capacity, queue, assignments, SLA timers | `ConversationAssigned`, `SlaBreached` | tenancy, conversations (events) |
| `agent_config` | AI agent settings, business rules, escalation rules | `AgentConfigPublished` | tenancy |
| `prompts` | prompt templates and immutable versions | `PromptVersionPublished` | tenancy |
| `knowledge` | sources, documents, chunks, embeddings, ingestion jobs | `DocumentIngested`, `DocumentDeleted` | tenancy, storage |
| `ai_runtime` | AI turns, context assembly, provider calls, guardrails, usage | `AiTurnCompleted`, `AiEscalationRequested` | agent_config, prompts, knowledge, tools (interfaces) |
| `tools` | tool registry, tool executions, approvals | `ToolExecutionRequested`, `ToolExecuted` | tenancy, integrations |
| `channels` | channel accounts, inbound/outbound adapters, delivery status | `InboundMessageNormalized` | tenancy |
| `integrations` | connections, encrypted secrets, OAuth tokens, webhooks, health | `IntegrationHealthChanged` | tenancy |
| `feedback` | CSAT ratings and comments | `FeedbackSubmitted` | conversations |
| `analytics` | read models, aggregates, dashboards | — | events from all modules |
| `notifications` | in-app, email, push notifications, preferences | — | events |
| `billing` | plans, subscriptions, usage meters, quotas, invoices | `QuotaExceeded` | tenancy, usage events |
| `audit` | append-only audit trail | — | events + direct audit API |
| `platform_ops` | platform users, impersonation sessions, system health, provider config | — | identity |

Rules:

- Arrows only point "down" this table; `analytics`, `notifications`, and `audit` consume events and
  are never called synchronously by producers.
- `ai_runtime` depends on **interfaces** (ports) for knowledge retrieval and tool execution, not on
  their implementations.

---

## 4. Code organization

### 4.1 Backend (Cargo workspace)

```
backend/
  Cargo.toml                    # workspace
  crates/
    kernel/                     # shared primitives only: ids (newtypes), TenantContext, Clock,
                                #   error model, pagination, money, event envelope
    platform/                   # adapters: db pool + tx + RLS, redis, object storage, telemetry,
                                #   outbox, job queue, http client w/ SSRF guard, crypto
    modules/
      conversations/
        src/
          domain/               # aggregates, value objects, domain events, invariants (pure Rust)
          application/          # commands, queries, ports (traits), use-case services
          infrastructure/       # SQLx repositories, adapters implementing ports
          api/                  # Axum routes, DTOs, extractors, OpenAPI annotations
          lib.rs                # public interface: only application facade + event types
      knowledge/ ...
      ...
  apps/
    api/                        # composition root: wires modules, middleware, routes
    gateway/
    worker/
    scheduler/
  migrations/                   # SQLx migrations, prefixed per module
  tests/                        # cross-module integration and tenant-isolation suites
```

- Module crates expose only their facade (`pub use application::Facade`) and event types; internal
  modules are `pub(crate)`. The crate graph enforces "no cross-module internals" at compile time.
- `cargo-deny` enforces license and duplicate-dependency policy; a CI check rejects forbidden
  dependency edges (e.g. `domain` depending on `sqlx`).

### 4.2 Frontend (Angular)

```
frontend/src/app/
  core/          # auth, tenant context, http interceptors, error handling, app shell, global store
  shared/        # design-system wrappers, pipes, directives, utilities (no feature logic)
  features/
    conversations/
      data-access/   # API services (generated client), SignalStore, facades
      feature/       # routed smart components
      ui/            # presentational components (inputs/outputs only, OnPush)
    knowledge/ ...
  platform/      # platform-only features (tenants, providers, flags, health)
```

- Lint rules (`eslint-plugin-boundaries` or Nx module boundaries) prevent `features/a` importing
  `features/b`, and `ui` importing `data-access`.
- The widget is a **separate, tiny bundle** (target < 50 KB gzipped), built as a Web Component with
  Shadow DOM so host-page CSS can't break it and it can't leak into the host. Recommended tech:
  framework-light TypeScript or Lit rather than full Angular. **ADR-011.**

---

## 5. Multi-tenancy

### 5.1 Model

**Shared database, shared schema, `tenant_id` column + Row-Level Security** (pool model).

| Option | Isolation | Cost | Ops | Verdict |
| -- | -- | -- | -- | -- |
| Shared schema + RLS | Logical, DB-enforced | Lowest | Simplest | **Default** |
| Schema per tenant | Stronger | Medium | Migrations × N | Rejected for v1 |
| Database per tenant | Strongest | Highest | Heavy | Offered later as "dedicated cell" for enterprise |

The design keeps a **tenant → cell** mapping table from day one (all tenants map to `cell-1`), so a
tenant can later move to a dedicated database without changing application code (silo for
enterprise, pool for everyone else).

### 5.2 Enforcement in depth

1. **Request:** `TenantContext` extractor reads JWT + `X-Tenant-ID`, loads membership (cached),
   rejects if not a member (tenant users) or lacking platform permission (platform users).
   Returns `404`, not `403`, for tenants the caller can't see (avoid enumeration).
2. **Application:** repository methods take `&TenantContext`; the type can only be constructed by
   the authorization layer (private constructor). Forgetting the tenant is a compile error.
3. **Database:** each transaction runs `SET LOCAL app.tenant_id = $1`; RLS policies on every tenant
   table: `USING (tenant_id = current_setting('app.tenant_id')::uuid)`. The app connects as a
   non-owner role without `BYPASSRLS`. A separate migration role owns tables.
4. **Referential:** composite FKs `(tenant_id, conversation_id) REFERENCES conversations(tenant_id, id)`.
5. **Indexes:** every tenant table indexes with `tenant_id` as the leading column.
6. **Everything else:** Redis keys `t:{tenant_id}:…`, object keys `tenants/{tenant_id}/…`, vector
   queries always filtered by tenant, job payloads carry tenant ID and re-establish context.
7. **Tests:** a shared isolation test harness creates two tenants and runs every repository and
   endpoint against the wrong tenant.

### 5.3 Platform users and impersonation

- Platform users act in a tenant via an **impersonation session**: explicit start with reason,
  time-boxed (e.g. 60 min), visible banner, every action audited with `actor = platform user,
  on_behalf_of_tenant`. Destructive actions in a tenant can require a second confirmation.
- Platform permissions are separate (`platform.tenants.list`, `platform.billing.manage`, …).
  Full impersonation-session rules: **ADR-006**.

### 5.4 Noisy-neighbor protection

- Per-tenant rate limits (API, widget, inbound messages) and AI token budgets.
- Fair job scheduling: workers pick jobs round-robin across tenants (weighted by plan) rather than
  FIFO globally, so one tenant's 10,000-document import can't starve others.
- Per-tenant concurrency caps on AI turns and ingestion.

---

## 6. Identity, authentication, authorization

### 6.1 Authentication

- Passwords hashed with **Argon2id** (memory-hard; parameters tuned to ~250 ms).
- Access token: short-lived JWT (10–15 min), signed with **EdDSA (Ed25519)** or ES256, `kid` header
  for key rotation, JWKS endpoint. Claims: subject, identity type (platform/tenant), session ID.
  **No tenant roles in the JWT** — memberships change; they're resolved per request (cached).
- Refresh token: opaque, random 256-bit, stored hashed, **rotated on every use with reuse
  detection** (reuse of an old token revokes the whole session family).
- Browser storage: tokens in `HttpOnly; Secure; SameSite=Lax` cookies; CSRF protection via
  strict `Origin` check + custom header (no double-submit token). Never `localStorage`. **ADR-005.**
- MFA: TOTP first, WebAuthn/passkeys later; mandatory for platform users.
- Enterprise SSO (OIDC/SAML) and SCIM provisioning later; the identity model supports external
  identities from day one.
- Brute-force protection: per-account and per-IP throttling, exponential lockout, generic errors.

### 6.2 Authorization

- **RBAC with permissions**: roles are named bundles of permissions (`conversations.manage`,
  `knowledge_base.manage`, `ai_agent.manage`, `billing.manage`, …), named `resource.action`.
  Code checks permissions only.
- **Policy layer** for rules RBAC can't express (ABAC-style): "agents can only reply to
  conversations assigned to them unless Manager", "Viewer can't export PII". Implemented as a
  `Policy` trait per resource; Cedar-style engine considered later. **ADR-006.**
- Compile-time help: handlers declare `RequirePermission<P>`; a handler without it fails a CI lint
  that scans the router.
- API keys for server-to-server (integrations): scoped permissions, prefix-identifiable
  (`csk_live_…`), stored hashed, last-used tracking, rotation.

### 6.3 Widget and end-customer identity

- Widget uses a **publishable key** (not secret) + **allowed origins** list per tenant.
- Anonymous visitors get a signed, short-lived visitor token.
- Logged-in customers of the tenant are verified via **identity verification HMAC**: the tenant's
  server signs the customer ID with a secret; the widget sends the signature; we verify. Prevents
  customers impersonating each other to read history.

---

## 7. Data architecture

### 7.1 PostgreSQL conventions

- Primary keys: **UUIDv7** (time-ordered → good B-tree locality, sortable, no enumeration).
- Every table: `created_at`, `updated_at`; aggregates: `version` for optimistic concurrency.
- Soft delete only where business requires restore; otherwise hard delete + audit record.
- Money as integer minor units + currency code.
- Enums as `text` + `CHECK` constraints (easier to evolve than PG enums).
- Large append-only tables (`messages`, `audit_events`, `ai_turns`, `usage_events`) are
  **range-partitioned by month**; old partitions detached/archived per retention policy.
- Connection pooling through the app pool (SQLx) and **PgBouncer in transaction mode** in
  production (compatible with `SET LOCAL`).
- Read replicas for analytics and heavy reads when needed; writes always to primary.

### 7.2 Vector storage (pgvector)

- `knowledge_chunks(tenant_id, document_id, chunk_index, content, tsv tsvector, embedding vector(N),
  embedding_model, content_hash)`.
- **HNSW** index (better recall/latency than IVFFlat, no training step). Tune `m`, `ef_construction`,
  query-time `ef_search`.
- Tenant filtering with HNSW: use pgvector iterative index scans so the tenant filter doesn't
  starve results; for very large tenants, a partial index per tenant or per cell.
- Store `embedding_model` per row so models can be migrated gradually (re-embed in background,
  query by model).
- Escape hatch: if vector volume or latency outgrows Postgres, the retrieval port allows swapping in
  a dedicated vector DB (Qdrant, etc.) without touching the AI runtime. **ADR-008.**

### 7.3 Redis usage

| Use | Pattern |
| -- | -- |
| Membership/permission cache | Cache-aside, short TTL, invalidated by `RoleChanged`/`MemberRemoved` events |
| Rate limiting | GCRA / token bucket via Lua scripts (atomic) |
| Realtime fan-out | Pub/Sub channels per tenant/conversation for multi-instance SSE/WebSocket |
| Presence | Agent heartbeats with TTL keys |
| Distributed locks | Short leases only (leader election for scheduler); never for correctness that the DB can guarantee |
| Idempotency keys (hot) | `SET NX` with TTL; durable copy in Postgres for long windows |

Redis is treated as **disposable**: losing it degrades performance, never correctness.

### 7.4 Object storage

- Keys: `tenants/{tenant_id}/{module}/{uuid}`; never user-supplied file names in keys.
- Uploads via **pre-signed URLs** (client → storage directly), size limits, then server-side
  validation: magic-byte type detection, malware scan (ClamAV in worker), then promote from
  `quarantine/` to `clean/`.
- Server-side encryption; signed, short-lived download URLs.

---

## 8. Asynchronous processing

### 8.1 Transactional outbox

```
BEGIN;
  UPDATE conversations SET status = 'escalated' ...;
  INSERT INTO outbox (id, tenant_id, type, payload, trace_context) VALUES (...);
COMMIT;
-- dispatcher: SELECT ... FROM outbox WHERE dispatched_at IS NULL
--             ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 100
```

- Dispatcher delivers to in-process handlers and enqueues jobs; marks dispatched.
- **At-least-once** delivery → every consumer is **idempotent** (inbox table keyed by
  `(consumer, event_id)`).
- Trace context is stored with the event so traces continue across async boundaries.

### 8.2 Job queue

- v1: **PostgreSQL-backed queue** using `FOR UPDATE SKIP LOCKED` — transactional with business
  data, no new infrastructure, good to thousands of jobs/sec.
- Features: priorities, scheduled run-at, per-tenant fairness, max attempts, **exponential backoff
  with full jitter**, visibility timeout/heartbeat for long jobs, **dead-letter** status with
  replay from the platform UI.
- Migrate hot queues to Redis Streams or NATS only when measured throughput requires. **ADR-007.**

### 8.3 Sagas / process managers

Multi-step flows with waiting and compensation are modeled as explicit state machines persisted
in the DB:

- **Escalation saga:** AI requests handoff → find agent → offer → accept/timeout → re-route → queue.
- **Tool approval saga:** tool requested → approval pending → approved/denied/expired → execute →
  record → notify.
- **Ingestion pipeline:** uploaded → scanned → parsed → chunked → embedded → indexed (or failed at
  a step with retry from that step).

---

## 9. Realtime

- **SSE** for server→client streams (AI token streaming, inbox updates) — simple, HTTP/2 friendly,
  auto-reconnect with `Last-Event-ID`. **WebSocket** where bidirectional is needed (agent typing,
  widget chat). **ADR-009.**
- Multi-instance fan-out through Redis Pub/Sub; clients resume from the last event ID using a
  short per-conversation replay buffer.
- Backpressure: bounded channels per connection; slow consumers are dropped and must resync.
- Authorization re-checked on subscribe and on token refresh; tenant-scoped channel names.

---

## 10. AI runtime

### 10.1 Turn pipeline

Implemented as a **pipeline of stages** (Chain of Responsibility), each stage a small, testable unit:

1. **Resolve** tenant, channel, customer, conversation.
2. **Load** agent config + published prompt version + business rules.
3. **Pre-guards:** rate/budget check, input moderation, PII detection, prompt-injection heuristics,
   "always escalate" rules (e.g. legal threats, refund over threshold).
4. **Retrieve** knowledge (hybrid search, §10.3).
5. **Assemble context** within the model's token budget (§10.4).
6. **Generate** via provider abstraction with streaming.
7. **Tool loop:** if the model requests a tool → validate → authorize → approval policy → execute →
   feed result back; bounded by max iterations and total time.
8. **Post-guards:** output moderation, groundedness/citation check, PII leak check, policy rules.
9. **Decide:** send, send with handoff offer, or escalate (low confidence, guardrail hit, customer
   asks for human, sentiment drop).
10. **Persist** turn, citations, tool calls, usage, cost; emit events.

### 10.2 Provider abstraction

- `LlmProvider` port: `complete`, `stream`, `embed`, `capabilities()` (context window, tools,
  vision, JSON mode), normalized `Usage` and `ProviderError` (rate_limited, overloaded,
  context_too_long, content_filtered, auth, transient).
- Adapters: OpenAI, Anthropic, Gemini; later local (vLLM/Ollama).
- **Resilience per provider:** timeout, retry on transient errors only, **circuit breaker**,
  bulkhead (separate concurrency semaphore per provider), **fallback chain** configured per tenant
  (e.g. primary model → secondary model on another provider).
- **Model router** (Strategy): choose model by task — cheap fast model for classification/routing/
  summaries, stronger model for answers.
- Prompt caching used where the provider supports it (static system prompt + rules first in the
  prompt to maximize cache hits).

### 10.3 Retrieval (RAG)

**Ingestion**

- Parse by type (PDF, DOCX, HTML, Markdown, URLs/sitemaps, FAQ entries).
- Normalize text, strip boilerplate, keep structure (headings → chunk metadata).
- **Chunking:** recursive structure-aware splitting (headings → paragraphs → sentences) with target
  size ~300–800 tokens and 10–15% overlap; FAQ entries stay one chunk per Q&A.
- **Content hashing (SHA-256)** per chunk → re-ingestion only re-embeds changed chunks.
- Batch embeddings with rate-limit-aware concurrency.

**Query**

- Optional **query rewriting** (resolve follow-ups using conversation history into a standalone query).
- **Hybrid search:** lexical (PostgreSQL full-text `tsvector`, BM25-like ranking) + semantic (HNSW
  cosine), fused with **Reciprocal Rank Fusion** (`score = Σ 1/(k + rank)`, k≈60). Hybrid handles
  exact terms (SKUs, error codes) that pure vectors miss.
- **Maximal Marginal Relevance (MMR)** to remove near-duplicate chunks and increase coverage.
- Optional **re-ranking** with a cross-encoder or provider rerank API for the top-N.
- Relevance threshold: if nothing clears it, the agent says it doesn't know or escalates — no
  ungrounded answers.
- Results cached per `(tenant, normalized query hash, knowledge version)`.

### 10.4 Context assembly

- Budget = model context − reserved output − system/rules − tool schemas.
- Fill with priority: business rules → retrieved chunks (by fused score) → conversation summary →
  recent messages (newest first). A greedy knapsack by value/token cost is enough.
- **Rolling conversation summaries** generated asynchronously after N turns keep long
  conversations within budget.
- Token counting uses the provider's tokenizer (or a calibrated estimate with safety margin).

### 10.5 Guardrails against prompt injection and data leaks

- **Instruction hierarchy:** system rules > tenant config > retrieved content/customer input.
  Retrieved and customer text is wrapped in delimited, labeled blocks ("untrusted content").
- Tools are the only way to act; the model never sees secrets or raw credentials.
- Tool allowlist per agent; arguments validated against JSON Schema; IDs in arguments re-checked
  against tenant scope (the model can't reach another tenant even if tricked).
- Output filters: PII redaction rules, link allowlist, policy checks.
- Red-team test set (injection attempts, jailbreaks, data exfiltration prompts) runs in the eval
  suite on every prompt or pipeline change.

### 10.6 Evaluation and prompt lifecycle

- Prompt versions: draft → published (immutable) → archived; agent config pins a version.
- Offline **evaluation harness**: golden Q&A sets per tenant/domain; metrics for retrieval
  (recall@k, MRR), answer faithfulness, citation correctness, escalation precision; LLM-as-judge
  with human spot checks.
- Optional online A/B between prompt versions with deterministic bucketing
  (`hash(conversation_id) mod 100`).

### 10.7 Cost control

- Per-tenant monthly token/cost budgets with soft (warn) and hard (degrade/escalate) limits.
- Usage metered per turn into `usage_events`; aggregated for billing.

---

## 11. Human handoff and routing

### 11.1 Algorithm

```
eligible = agents where tenant matches
                   and status = online
                   and active_count < capacity
skilled  = eligible where required_skills ⊆ agent.skills          -- set containment
pool     = skilled if non-empty else (fallback_enabled ? eligible : ∅)
choose   = argmin over pool of (active_count / capacity,          -- least-loaded (normalized)
                                last_assigned_at)                 -- tie-break: longest idle (fairness)
if none  → enqueue in shared queue
```

- **Atomic assignment:** `UPDATE agent_load SET active = active + 1 WHERE agent_id = $1 AND
  active < capacity` (conditional update) inside the transaction that sets the assignment, so two
  concurrent escalations can't overfill an agent. Retry with next candidate on conflict.
- **Queue ordering:** priority, then waiting time with **aging** (priority increases with wait) to
  prevent starvation.
- **SLA timers:** deadlines stored as timestamps; the scheduler scans an index on `due_at`
  (timer-wheel equivalent using the DB) and emits `SlaBreached`.
- Offer/accept with timeout: if the agent doesn't accept within N seconds, re-route and record.
- Property-based tests: capacity never exceeded, skilled agents preferred, no conversation lost.

---

## 12. Channels and integrations

### 12.1 Channel adapters

- **Adapter + Anti-Corruption Layer:** each channel (widget, WhatsApp, Telegram, future SDKs)
  converts vendor payloads into a canonical `InboundMessage` and back. The conversation core never
  sees vendor formats.
- Inbound webhook handling: verify signature (HMAC-SHA256, constant-time compare) → check
  timestamp window (replay protection) → **dedupe by provider message ID** (inbox pattern) →
  persist raw payload → ACK fast → process async.
- Outbound: queued, per-channel **token-bucket rate limits** matching provider limits, retries with
  backoff, delivery status tracking (sent/delivered/read/failed), channel-specific constraints
  (e.g. WhatsApp 24-hour session window and templates).

### 12.2 Integrations and secrets

- **Envelope encryption:** each secret encrypted with a per-record data key (AES-256-GCM); data keys
  encrypted by a key-encryption key in a KMS (or a sealed master key locally). Supports key rotation
  and crypto-shredding on tenant deletion.
- OAuth connections: token refresh in background with locking per connection; refresh failures
  surface as integration health.
- **SSRF protection** for tenant-configured URLs (webhooks, HTTP tools): HTTPS only, resolve DNS
  and block private/link-local/metadata ranges (10/8, 172.16/12, 192.168/16, 169.254/16, ::1, fc00::/7),
  re-check after redirects (DNS-rebinding defense), egress through a dedicated proxy in production.
- Outbound webhooks to tenants are signed (HMAC with timestamp) and retried with backoff.

---

## 13. Security architecture

### 13.1 Threat modeling

- **STRIDE** per module during planning; results recorded in the spec's security section.
- Key threats and controls:

| Threat | Control |
| -- | -- |
| Cross-tenant data access | §5 defense in depth, isolation tests |
| Account takeover | Argon2id, MFA, throttling, refresh rotation + reuse detection |
| Privilege escalation via tenant header | Server-side membership validation every request |
| Prompt injection → unwanted action | Tool allowlist, schema validation, server-side authz, approvals |
| Data exfiltration via AI output | Tenant-scoped retrieval only, output filters, no secrets in context |
| Malicious uploads | Pre-signed upload, magic-byte check, AV scan, quarantine |
| SSRF via integrations | §12.2 egress controls |
| Webhook spoofing/replay | Signatures, timestamps, idempotency |
| Widget abuse / spam | Origin allowlist, rate limits, bot challenge escalation, visitor tokens |
| Supply chain | Pinned deps + lockfiles, `cargo-audit`, `cargo-deny`, `npm audit`, Dependabot, SBOM, pinned GitHub Actions by SHA, image signing |
| Insider misuse | Impersonation sessions, audit log, least privilege for platform roles |
| Log/PII leakage | Structured logging with redaction, no tokens/secrets ever logged |

### 13.2 Application security controls

- HTTP: TLS 1.2+ (1.3 preferred), HSTS, strict **CSP** (no inline scripts; Trusted Types in
  Angular), `X-Content-Type-Options`, `Referrer-Policy`, frame-ancestors restrictions (widget
  embedding handled explicitly).
- CORS: dashboard origin only for `api`; widget origins per tenant for `gateway`.
- Input validation at DTO level (Serde + validator), size limits on bodies and fields.
- Output encoding by Angular templates; no `bypassSecurityTrust*` without review.
- **Audit log:** append-only table, no UPDATE/DELETE grants, each row stores
  `hash = SHA-256(prev_hash || row)` → **hash chain** makes tampering detectable.
- Secrets management: environment/KMS-backed, never in repo; secret scanning in CI (gitleaks).
- Privacy: data classification (public / internal / confidential / PII), retention policies per
  tenant, export and delete endpoints, data residency via cells later.

---

## 14. Scalability

| Concern | Approach |
| -- | -- |
| API throughput | Stateless Axum instances behind a load balancer; horizontal autoscaling |
| DB connections | PgBouncer transaction pooling; bounded pools per process |
| Read load | Query-model tables/materialized views for inbox & analytics; read replicas later |
| Write load | Partitioned append-only tables; batched inserts for usage/analytics events |
| Background load | Independent worker pool scaling on queue depth/age; per-queue concurrency |
| AI latency | Streaming, model routing, prompt caching, retrieval caching |
| Hot tenants | Per-tenant quotas, fair scheduling, cell migration for very large tenants |
| Widget assets | Static bundle on CDN with long-cache, versioned URLs |
| Pagination | Keyset (`WHERE (created_at, id) < ($1,$2)`) instead of OFFSET |
| Future sharding | Tenant → cell routing table; cells can use consistent hashing for assignment |

Capacity planning starts with a baseline load test (k6) per phase; budgets in the constitution
(Principle XI) are verified, not assumed.

---

## 15. Reliability

- **Timeouts** on every I/O; request-level deadline propagated to downstream calls.
- **Retries** only for idempotent operations, exponential backoff with full jitter, capped.
- **Circuit breakers** for AI providers, channels, integrations; **bulkheads** (separate semaphores
  and pools) so AI slowness can't exhaust DB connections or HTTP workers.
- **Graceful degradation:** AI down → route to human queue with an honest customer message;
  Redis down → bypass cache, use DB-backed rate limits with relaxed limits.
- **Graceful shutdown:** stop accepting, drain in-flight requests and jobs, release leases.
- **Health:** `/livez` (process alive), `/readyz` (DB, Redis, storage reachable).
- **SLOs:** API availability 99.9%, message ingest-to-persist p99 < 1 s; error budgets drive
  release pace.
- **Backups:** PostgreSQL PITR (WAL archiving), daily snapshots, quarterly restore drills;
  targets RPO ≤ 5 min, RTO ≤ 1 h. Object storage versioning.

---

## 16. Observability

- **OpenTelemetry** traces: HTTP → module → DB/Redis → outbox → worker → AI provider → channel.
- Structured logs (JSON) with `trace_id`, `request_id`, `tenant_id`, `actor_id`, redaction layer.
- Metrics: RED per endpoint/module; USE for pools; queue depth/age; AI tokens, cost, latency,
  first-token time, escalation rate, retrieval hit rate per tenant.
- Business telemetry flows to analytics through events, not logs.
- Alerting on SLO burn rate, DLQ growth, provider circuit open, isolation-test failure in CI.
- Audit logs (who did what) are separate from application logs (what the system did).

---

## 17. Frontend architecture details

- **Zoneless** Angular with Signals and `OnPush` everywhere.
- **Facade pattern**: components talk to a feature facade; facades hide NgRx Store/SignalStore.
- State ownership (from the roadmap): NgRx Store for identity, active tenant, global UI prefs,
  notifications; SignalStore per feature; local signals for ephemeral UI.
- **Interceptors (Chain of Responsibility):** auth/refresh (single-flight refresh to avoid token
  stampede), `X-Tenant-ID`, request ID, error normalization (Problem Details → UI errors), retry
  for idempotent GETs.
- Generated, typed API client from OpenAPI; no hand-written DTOs.
- Realtime service wraps SSE/WebSocket with reconnect (exponential backoff + jitter) and resync.
- Tenant switch = full state reset of tenant-scoped stores (meta-reducer), preventing stale data
  from another tenant appearing on screen.
- Performance: lazy routes, `@defer` blocks, virtual scrolling for inbox and message lists,
  image optimization.
- i18n with RTL support (Arabic is a likely early market); dates/numbers localized.

---

## 18. Design patterns catalog

| Pattern | Where |
| -- | -- |
| Hexagonal / Ports & Adapters | Every backend module |
| Repository | Aggregate persistence (SQLx) |
| Unit of Work | Transaction wrapper carrying tenant context + outbox writes |
| Aggregate / Value Object / Domain Event | DDD domain layer (Conversation, Document, AgentConfig) |
| Newtype | IDs and validated values in Rust (`TenantId`, `Email`, `Slug`) |
| Typestate | Compile-time states (e.g. `UnverifiedUpload` → `ScannedUpload`; `Authorized<P>`) |
| Command / Query handlers (CQRS-lite) | Application layer |
| Mediator / in-process event bus | Outbox dispatcher → handlers |
| Transactional Outbox / Inbox | Event publishing and idempotent consumption |
| Saga / Process Manager | Escalation, tool approval, ingestion |
| State Machine | Conversation status, assignment, job status, prompt version lifecycle |
| Strategy | Model routing, routing policy, chunking strategy, retrieval strategy |
| Adapter | AI providers, channels, storage, integrations |
| Anti-Corruption Layer | Channel and third-party payload translation |
| Abstract Factory / Registry | Provider and tool registries |
| Chain of Responsibility / Pipeline | AI turn stages, Tower middleware, Angular interceptors |
| Decorator | Caching, metrics, retries wrapping ports (Tower layers) |
| Circuit Breaker, Bulkhead, Retry, Timeout | External calls |
| Specification | Composable eligibility rules (routing, escalation rules) |
| Policy | Authorization rules beyond RBAC |
| Template Method | Channel adapter lifecycle (verify → normalize → persist → ack) |
| Observer / Pub-Sub | Realtime updates, notifications |
| Facade | Module public interfaces; Angular feature facades |
| Builder | Prompt/context assembly, complex test fixtures |
| Null Object / Safe fallback | AI fallback responses when providers fail |
| Feature Flags | Gradual rollout per tenant |
| Smart/Presentational components | Angular UI |
| Strangler Fig (future) | Extracting modules into services |

---

## 19. Algorithms catalog

| Problem | Algorithm / technique |
| -- | -- |
| Password storage | Argon2id |
| Token signing | Ed25519 (EdDSA) JWT, key rotation via `kid` |
| Secret encryption | AES-256-GCM envelope encryption |
| Webhook verification | HMAC-SHA256, constant-time compare, timestamp window |
| Audit tamper evidence | SHA-256 hash chain |
| IDs | UUIDv7 (time-ordered) |
| Rate limiting | GCRA / token bucket (Redis Lua, atomic); sliding window for login throttling |
| Retry timing | Exponential backoff with full jitter |
| Job claiming | `FOR UPDATE SKIP LOCKED` |
| Fair scheduling | Weighted round-robin across tenants (deficit round-robin) |
| Agent routing | Set containment for skills + least-normalized-load + longest-idle tie-break |
| Queue starvation | Priority aging |
| SLA timers | Indexed deadline scan (DB-backed timer wheel) |
| Chunking | Recursive structure-aware splitting with overlap; token counting |
| Change detection | SHA-256 content hashing per chunk |
| Vector search | HNSW (approximate nearest neighbors), cosine similarity |
| Lexical search | PostgreSQL full-text search (`ts_rank_cd`), trigram for fuzzy names |
| Result fusion | Reciprocal Rank Fusion |
| Diversity | Maximal Marginal Relevance |
| Context packing | Greedy knapsack by relevance per token |
| Near-duplicate detection | MinHash / SimHash for duplicate documents (later) |
| A/B bucketing | Deterministic hashing (`xxhash(id) mod 100`) |
| Cache stampede | Single-flight + probabilistic early expiration |
| Pagination | Keyset/seek pagination |
| Analytics roll-ups | Incremental aggregation into time buckets; HyperLogLog for unique visitors (later) |
| Customer identity merge | Union-find over matched identifiers (email/phone/channel IDs) |
| Future sharding | Consistent hashing / lookup-table routing to cells |

---

## 20. Testing architecture

| Layer | Tooling | Focus |
| -- | -- | -- |
| Rust unit | `cargo test`, `proptest` | Domain invariants, algorithms |
| Rust integration | Testcontainers (Postgres+pgvector, Redis, MinIO) | Repositories, RLS, outbox, queue |
| Tenant isolation | Shared harness, runs per module | Wrong-tenant access on every repo/endpoint |
| API contract | OpenAPI schema validation (e.g. Schemathesis) | Request/response conformance, fuzzing |
| Frontend unit/component | Angular testing utilities | Components, stores, facades |
| E2E | Playwright | Critical journeys: login, tenant switch, chat → AI → escalate → resolve |
| AI evals | Eval harness + golden sets | Retrieval quality, faithfulness, injection resistance |
| Load | k6 | Budgets per phase |
| Security | SAST (clippy lints, semgrep), DAST (OWASP ZAP baseline), dependency scans | Pre-release |

---

## 21. Evolution path

1. **Now:** modular monolith, one Postgres, Postgres queue, one cell.
2. **Growth:** read replicas, PgBouncer, partition maintenance, separate worker pools per queue,
   CDN for widget.
3. **Scale:** extract the highest-pressure modules if measurements justify — likely candidates
   `gateway`/channels, `ai_runtime`, and `knowledge` ingestion — via Strangler Fig; move queues to
   NATS/Redis Streams; dedicated vector store if needed.
4. **Enterprise:** dedicated cells per large tenant, data residency, SSO/SCIM, customer-managed
   keys.

---

## 22. ADR backlog (Phase 0)

| ADR | Decision |
| -- | -- |
| ADR-001 | Modular monolith with hexagonal modules |
| ADR-002 | Rust/Axum/SQLx backend stack |
| ADR-003 | Angular 22 + NgRx Store/SignalStore state ownership |
| ADR-004 | Multi-tenancy: shared schema + RLS + composite FKs + cell mapping |
| ADR-005 | Authentication: short JWT + rotating refresh tokens in HttpOnly cookies |
| ADR-006 | Authorization: permission-based RBAC + policy layer |
| ADR-007 | PostgreSQL job queue + transactional outbox |
| ADR-008 | pgvector HNSW + hybrid search with RRF |
| ADR-009 | SSE for streaming, WebSocket for bidirectional chat |
| ADR-010 | Separate `gateway` deployment unit for public traffic |
| ADR-011 | Widget as lightweight Web Component (non-Angular) |
| ADR-012 | API conventions: `/api/v1`, OpenAPI 3.1, RFC 9457 errors, keyset pagination |
| ADR-013 | UUIDv7 identifiers |
| ADR-014 | Envelope encryption for tenant secrets |
| ADR-015 | Monorepo layout and tooling |