<!--
Sync Impact Report
==================
Version change: 1.1.0 → 1.2.0
Modified principles: N/A
Added sections: N/A
Expanded sections:
  - Technology Stack & Platform Requirements → Frontend now mandates
    RxJS-first asynchronous logic: Angular code MUST prefer RxJS observables
    and operator composition over Promise-based flows; Observable→Promise
    conversion is confined to inherently Promise-based integration boundaries.
Removed sections: N/A
Templates requiring updates:
  - .specify/templates/plan-template.md ✅ no change needed (Constitution Check
    gate is populated dynamically from this file; already generic)
  - .specify/templates/spec-template.md ✅ no constitution-specific references found
  - .specify/templates/tasks-template.md ✅ no constitution-specific references found
  - .specify/templates/checklist-template.md ✅ no constitution-specific references found
Follow-up TODOs: none.
-->

# AI Customer Service Platform Constitution

## Vision

Build an Enterprise Operating System for AI Customer Service: not a chatbot, but a
platform that lets businesses configure, monitor, analyze, debug, and continuously
improve AI agents across multiple communication channels. Every architectural
decision MUST optimize for maintainability and long-term SaaS scalability over
premature feature development.

## Core Principles

### I. Enterprise Modular Monolith
The initial implementation MUST be a Modular Monolith. Modules MUST be isolated
behind clear interfaces and communicate only through application services and
domain events — never through direct cross-module data access or tightly coupled
calls. Every module boundary MUST be drawn so that the module could be extracted
into an independent microservice later without a major rewrite.
**Rationale**: A monolith is faster to build and operate at this stage, but the
business requires a large multi-tenant SaaS platform long-term; strict internal
modularity is what makes that future extraction possible instead of requiring a
rewrite.

### II. Multi-Tenant Isolation, No Exceptions
The system distinguishes Platform Users (Super Admin, Developer, Sales, Support,
Finance — who may switch tenant context via a Tenant Switcher) from Tenant Users
(Owner, Admin, Manager, Agent, Viewer — who never access data outside their own
tenant). Every tenant-owned table MUST include a `tenant_id`, and every
tenant-aware query MUST enforce tenant isolation at the data-access layer.
Frontend-only authorization checks MUST NEVER be relied upon as the sole
enforcement mechanism for tenant or role boundaries.
**Rationale**: A single cross-tenant data leak is an existential trust failure
for an enterprise SaaS platform; isolation must be enforced where it cannot be
bypassed by a compromised or buggy client.

### III. Zero-Trust Security & RBAC
All endpoints MUST require authorization; there are no implicitly trusted
internal routes. Access control MUST use RBAC consistent with the Platform User
and Tenant User role sets. Secrets MUST NEVER appear in source code or version
control. Sensitive operations (role changes, tenant data export, billing,
credential rotation, AI configuration changes) MUST be audited with who/what/when
records.
**Rationale**: Zero-trust and audit trails are baseline expectations for
enterprise buyers and are far cheaper to build in from the start than to retrofit
after a security incident.

### IV. AI Provider Independence & Tool-Mediated Access
LLMs MUST NEVER access databases or internal services directly; they interact
exclusively through approved, explicitly defined tools. Prompt construction MUST
be deterministic (same inputs produce the same prompt structure). The AI
subsystem MUST support tool calling, RAG, conversation memory, human escalation,
and prompt versioning, and MUST remain provider-independent across OpenAI,
Anthropic, and Gemini today, such that adding a future provider requires minimal
implementation effort.
**Rationale**: Direct LLM-to-database access is both a security hole and a
correctness risk (non-deterministic queries); a tool-mediated, provider-abstracted
design keeps the platform auditable and lets the business swap or add model
vendors without rearchitecting.

### V. API-First & Contract Consistency
All functionality MUST be exposed through a REST-first, versioned API with
consistent naming, predictable pagination, and standardized error responses.
Write operations MUST be idempotent where the operation's semantics allow it.
Public interfaces are a first-class deliverable, not an afterthought to the UI.
**Rationale**: API-first design keeps the platform extensible (integrations,
marketplace, third-party channels) and prevents the frontend from becoming the
only consumer of business logic.

### VI. Observability by Default
Every request MUST propagate a request ID and support structured logging,
distributed tracing, and metrics. Critical AI operations (tool calls, RAG
retrieval, escalation decisions) MUST expose an inspectable execution timeline.
**Rationale**: AI-driven behavior is inherently harder to reason about than
deterministic code paths; without built-in observability, debugging agent
behavior in production becomes guesswork.

### VII. Test-First & Regression Discipline
Unit, integration, API, and end-to-end tests are all required categories of
coverage for shipped functionality. Every bug fix MUST introduce a regression
test that fails before the fix and passes after.
**Rationale**: Regression tests turn every past incident into a permanent
guardrail, which compounds in value as the platform grows in scope and
contributor count.

