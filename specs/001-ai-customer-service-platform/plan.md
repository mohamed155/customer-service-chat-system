# Implementation Plan: AI Customer Service Platform

**Branch**: `001-ai-customer-service-platform` | **Date**: 2026-07-03 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-ai-customer-service-platform/spec.md`

**Note**: This plan is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Build a multi-tenant, enterprise AI customer service platform as a Rust modular
monolith (Axum/Tokio/SQLx/PostgreSQL/Redis/pgvector) with an Angular workspace
(dashboard + embeddable chat widget). Delivery is organized into eight
milestones (M0–M7), each producing a usable, deployable system. Foundations —
infrastructure, observability, authentication, tenancy, RBAC — are completed
(M0–M1) before any conversation or AI feature is built (M2+), per the
Constitution's "maintainability over premature features" mandate and the
explicit instruction to avoid AI work before auth, tenancy, observability, and
infrastructure are complete.

## Technical Context

**Language/Version**: Backend: Rust (stable, edition 2024, pin latest stable ≥1.85). Frontend: TypeScript 5.x on Angular 19+ (standalone components, Signals).

**Primary Dependencies**: Axum, Tokio, SQLx, Serde, Tracing (backend); Angular, RxJS, Angular Material + project design-token layer (frontend); provider APIs accessed via plain HTTPS clients behind the AI provider abstraction (no direct vendor SDK coupling). Design system: tokens extracted from `Helix Admin.html` (repo root) as the single visual source of truth, consumed via `libs/ui`; all component CSS follows BEM (`hx-block__element--modifier`).

**Storage**: PostgreSQL 16+ (primary, with pgvector for embeddings), Redis 7+ (cache, pub/sub, rate limiting, presence), S3-compatible object storage (knowledge files, exports, branding assets).

**Testing**: `cargo test` (unit + integration with testcontainers Postgres/Redis), API contract tests against the running Axum app, Angular unit tests, Playwright end-to-end suites, dedicated tenant-isolation test suite as a permanent release gate.

**Target Platform**: Linux containers (OCI), docker-compose for local dev, Kubernetes-ready for production; widget served as a static embeddable bundle via CDN.

**Project Type**: Web application — Rust API backend + Angular frontend workspace (dashboard app + widget app).

**Performance Goals**: AI first-token ≤3 s p95; agent message relay ≤500 ms p95; retrieval ≤1 s p95; dashboards interactive ≤2 s p95; 10k concurrent conversations, 1M conversations/month (spec NFR-PERF-*, NFR-SCAL-*).

**Constraints**: Hard tenant isolation on every query; deterministic prompt assembly; LLM access only via approved tools; streaming responses; 99.9% conversation-service availability; RPO ≤5 min, RTO ≤4 h; WCAG 2.1 AA.

**Scale/Scope**: v1 targets 1,000 active tenants, 500 seats/tenant, 100k customers/tenant, 10 GB knowledge/tenant; ~90 functional requirements across 18 domains; 8 milestones.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | How this plan complies |
|---|-----------|--------|------------------------|
| I | Enterprise Modular Monolith | ✅ PASS | Single deployable Rust binary; Cargo workspace with one crate per domain module; modules communicate via application services + domain events (in-process event bus with a transactional outbox); no cross-module table access |
| II | Multi-Tenant Isolation | ✅ PASS | `tenant_id` on every tenant-owned table; tenant context extracted once per request and threaded through a typed `TenantScope` that data-access APIs require; isolation test suite is a CI release gate from M1 |
| III | Zero-Trust Security & RBAC | ✅ PASS | Auth middleware on all routes; RBAC permission checks in application services (not just handlers); secrets via env/secret store; audit module receives domain events from M1 onward |
| IV | AI Provider Independence & Tool-Mediated Access | ✅ PASS | `ai-providers` crate defines the capability trait (chat, streaming, tools, embeddings); OpenAI/Anthropic/Gemini adapters; deterministic context assembler; tool registry is the only data path for the LLM (M3+) |
| V | API-First & Contract Consistency | ✅ PASS | `/api/v1` REST from M0; shared error envelope, cursor pagination, idempotency keys designed in contracts/ before implementation; dashboard consumes the same API |
| VI | Observability by Default | ✅ PASS | Request ID + tracing + structured JSON logs + metrics land in M0 before any feature code; AI execution timeline persisted from M3 |
| VII | Test-First & Regression Discipline | ✅ PASS | Testing strategy mandates unit/integration/API/E2E layers per milestone; every bug fix requires a regression test (PR-template enforced) |
| VIII | Database Integrity & Migration Discipline | ✅ PASS | SQLx migrations only, from M0; normalized schema in data-model.md; index requirements listed per entity; expand→migrate→contract rule; no manual schema changes |
| IX | Design System Discipline | ✅ PASS | Angular workspace builds `libs/ui` design tokens + component library in M0–M1 before feature pages; widget and dashboard share tokens |
| X | Performance & Efficiency | ✅ PASS | Streaming (WS/SSE) designed into contracts; N+1 avoidance via query review checklist; perf budgets tracked in milestone acceptance criteria |

**Initial gate result**: PASS — no violations; Complexity Tracking not required.
**Post-design re-check (after Phase 1)**: PASS — data-model.md keeps modules
normalized and tenant-scoped; contracts/ follow the API principles; no new
deviations introduced.

## Project Structure

### Documentation (this feature)

```text
specs/001-ai-customer-service-platform/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── rest-api.md
│   ├── realtime.md
│   ├── domain-events.md
│   └── ai-provider-interface.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── Cargo.toml                     # workspace root
├── crates/
│   ├── server/                    # binary crate: composition root, router assembly, startup
│   ├── shared/
│   │   ├── kernel/                # TenantScope, typed ids, error types, pagination, idempotency
│   │   ├── db/                    # SQLx pool, migration runner, tenant-scoped query helpers
│   │   ├── events/                # domain event bus (in-process) + transactional outbox
│   │   ├── observability/         # tracing setup, request-id, metrics, log redaction
│   │   └── config/                # typed config, secret loading
│   ├── modules/
│   │   ├── identity/              # authn, sessions, 2FA, API credentials, invitations
│   │   ├── tenancy/               # tenants, lifecycle, settings, tenant switcher
│   │   ├── rbac/                  # roles, permissions, authorization service
│   │   ├── users/                 # profiles, memberships, agent availability/skills
│   │   ├── customers/             # customer profiles, merge, attributes, GDPR delete
│   │   ├── conversations/         # conversations, messages, inbox, tags, CSAT
│   │   ├── escalations/           # queue, routing (skill→load fallback), assignment
│   │   ├── ai/                    # agent config, context assembly, executions, confidence
│   │   ├── prompts/               # prompt sections, versions, publish/rollback, sandbox
│   │   ├── knowledge/             # sources, collections, ingestion pipeline, retrieval
│   │   ├── tools/                 # tool registry, schema validation, invocation engine
│   │   ├── integrations/          # widget config, webhooks, delivery log
│   │   ├── notifications/         # in-app + email notifications, preferences
│   │   ├── analytics/             # metrics aggregation, topics, exports
│   │   ├── billing/               # plans, subscriptions, metering, invoices, dunning
│   │   ├── audit/                 # audit event sink (subscribes to domain events)
│   │   ├── flags/                 # feature flags + overrides
│   │   └── platform/              # platform users, health, incidents, provider config
│   └── ai-providers/              # provider trait + openai/anthropic/gemini adapters
├── migrations/                    # SQLx migrations (single ordered set)
└── tests/                         # cross-module API + isolation + contract tests

