<!--
Sync Impact Report
==================
Version change: 2.0.0 → 2.0.1 (PATCH — follow-up TODOs resolved; no principle changes)

Modified principles: none
Added sections: none
Removed sections: none

Resolved follow-up TODOs (from 2.0.0):
  - TODO(ARCH_DOC): `docs/architecture/system-architecture.md` is on master (MOH-14, PR #9).
  - TODO(ADR_DIR): `docs/adr/` is on master with the template, index and ADR-001–015,
    all Accepted (epic MOH-6, PRs #9–#12).

Templates:
  - .specify/templates/plan-template.md ✅ no change needed
  - .specify/templates/spec-template.md ✅ no change needed
  - .specify/templates/tasks-template.md ✅ no change needed
  - .specify/templates/checklist-template.md ✅ no change needed
  - CLAUDE.md, frontend/CLAUDE.md, CONTRIBUTING.md ✅ no principle references changed

Follow-up TODOs:
  - TODO(COMPLIANCE_GAP): existing code (specs 005–033) was built under 1.2.0. Each ADR's
    "Current state and migration" table in `docs/adr/` lists its gaps with file references,
    and `docs/security/threat-model.md` §5 ranks the top risks. Tracked so far:
      - No PostgreSQL RLS (I): MOH-31, blocked by MOH-23 (DB role split) — ADR-004 T1/T2
      - Internal error text returned to clients (II, VIII): MOH-32 — ADR-012 A1
      - Tenant secrets not envelope-encrypted (II): MOH-33 — ADR-014 E1
    Still untracked: no i18n externalization (XII); UUIDv7 not used for new PKs
    (Technology Standards, ADR-013 U1); no generated TypeScript client check in CI
    (VIII, ADR-012 A5).
-->

# AI Customer Service Platform Constitution

This constitution defines the non-negotiable rules for designing, building, and operating the
AI Customer Service Platform: a multi-tenant SaaS that lets businesses run AI support agents with
knowledge retrieval, safe tool calling, omnichannel conversations, and human handoff.

Every spec, plan, task, and pull request MUST comply. Where a rule cannot be met, the deviation
MUST be recorded in the plan's Complexity Tracking table with a justification and an ADR.

Keywords MUST, MUST NOT, SHOULD, and MAY follow RFC 2119.

Detailed architecture guidance lives in `docs/architecture/system-architecture.md`.
This document states the rules; the architecture document explains how to satisfy them.

---

## Core Principles

### I. Tenant Isolation Is Non-Negotiable

Cross-tenant data exposure is the single worst failure this product can have.

- Every tenant-owned table MUST carry a non-null `tenant_id`, and every tenant-owned query and
  mutation MUST be scoped by it.
- Tenant scope MUST be enforced in at least two independent layers:
  1. Application layer: repositories accept a validated `TenantContext`; there is no API that
     reads tenant data without one.
  2. Database layer: PostgreSQL Row-Level Security policies keyed on a per-transaction
     `app.tenant_id` setting.
- Foreign keys between tenant-owned tables MUST be composite `(tenant_id, id)` so a row can never
  reference another tenant's row.
- Client-supplied tenant identifiers (e.g. `X-Tenant-ID`) MUST be validated server-side against
  membership or platform privilege on every request. Frontend guards are UX, never security.
- Caches, object-storage keys, vector indexes, queues, logs, and metrics MUST be tenant-namespaced.
- Every module that owns tenant data MUST ship automated cross-tenant access tests
  (tenant A cannot read, list, update, or delete tenant B's data, including via IDs guessed from
  another tenant).

### II. Secure by Default

- Deny by default. Every endpoint MUST declare the permission it requires; an endpoint without a
  declared permission MUST fail to compile or fail CI.
- Authorization MUST check permissions, not role names. Roles map to permission sets.
- The security baseline is OWASP ASVS Level 2 and the OWASP Top 10 for LLM Applications.
- Secrets MUST live server-side only, encrypted at rest (envelope encryption), never in frontend
  configuration, logs, prompts, error messages, or source control.
- All external input (HTTP, webhooks, files, channel messages, LLM output, retrieved documents)
  MUST be treated as untrusted and validated at the boundary.
- Inbound webhooks MUST verify signatures with constant-time comparison and reject replays.
- Outbound requests to tenant-configured URLs MUST pass SSRF protection.
- Sensitive administrative and business actions MUST write an append-only audit record.
- Platform users entering a tenant context MUST be audited with actor, tenant, reason, and time.
- Dependencies MUST pass vulnerability and license scanning in CI.

### III. AI Is Constrained, Observable, and Accountable

- LLMs MUST NOT receive direct database, filesystem, network, or secret access. All actions go
  through registered tools.
- Every tool MUST declare: input JSON Schema, required permission, tenant scope, side-effect class
  (read / write / irreversible), approval policy, timeout, and idempotency behavior.
- Tool arguments produced by an LLM MUST be schema-validated and authorization-checked server-side
  before execution. Irreversible tools MUST require explicit approval unless the tenant opts in.
- Retrieved knowledge and customer messages MUST be treated as data, never as instructions, and
  MUST be clearly delimited in prompts.
- AI responses that cite knowledge MUST store citations to the source chunks.
- Every AI turn MUST record provider, model, prompt version, token usage, latency, cost, tool calls,
  and outcome.
- Prompts MUST be versioned and immutable once published. Changes create a new version.
- The system MUST degrade safely: on provider failure, budget exhaustion, low confidence, or a
  guardrail violation, the conversation escalates to a human or returns a safe fallback. It MUST
  NOT fabricate an answer.
- AI provider access MUST go through the provider abstraction. Feature code MUST NOT call a vendor
  SDK directly.

### IV. Modular Monolith With Enforced Boundaries

- The backend is a single deployable Rust codebase organized as bounded-context modules.
- Each module owns its data. Modules MUST NOT read or write another module's tables.
- Cross-module interaction happens only through the other module's public application interface
  or through domain events. Circular dependencies are forbidden and MUST be enforced by the
  crate graph.
- Inside a module, dependencies point inward: `api → application → domain`, with
  `infrastructure` implementing ports declared by `application`/`domain` (hexagonal architecture).
- Domain code MUST NOT depend on Axum, SQLx, Redis, or any vendor SDK.
- Extracting a module into a separate service requires a measured reason (load, isolation,
  deployment cadence) documented in an ADR.

### V. Spec-Driven, Incremental Delivery

- Work proceeds spec → clarify → plan → tasks → implement, one bounded spec at a time.
- Every spec MUST state scope, non-goals, security and tenancy impact, data changes, API contracts,
  test strategy, and measurable acceptance criteria.
- Every task MUST be independently verifiable and produce working code with tests.
- Placeholder modules, stubbed business logic, or "TODO: implement" paths MUST NOT be merged as
  complete work. Visual mock fixtures are allowed only when isolated from production services and
  explicitly in scope.
- A spec is complete only when its acceptance criteria are verified, not when files exist.

### VI. Tests Are Part of the Change

- Every behavior change MUST include tests at the lowest effective level:
  - Unit tests for domain logic and algorithms.
  - Integration tests against real PostgreSQL and Redis (containers), not mocks, for repositories,
    migrations, RLS, and queues.
  - Contract tests for HTTP APIs against the OpenAPI specification.
  - Component tests for Angular UI; end-to-end tests for critical user journeys.
- Security-relevant code MUST have negative tests (unauthorized, wrong tenant, malformed input).
- Algorithms with invariants (routing, rate limiting, chunking, ranking) SHOULD have
  property-based tests.
- Every bug fix MUST add a regression test that fails before the fix.
- AI behavior changes (prompts, retrieval, guardrails) MUST run the evaluation suite and MUST NOT
  regress agreed metrics without explicit sign-off.
- CI MUST be green (format, lint, type check, build, tests, security scans) before merge.

### VII. Observability Is a Feature

- All services MUST emit structured JSON logs, OpenTelemetry traces, and metrics.
- Every request and job MUST carry a request/trace ID propagated across HTTP, queues, workers,
  channel adapters, and AI provider calls.
- Logs MUST include tenant ID and actor where applicable and MUST NOT include secrets, tokens,
  passwords, or raw personal data beyond what is required.
- Each module MUST expose RED metrics (rate, errors, duration); queues MUST expose depth and age.
- Liveness and readiness endpoints are mandatory; readiness reflects real dependency health.

### VIII. Explicit Contracts and Type Safety

- Rust: no `unwrap`/`expect`/`panic!` in request or job paths; errors use typed enums mapped to a
  single public error model. Identifiers use newtypes (`TenantId`, `ConversationId`, …) so IDs
  cannot be mixed up. SQL is compile-time checked with SQLx.
- TypeScript: `strict` mode, no `any` without a justified, linted exception.
- The HTTP API is versioned (`/api/v1`), documented in OpenAPI generated from code, and the
  frontend client is generated from that contract.
- Breaking API changes require a new version or a documented deprecation window.
- Errors returned to clients follow RFC 9457 Problem Details and never leak internals.

### IX. Simplicity and Evolvability

- Prefer the simplest design that satisfies current requirements and keeps future options open.
- New infrastructure (message broker, search engine, service split) requires an ADR with the
  problem, alternatives, and the measurement that justifies it.
- Significant architectural decisions MUST be recorded as ADRs in `docs/adr/`.
- Code MUST be readable by a new engineer: clear naming, small functions, module READMEs stating
  responsibilities, public interface, owned data, and emitted events.

### X. Data Integrity and Safe Change

- All schema changes go through versioned, reviewed migrations. Manual production DDL is forbidden.
- Migrations MUST be backward compatible with the currently deployed code (expand → migrate →
  contract) so deployments are zero-downtime.
- Writes that can be retried (API commands, webhooks, jobs, tool executions) MUST be idempotent,
  using idempotency keys or natural keys.
- State changes that must publish events MUST use the transactional outbox; dual writes are
  forbidden.
- Concurrent updates to shared aggregates MUST use optimistic concurrency (version column) or
  explicit row locks.
- Personal data MUST support per-tenant export and deletion; retention periods are configurable.

### XI. Scalable and Resilient by Design

- API and worker processes MUST be stateless; state lives in PostgreSQL, Redis, or object storage.
- Every outbound call (DB, Redis, AI provider, channel, integration) MUST have a timeout.
  Retries MUST use exponential backoff with jitter and only for retryable, idempotent operations.
- External dependencies MUST be isolated with circuit breakers and bulkheads so a failing provider
  cannot exhaust shared resources.
- Long or bursty work (ingestion, embeddings, outbound messages, webhooks) MUST run in background
  workers, not in request handlers.
- Per-tenant rate limits and quotas MUST protect the platform from noisy neighbors.
- Lists MUST be paginated (keyset pagination for large or real-time collections).
- Each spec touching hot paths MUST state its performance budget; defaults: API p95 < 300 ms
  excluding AI generation; first AI token p95 < 2.5 s.

### XII. Consistent, Accessible User Experience

- UI MUST be built from the Design System (Taiga UI wrapped by product components and design
  tokens); one-off styling of shared patterns is not allowed.
- Interfaces MUST meet WCAG 2.2 AA, support keyboard navigation, light and dark themes, and
  right-to-left layouts.
- Every data view MUST handle loading, empty, error, and permission-denied states.
- User-facing text MUST be externalized for internationalization.

---

## Technology Standards

| Area | Standard |
| -- | -- |
| Frontend | Angular 22, standalone components, Signals, zoneless change detection, strict TypeScript |
| Frontend state | NgRx Store/Effects for global state; NgRx SignalStore for feature state; signals for component state. One writable owner per piece of state |
| UI | Taiga UI wrapped by product components; CSS design tokens; light/dark/RTL |
| Backend | Rust (stable), Axum, Tokio, Tower middleware, SQLx, Serde, Tracing + OpenTelemetry |
| Data | PostgreSQL (with pgvector, RLS), Redis, S3-compatible object storage (MinIO local; R2/S3 prod) |
| AI | Provider-independent abstraction; OpenAI, Anthropic, Gemini adapters; local models later |
| Contracts | OpenAPI 3.1 generated from code; generated TypeScript client; RFC 9457 errors |
| IDs | UUIDv7 for all primary keys |
| Delivery | Docker, GitHub Actions, Conventional Commits, trunk-based development with short-lived branches |

Adding or replacing a technology in this table requires an ADR and a constitution amendment.

---

## Development Workflow and Quality Gates

1. **Spec** (`/speckit.specify`): problem, scope, non-goals, users and roles, tenancy and security
   impact, data, API, UX states, acceptance criteria.
2. **Clarify** (`/speckit.clarify`): resolve ambiguities before planning.
3. **Plan** (`/speckit.plan`): MUST include a Constitution Check against Principles I–XII and list
   any deviation in Complexity Tracking.
4. **Tasks** (`/speckit.tasks`): small, ordered, independently verifiable; each with tests.
5. **Implement** (`/speckit.implement`): one task per cycle; run format, lint, build, and tests.
6. **Review**: pull request linked to its Linear issue; at least one approval; checklist covers
   tenancy, authorization, input validation, error handling, observability, tests, and docs.
7. **QA**: verify acceptance criteria in the QA environment before Done.

**Definition of Done**

- Acceptance criteria verified.
- Tests added and passing; coverage of new domain logic ≥ 80% lines.
- Tenant-isolation and authorization tests present where tenant data or permissions are touched.
- No new lint warnings; no unresolved review comments.
- OpenAPI, ADRs, module READMEs, and user docs updated to match the code.
- Observability (logs, traces, metrics) in place for new flows.
- Migrations reversible or explicitly documented as forward-only.

---

## Governance

- This constitution supersedes conflicting conventions, templates, and habits.
- Amendments require a pull request that explains the change and its impact, updates dependent
  templates, and adds a Sync Impact Report.
- Versioning follows semantic versioning:
  - MAJOR: removing or redefining a principle in a backward-incompatible way.
  - MINOR: adding a principle or materially expanding guidance.
  - PATCH: clarifications and wording fixes.
- Every plan MUST pass the Constitution Check; every pull request review MUST confirm compliance.
- Compliance is reviewed at the end of each phase; recurring violations trigger an amendment or a
  corrective spec.

**Version**: 2.0.1 | **Ratified**: 2026-07-03 | **Last Amended**: 2026-11-07