### VIII. Database Integrity & Migration Discipline
Schemas MUST be normalized by default; deviations require explicit justification.
All schema changes MUST go through migrations — schema changes MUST NEVER be
applied manually against an environment. Every tenant-owned table includes
`tenant_id` (see Principle II). Indexes are mandatory for production query paths;
N+1 query patterns MUST be avoided.
**Rationale**: Migrations are the only reliable way to keep schema state
reproducible and auditable across environments and tenants at SaaS scale.

### IX. Design System Discipline
Reusable design tokens MUST exist before components; reusable components MUST
exist before pages; reusable patterns MUST exist before features. UI logic MUST
NEVER be duplicated across the codebase. The product's visual and interaction
language MUST target the density, accessibility, performance, and consistency
bar set by enterprise tools such as Intercom, Stripe, HubSpot, Notion, and Linear.
**Rationale**: Building bottom-up (tokens → components → patterns → features)
is what keeps a growing UI codebase consistent instead of accumulating
one-off, divergent implementations.

### X. Performance & Efficiency
Implementations MUST optimize for low latency, efficient queries, and minimal
allocations, and MUST use streaming where the interaction pattern benefits from
it (e.g., AI response generation). N+1 queries are treated as defects, not
style issues.
**Rationale**: Customer-service interactions are latency-sensitive by nature;
performance is a correctness property of this product, not a later optimization
pass.

## Technology Stack & Platform Requirements

**Frontend**: Angular, TypeScript, Signals, RxJS, Angular Material or the
project's own component library. Components, directives, and pipes MUST be
standalone by default — NgModules MUST NOT be introduced for new code. The
Angular workspace configuration (`angular.json` `schematics` defaults) MUST
keep `standalone: true` as the generated default so this is enforced by
tooling, not convention alone.

Asynchronous and event-driven logic in Angular code MUST prefer RxJS
observables and operator composition (`pipe`, `map`, `switchMap`,
`catchError`, …) over Promise-based flows. Converting an Observable to a
Promise (`firstValueFrom`/`lastValueFrom`, `async/await` wrappers, `.then()`
chains) is permitted only at integration boundaries where the consumed or
exposed API is inherently Promise-based (e.g., application initializers,
imperative `Router.navigate` calls); such conversions MUST stay localized to
that boundary and MUST NOT replace operator composition inside services,
interceptors, guards, effects, or component streams.

**Backend**: Rust, Axum, Tokio, SQLx, PostgreSQL, Redis, pgvector, Serde, Tracing.

**Storage**: S3-compatible object storage.

**AI**: A provider abstraction layer supporting OpenAI, Anthropic, and Gemini
today. Adding a future provider MUST require implementing only the abstraction's
defined interface, not changes to calling code.

Deviating from this stack for a given module requires explicit justification
recorded in that feature's plan (see Complexity Tracking in the plan template).

## Documentation & Future Readiness

Every module MUST document its Purpose, Responsibilities, Public Interfaces,
Dependencies, Data Model, and Extension Points.

The architecture MUST be designed so the following are addable as extensions
rather than rewrites: Voice AI, Email, Facebook Messenger, Instagram, Slack,
Microsoft Teams, CRM integrations, ERP integrations, a partner/app Marketplace,
workflow automation, and custom AI providers. New channels and integrations are
expected to plug into the existing module and tool-abstraction boundaries defined
by Principles I and IV.

## Governance

This constitution supersedes all other engineering practices, style guides, and
prior conventions where they conflict. All specifications, implementation plans,
and PRs/reviews MUST verify compliance with these principles before merge.

**Amendment procedure**: Amendments are proposed by editing this file, must state
the rationale for the change, and must update the Sync Impact Report at the top
of this file along with the version and date fields below. Any dependent
templates (plan, spec, tasks, checklist) or agent-facing guidance found to be
inconsistent with the amendment MUST be updated in the same change.

**Versioning policy** (semantic versioning applied to governance):
- **MAJOR**: Backward-incompatible principle removals or redefinitions.
- **MINOR**: New principle or section added, or existing guidance materially
  expanded.
- **PATCH**: Clarifications, wording, typo fixes, and non-semantic refinements.

**Compliance review**: Any deviation from a Core Principle in a plan MUST be
recorded and justified in that plan's Complexity Tracking section; unjustified
deviations are grounds to block the plan at the Constitution Check gate.

When making decisions not explicitly covered above, prioritize simplicity,
maintainability, and long-term scalability over short-term convenience.

**Version**: 1.2.0 | **Ratified**: 2026-07-03 | **Last Amended**: 2026-07-11