frontend/
├── angular.json                   # workspace
├── apps/
│   ├── dashboard/                 # tenant + platform dashboard (role-driven shell)
│   └── widget/                    # embeddable chat widget (strict bundle budget)
├── libs/
│   ├── ui/                        # design tokens, primitives, patterns (built FIRST)
│   ├── data-access/               # API clients, auth interceptors, generated types
│   ├── realtime/                  # WS/SSE client, reconnection, replay, presence
│   ├── feature-inbox/             # agent inbox feature
│   ├── feature-ai-config/         # prompts, sandbox, tools, knowledge UIs
│   ├── feature-analytics/         # dashboards, reports
│   ├── feature-admin/             # settings, users, billing UIs
│   └── feature-platform/          # platform features (tenants, health, flags)
└── e2e/                           # Playwright suites

infra/
├── docker-compose.yml             # local: postgres+pgvector, redis, minio, mailhog
├── Dockerfile.backend
├── Dockerfile.frontend
└── k8s/                           # production manifests (added M7)

.github/workflows/                 # CI/CD pipelines
```

**Structure Decision**: Web application layout with a Cargo workspace that
enforces module boundaries at the crate level — a module can only use another
module's public application-service API because cross-crate access to
internals won't compile. The Angular workspace splits the embeddable widget
(hard bundle-size budget) from the dashboard, with all shared visuals flowing
through `libs/ui` per Constitution Principle IX.

---

## Milestone Roadmap

Ordering rule: **foundations before features; no AI code until M0–M2 are
complete** (infrastructure, observability, auth, tenancy, RBAC, and the
conversation substrate AI attaches to). Every milestone ends with a
deployable, demonstrably usable system.

### M0 — Platform Foundations & Walking Skeleton

**Objectives**: Stand up the repository, toolchain, CI/CD, local infra, and a
deployed "walking skeleton": one Axum service with observability baked in, one
Angular shell, one migration, one versioned endpoint.

**Deliverables**:
- Cargo workspace + crate skeletons (`server`, `shared/*`, empty module crates)
- Angular workspace + `libs/ui` seeded with design tokens (color, spacing, type scale, density) and 5–8 primitives (button, input, table, dialog, toast)
- SQLx migration framework wired; migration 0001 (extensions: pgvector, citext; helper functions)
- Observability: request-ID middleware, `tracing` JSON logs with redaction layer, Prometheus `/metrics`, OpenTelemetry trace export, health/readiness endpoints
- API skeleton: `/api/v1` router, standard error envelope, cursor-pagination helpers, idempotency-key middleware scaffold
- docker-compose (Postgres+pgvector, Redis, MinIO, Mailhog); Dockerfiles; CI pipeline (fmt, clippy -D warnings, tests, audit, frontend lint/test/build, image build)
- Environment config + secret loading (no secrets in repo; `.env.example` only)

**Dependencies**: None (first milestone).

**Acceptance Criteria**:
- `docker compose up` + one documented command boots API + dashboard shell locally
- Every HTTP response carries a request ID; any request's trace is visible end-to-end in the local trace viewer
- CI is green and blocks on lint, tests, and migration validity
- Error envelope and pagination format match `contracts/rest-api.md` exactly

**Estimated Complexity**: Medium (breadth, not depth) — ~2 weeks.

**GitHub Milestone**: `M0 – Foundations`
**Epics**: `epic/repo-and-ci`, `epic/observability-baseline`, `epic/api-skeleton`, `epic/design-tokens`
**Feature Branches**: `feat/m0-cargo-workspace`, `feat/m0-ci-pipeline`, `feat/m0-observability`, `feat/m0-api-skeleton`, `feat/m0-angular-workspace`, `feat/m0-design-tokens`, `feat/m0-docker-compose`

---

### M1 — Identity, Tenancy, RBAC & Audit

**Objectives**: Complete the security core: signup/login/sessions/2FA, tenant
lifecycle, the full two-category RBAC model, tenant-isolation enforcement, the
Tenant Switcher, and audit logging. After M1 the platform is a secure
multi-tenant admin system with nothing to administer yet — which is correct.

**Deliverables**:
- `identity`: email/password + verification, sessions (idle/absolute timeout, revocation UI), password reset, TOTP 2FA (always-on for platform users, tenant-enforceable), account lockout (5 attempts / 15 min), scoped API credentials (hashed, last-used visibility)
- `tenancy`: tenant CRUD (self-serve + operator-provisioned), lifecycle states (trial→active→suspended→pending-deletion→deleted), profile/settings storage, per-tenant limit scaffold
- `rbac`: permission catalog, role→permission mapping for all 10 roles, `Authorize` service called from every application service, last-Owner protection, role-change propagation ≤1 min (permission cache with bust)
- Tenant Switcher: platform-user context assumption with scoped token, persistent UI banner, full audit trail
- `audit`: event schema per FR-AUDIT-002, subscriber on the domain-event bus, tenant + platform search APIs (read-only)
- `users`: invitations (7-day expiry, revocable), profiles, multi-tenant memberships + context switcher, deactivation
- Tenant-scoped data-access layer: `TenantScope`-required query helpers in `shared/db`; **tenant-isolation test suite added to CI as a permanent release gate**
- Dashboard: auth screens, tenant admin area (users, roles, invitations, audit log), platform area (tenant directory, platform users), role-filtered navigation

**Dependencies**: M0.

**Acceptance Criteria**:
- All 10 roles enforce their documented permission sets server-side (role × endpoint matrix tests)
- Isolation suite proves cross-tenant access is impossible via any shipped endpoint (including search and audit)
- Every sensitive action in FR-AUDIT-001 that exists so far produces a complete audit event (coverage test)
- Tenant Switcher entry/exit and all in-context actions appear in the platform audit log
- SC-012 verifiable for the M1 feature surface

**Estimated Complexity**: High — ~4 weeks; security-critical, sets patterns for everything after.

**GitHub Milestone**: `M1 – Identity & Tenancy`
**Epics**: `epic/authentication`, `epic/tenant-lifecycle`, `epic/rbac`, `epic/audit-log`, `epic/tenant-switcher`, `epic/isolation-gate`
**Feature Branches**: `feat/m1-auth-core`, `feat/m1-2fa`, `feat/m1-sessions`, `feat/m1-tenants`, `feat/m1-rbac`, `feat/m1-invitations`, `feat/m1-audit`, `feat/m1-tenant-switcher`, `feat/m1-isolation-tests`, `feat/m1-dashboard-admin`

---

### M2 — Conversations, Widget & Human-Only Chat

**Objectives**: Build the entire conversation substrate with **humans only** —
a fully working live-chat product (widget → inbox → agent replies) before any
AI. This validates realtime infrastructure, the conversation state machine,
and inbox UX independently of AI complexity.

**Deliverables**:
- `customers`: auto-created profiles, anonymous→identified merge, custom attributes, GDPR delete pipeline (30-day)
- `conversations`: state machine (open→active→waiting→resolved→closed), messages, internal notes, tags/dispositions, search/filter, auto-close policies (24 h/72 h defaults), CSAT (1–5 stars + optional comment)
- Realtime: WebSocket gateway (widget + inbox), Redis pub/sub fan-out, typing indicators, presence (agent availability), SSE fallback, reconnect + replay from last-acked cursor
- Widget app: embeddable script + iframe bundle (≈50 KB gz budget), tenant theming, widget session tokens, offline/business-hours states, a11y-audited
- Agent inbox: queues (mine/unassigned/all), conversation view, customer context panel, notes, tags, manual assignment
- `escalations` v0: shared queue + manual claim (skill routing arrives in M5); availability drives eligibility
- `notifications` v0: in-app notification center + real-time queue-entry alert; email sender (Mailhog locally)
- Widget config API + embed-snippet generation + install detection

**Dependencies**: M1 (auth, tenancy, RBAC on every new endpoint; audit events for new sensitive ops).

**Acceptance Criteria**:
- Customer on a test page and agent in the inbox exchange messages with ≤500 ms p95 relay (perf smoke in CI)
- All lifecycle transitions recorded and visible; auto-close per policy works
- Widget passes WCAG 2.1 AA checks and works inside a third-party page
- Isolation suite extended to conversations/customers passes
- Load smoke: 1k concurrent widget connections on staging without message loss

**Estimated Complexity**: High — ~5 weeks; realtime + widget engineering are the risk centers.

**GitHub Milestone**: `M2 – Conversations & Live Chat`
**Epics**: `epic/customer-profiles`, `epic/conversation-core`, `epic/realtime-gateway`, `epic/chat-widget`, `epic/agent-inbox`, `epic/notifications-v0`
**Feature Branches**: `feat/m2-customers`, `feat/m2-conversation-state`, `feat/m2-messages`, `feat/m2-ws-gateway`, `feat/m2-widget-app`, `feat/m2-inbox`, `feat/m2-csat`, `feat/m2-notifications`, `feat/m2-widget-config`

---

### M3 — AI Agent Foundation (Providers, Prompts, Streaming Replies)

**Objectives**: Introduce AI only now that auth, tenancy, observability, and
the conversation substrate exist. Deliver the provider abstraction with three
adapters + failover, versioned prompt management with sandbox, deterministic
context assembly, streaming AI replies into M2 conversations, full execution
timelines, confidence scoring, and rule-based escalation into the M2 queue.
(No RAG yet — the AI answers from prompt + conversation context only.)

**Deliverables**:
- `ai-providers` crate: capability trait (chat/stream/tools/embeddings), OpenAI/Anthropic/Gemini adapters, uniform token+cost accounting, error taxonomy, failover executor
- `platform` provider config: encrypted credentials (write-only, last-4 display), model catalog, routing policy (default + per-plan/tenant overrides)
- `prompts`: structured sections (persona/instructions/constraints/escalation), draft→publish→rollback with immutable versions and change notes, sandbox conversations isolated from live traffic, per-conversation version stamping
- `ai` module: deterministic context assembler (ordered inputs, snapshot recorded), streaming reply pipeline into the WS gateway, rolling summarization for long conversations, confidence scoring + threshold behaviors (answer/caveat/clarify/escalate), behavior constraints (blocked topics, disclaimers, business hours)
- Execution timeline: persisted per AI turn (assembly record, model calls, provider/model, tokens, latency, decision) + timeline viewer in conversation detail
- Escalation triggers v1: explicit request, low confidence, topic rules, repeated failure → M2 escalation queue
- AI usage metering events (consumed by billing in M6)

**Dependencies**: M2 (conversations, realtime, queue); M1 (RBAC on config surfaces; audit for publish/rollback/provider changes); M0 (tracing wraps provider calls).

**Acceptance Criteria**:
- First token ≤3 s p95 on staging against at least two live providers
- Same inputs ⇒ byte-identical assembled context (determinism test)
- Provider kill-switch test: mid-conversation failover completes without conversation failure (SC-009)
- Rollback restores prior behavior for new conversations ≤1 min (SC-006)
- 100% of AI turns have complete timelines (SC-007 coverage test)

**Estimated Complexity**: High — ~5 weeks; the provider abstraction and determinism guarantees are architecturally load-bearing.

**GitHub Milestone**: `M3 – AI Foundation`
**Epics**: `epic/provider-abstraction`, `epic/prompt-management`, `epic/context-assembly`, `epic/ai-streaming`, `epic/execution-timeline`, `epic/confidence-escalation`
**Feature Branches**: `feat/m3-provider-trait`, `feat/m3-openai-adapter`, `feat/m3-anthropic-adapter`, `feat/m3-gemini-adapter`, `feat/m3-failover`, `feat/m3-prompt-versions`, `feat/m3-sandbox`, `feat/m3-context-assembler`, `feat/m3-streaming-replies`, `feat/m3-timeline`, `feat/m3-confidence`

---

### M4 — Knowledge Base & RAG

**Objectives**: Ground the AI in tenant knowledge: ingestion pipeline,
embeddings in pgvector, hybrid retrieval, citations, collections/scoping,
retrieval testing tool, and quotas.

**Deliverables**:
- `knowledge`: sources (upload PDF/Word/text/MD/HTML, authored articles, URL fetch + bounded crawl), async pipeline (queued→processing→ready/failed with actionable reasons), segmentation, embedding generation via provider abstraction, atomic re-ingestion (old serves until new is ready), disable/delete with index cleanup
- Retrieval: hybrid semantic (pgvector) + keyword (Postgres FTS) with rank fusion, relevance thresholding, tenant + collection scoping enforced in the query layer
- RAG integrated into context assembly (ordered, recorded, cited); citations always visible to staff, customer-visible per tenant setting; honest fallback when retrieval is weak
- Retrieval test tool (FR-KB-006) UI + API; storage quotas + usage display
- S3 storage for source files; ingestion workers with progress events

**Dependencies**: M3 (embeddings via provider abstraction; context-assembler extension points); M0 (S3/MinIO infra).

**Acceptance Criteria**:
- US1 acceptance scenarios pass end-to-end (grounded, cited, streamed answers; honest fallback)
- Knowledge changes reflected in answers ≤5 min after "ready" (FR-KB-004 test)
- Retrieval ≤1 s p95 against a 10 GB/tenant synthetic corpus (NFR-PERF-004)
- Deleted source content provably absent from new answers
- Isolation suite extended to retrieval (cross-tenant embedding leakage test)

**Estimated Complexity**: High — ~4 weeks; ingestion robustness across formats is the long tail.

**GitHub Milestone**: `M4 – Knowledge & RAG`
**Epics**: `epic/ingestion-pipeline`, `epic/retrieval`, `epic/rag-integration`, `epic/knowledge-ui`
**Feature Branches**: `feat/m4-sources`, `feat/m4-ingestion-workers`, `feat/m4-embeddings`, `feat/m4-hybrid-retrieval`, `feat/m4-rag-context`, `feat/m4-citations`, `feat/m4-retrieval-tester`, `feat/m4-quotas`

---

### M5 — Escalation Maturity, Tools & Webhooks

**Objectives**: Complete the human-AI collaboration loop and outbound
extensibility: skill-based routing, AI-context handoff, return-to-AI, custom
tool registry with schema-validated execution, and signed webhooks.

**Deliverables**:
- `escalations` v1: skill-tag routing with load-based fallback (FR-USER-006), tenant toggle for manual-claim mode, offline behavior (expectation message / contact capture), reconnect-priority re-queue, escalation records with trigger provenance
- Handoff UX: AI summary + suggested knowledge in the agent panel, customer-facing continuity messaging, agent→AI return, AI-suggested replies (agent-approved)
- `tools`: registry (schema, endpoint, timeout, enablement, approval), input/output JSON-Schema validation, invocation engine with per-conversation rate limits, sandboxed HTTP egress, full invocation records in timelines; platform starter tools (business-hours lookup, escalate, collect-contact)
- `integrations`: outbound webhooks (HMAC-signed, retries with backoff, delivery log UI), event catalog per FR-INT-002
- Notification maturity: per-category/channel preference matrix, quiet hours, email templates

**Dependencies**: M3 (tool-calling in provider trait; timelines), M2 (queue, inbox), M4 (suggested knowledge in handoff panel).

**Acceptance Criteria**:
- US3 acceptance scenarios pass, including no-agent-available and reconnect edge cases; queue-entry alert ≤5 s
- Tool calls: schema-invalid input/output rejected and surfaced; timeline shows full invocation detail; failing tool degrades to explain-or-escalate
- Webhook deliveries retried per policy and fully logged; signatures verifiable
- Skill routing matches by tag with correct load-based fallback (routing simulation tests)

**Estimated Complexity**: Medium-High — ~4 weeks.

**GitHub Milestone**: `M5 – Handoff & Tools`
**Epics**: `epic/skill-routing`, `epic/handoff-ux`, `epic/tool-registry`, `epic/webhooks`, `epic/notification-maturity`
**Feature Branches**: `feat/m5-skill-routing`, `feat/m5-handoff-context`, `feat/m5-return-to-ai`, `feat/m5-tool-registry`, `feat/m5-tool-execution`, `feat/m5-webhooks`, `feat/m5-notification-prefs`

---

### M6 — Analytics & Billing

**Objectives**: Close the improvement loop (dashboards, topics, knowledge
gaps) and monetize (plans, metering, invoices, dunning).

**Deliverables**:
- `analytics`: aggregation pipeline (≤5 min freshness, labeled), tenant KPIs with period-over-period, segmentation (channel/tag/agent/prompt-version/customer attributes), topic clustering via embeddings, knowledge-gap surfacing (FR-KB-008/FR-ANLT-003), CSV export + export API, platform cross-tenant aggregates (no conversation content)
- `billing`: plan catalog (fees, quotas, entitlements, overage), subscriptions + trials, idempotent usage metering (AI interactions from M3 events, seats, storage), threshold notifications (80/100%), limit behaviors (soft-warn / hard-stop with graceful widget fallback), itemized invoices, external payment-processor integration (webhook-driven), dunning state machine (retry→notify→grace→suspend), proration on upgrade, downgrade validation
- Tenant suspension mode (read-only dashboard, widget offline message) and reactivation
- Owner billing UI; platform Finance UI (plans, invoices, exceptions, credits/refunds — audit-logged)

**Dependencies**: M3 (usage events), M2 (conversation metrics), M5 (escalation metrics), M1 (Finance role surfaces).

**Acceptance Criteria**:
- US7 and US8 acceptance scenarios pass
- Metering double-billing test: forced retries never duplicate a usage record (SC-010); usage lag ≤1 h
- Dashboards reflect staged fixture data correctly across all segments; freshness ≤5 min
- Dunning walkthrough: failed payment → retries → suspension → payment → reactivation, fully audit-logged

**Estimated Complexity**: High — ~5 weeks; billing correctness demands heavy test investment.

**GitHub Milestone**: `M6 – Analytics & Billing`
**Epics**: `epic/analytics-pipeline`, `epic/topic-clustering`, `epic/plan-catalog`, `epic/metering`, `epic/invoicing-dunning`
**Feature Branches**: `feat/m6-aggregations`, `feat/m6-dashboards`, `feat/m6-topics`, `feat/m6-exports`, `feat/m6-plans`, `feat/m6-metering`, `feat/m6-invoices`, `feat/m6-dunning`, `feat/m6-suspension`

---

### M7 — Platform Operations, Hardening & GA

**Objectives**: Everything required to run this as a commercial SaaS: feature
flags, health/incidents, SLO alerting, backups/DR rehearsal, i18n/a11y
completion, load testing to scale targets, security review, production
deployment topology.

**Deliverables**:
- `flags`: global/plan/tenant flags, ≤5 min propagation (Redis-backed cache with pub/sub bust), fail-safe defaults, history + audit
- `platform` health: operator dashboard (availability, error rates, latency percentiles, queue depths, provider status/cost), incident declare/update with tenant banners, configurable alert thresholds → on-call notification
- SLO burn-rate alerting; AI drift monitoring (confidence drift, escalation anomalies)
- Backups: PITR validated, quarterly-restore runbook + first rehearsal, per-tenant restore procedure; DR runbook (regional failure, corruption, provider dependency) + tabletop exercise
- i18n: externalized strings complete, second locale shipped, RTL verified in widget/inbox; a11y: WCAG 2.1 AA audit + fixes across dashboard and widget
- Load/perf: 10k concurrent conversations + 1M conv/month soak on staging; N+1 sweep; query/index tuning pass
- Security: dependency audit, penetration-test remediation, rate-limit tuning, prompt-injection red-team pass; SOC 2 evidence-collection scaffolding
- k8s production manifests, rolling deploy with WS drain (no interrupted conversations, NFR-AVAIL-002), CDN for widget

**Dependencies**: All prior milestones.

**Acceptance Criteria**:
- All 12 success criteria (SC-001…SC-012) verified and recorded
- DR rehearsal meets RPO ≤5 min / RTO ≤4 h on staging
- Flag flip visible in a running session ≤5 min without restart
- Load test sustains scale targets within availability budgets
- Zero criticals outstanding from security review

**Estimated Complexity**: Medium-High — ~4 weeks, highly parallelizable.

**GitHub Milestone**: `M7 – Operations & GA`
**Epics**: `epic/feature-flags`, `epic/system-health`, `epic/backup-dr`, `epic/i18n-a11y`, `epic/perf-hardening`, `epic/security-hardening`, `epic/prod-deploy`
**Feature Branches**: `feat/m7-flags`, `feat/m7-health-dashboard`, `feat/m7-incidents`, `feat/m7-slo-alerts`, `feat/m7-dr-runbooks`, `feat/m7-i18n`, `feat/m7-a11y`, `feat/m7-load-tests`, `feat/m7-k8s`

---

## Deployment Considerations

- **Topology**: single stateless backend deployment (modular monolith) scaled horizontally; WebSocket sessions rebalance via Redis pub/sub so any node serves any conversation; Postgres primary + read replica (analytics reads); Redis; S3; CDN for widget bundle and dashboard assets.
- **Environments**: local (docker-compose) → staging (k8s, production-shaped, synthetic load) → production. Staging receives every merge to `main`; production releases are tagged.
- **Zero-interruption deploys**: rolling deploys with connection draining plus widget/inbox client auto-reconnect and replay from last-acked cursor (`contracts/realtime.md`) satisfy NFR-AVAIL-002.
- **Config & secrets**: environment-injected; provider keys envelope-encrypted at rest; no environment reads secrets from the repo.
- **Migrations in deploy**: run as a pre-rollout job; only backward-compatible (expand→migrate→contract) changes so old and new versions coexist during rollout.

## CI/CD Additions (cumulative by milestone)

- **M0**: rustfmt, clippy (`-D warnings`), cargo test, cargo audit/deny, SQLx offline check + migration dry-run against a scratch DB, Angular lint/test/build, Docker image build, secret-leak scan.
- **M1**: tenant-isolation suite (release gate), RBAC role×endpoint matrix tests, audit-coverage test.
- **M2**: Playwright E2E (widget↔inbox), WS load smoke (1k connections), widget bundle-size budget gate, axe a11y checks.
- **M3**: provider-adapter contract tests against recorded fixtures + nightly live-provider smoke, determinism test, timeline-coverage test.
- **M4**: ingestion format-matrix tests, retrieval relevance regression set, retrieval isolation test.
- **M5**: tool schema-validation tests, webhook signature/retry tests, routing simulation tests.
- **M6**: metering idempotency/property tests, invoice snapshot tests, dunning state-machine tests.
- **M7**: nightly load test, SLO burn-rate alert dry-run, security scanning gates, scheduled DR-restore job.

## Testing Strategy

- **Unit** (per crate/lib): domain logic, state machines, assembly determinism, routing selection, metering math. Fast, no I/O.
- **Integration** (testcontainers Postgres/Redis/MinIO): repositories + migrations, event bus + outbox, ingestion pipeline, retrieval queries.
- **API/contract**: every endpoint tested against `contracts/rest-api.md` conventions (envelope, pagination, errors, idempotency) plus the role×tenant authorization matrix; generated API types keep the frontend in lockstep.
- **E2E (Playwright)**: the eight user stories as journey suites on staging-shaped compose; widget embedded in a fixture third-party page.
- **Permanent gates**: tenant isolation (M1+), audit coverage (M1+), determinism (M3+), timeline coverage (M3+), bundle budget (M2+).
- **Non-functional**: perf smoke per milestone against its NFR budget; full-scale load/soak in M7; chaos drills (provider kill, Redis loss, WS node kill) in M7.
- **Regression discipline**: every bug fix lands with a failing-then-passing test referencing the issue (PR template + review checklist), per Constitution Principle VII.
- **AI-specific**: recorded-fixture provider tests in CI; nightly live smoke against real providers; retrieval relevance regression corpus curated from sandbox sessions; prompt-injection red-team suite (M7).

## Migration Strategy

- **Tooling**: SQLx migrations only, single ordered directory, validated in CI against a scratch database built from migration 0001 (never from a dump). No manual schema changes in any environment (Constitution Principle VIII).
- **Compatibility rule**: expand→migrate→contract. Additive change ships first; backfills run as idempotent, batched, resumable jobs (not inside migrations for large tables); contraction ships ≥1 release later.
- **Tenancy invariants**: every new tenant-owned table must carry `tenant_id` + composite indexes leading with `tenant_id`; enforced by a migration lint script in CI plus PR checklist.
- **Data seeds**: idempotent seeds per environment (role/permission catalog, plan catalog, starter tools, staging demo tenant).
- **Rollback posture**: migrations forward-only in production; recovery is restore-based (PITR) per DR runbook; destructive migrations require an explicit approval label and pre-migration snapshot.

## Potential Risks

| # | Risk | Mitigation in this plan |
|---|------|------------------------|
| PR-01 | Realtime infrastructure (WS scale-out, reconnect/replay) harder than expected | Isolated in M2 as a human-only product, load-smoked at 1k connections before AI depends on it |
| PR-02 | Provider abstraction leaks vendor semantics (tool-call formats, streaming quirks) | Adapter contract tests with recorded fixtures for all three vendors from day one; abstraction reviewed against all three before M3 merges |
| PR-03 | Tenant-isolation regression as module count grows | Compile-time `TenantScope` requirement + permanent CI isolation gate extended every milestone |
| PR-04 | Ingestion quality across messy real-world documents delays M4 | Format matrix tests, visible failure reasons, retrieval tester for tenant self-diagnosis; bounded crawl |
| PR-05 | Billing correctness bugs erode trust | End-to-end idempotency keys, property-based metering tests, invoice snapshots, shadow-billing dry-run before enabling charges |
| PR-06 | Milestone scope creep (18 domains invite it) | Each milestone has one demoable definition of "usable"; anything outside its acceptance criteria moves down the roadmap |
| PR-07 | Prompt-injection / AI-safety incident at launch | Tool allow-listing + schema validation (M5), blocked-topic constraints (M3), red-team pass gating GA (M7) |
| PR-08 | Widget bundle growth breaks embed performance | CI bundle-budget gate from M2; widget kept dependency-minimal, sharing only `libs/ui` tokens |

## Potential Technical Debt (accepted knowingly, tracked)

- **In-process event bus + transactional outbox** instead of a message broker: correct for a modular monolith; the outbox table is the upgrade path if a module is extracted.
- **Postgres FTS for keyword search**: sufficient at v1 scale; dedicated search engine is a known post-GA upgrade if conversation search demands grow.
- **Analytics aggregation in Postgres** (rollup tables + read replica): fine to 1M conv/month; columnar/warehouse offload deferred.
- **Single-region deployment** at GA with restore-based DR: multi-region active-passive deferred; RPO/RTO met via PITR + rehearsed runbooks.
- **Manual-claim escalation only until M5**: skill routing intentionally one milestone behind the inbox.
- **Email as the only out-of-app notification channel** until post-GA (no SMS/push).
- **Sandbox shares the live retrieval index (read-only)** rather than a snapshot; acceptable because prompts are the variable under test.

## Complexity Tracking

> No Constitution Check violations — table intentionally empty.
