# ADR-001: Modular monolith with hexagonal modules, and when to extract a service

- **Status:** Accepted (MOH-15); amended by [Amendment 1](#amendment-1-layering-for-new-and-changed-modules-moh-79) (MOH-79)
- **Date:** 2026-11-07 (amended 2026-11-17)
- **Deciders:** Platform engineering
- **Related:** [System architecture §2–4 and §21](../architecture/system-architecture.md#2-architectural-style),
  constitution Principle IV, ADR-002 (backend stack), ADR-007 (async processing), ADR-010
  (gateway deployment unit), ADR-015 (monorepo layout), [Module dependencies](../architecture/module-dependencies.md) (generated)

## Context

The platform is a multi-tenant SaaS with about twenty bounded contexts: identity, tenancy,
conversations, routing, knowledge, AI runtime, tools, channels, billing, audit and others
(architecture doc §3). One small team builds and runs it.

The decision has to balance these forces:

- **Operational cost.** Every separately deployed service adds a pipeline, dashboards, alerts,
  network failure modes and versioned contracts. A small team pays that cost on every change.
- **Consistency.** Many operations span contexts in one business step: a message arrives, the AI
  replies, the conversation escalates, an agent is assigned. One database transaction is far
  simpler than distributed consistency.
- **Boundaries rot.** A monolith without enforced seams turns into a big ball of mud, which makes
  later extraction impossible and slows every change.
- **Testability and vendor churn.** AI providers, channels and storage vendors change often.
  Business rules must be testable without Axum, SQLx, Redis or a vendor SDK.
- **Uneven load.** Public widget/webhook traffic, AI turns and document ingestion scale very
  differently from the dashboard API, so some separation of runtime is needed even in one
  codebase.

## Decision

### 1. One deployable codebase, modular inside

The backend is **one Rust Cargo workspace** producing one artifact. Bounded contexts are
**module crates**. Runtime separation happens through **deployment units** (`api`, `gateway`,
`worker`, `scheduler`) built from the same codebase, not through separate services
(§2.1; the `gateway` split is ADR-010).

### 2. Module boundary rules

1. **Each module owns its tables.** No module reads or writes another module's tables, including
   through raw SQL joins. Cross-module data comes through the owner's public facade or from a read
   model the consumer builds from the owner's events.
2. **Public surface = facade + event types.** A module's `lib.rs` exports its application facade
   and its domain event types only. Everything else is `pub(crate)`.
3. **Dependencies point down** the module map (§3) and the crate graph must stay acyclic.
   `analytics`, `notifications` and `audit` consume events and are never called synchronously by
   producers.
4. **Asynchronous by default across modules.** A module reacts to another module's domain events
   (delivered through the transactional outbox, ADR-007) rather than calling it, unless it needs
   an answer to continue.

### 3. Hexagonal layering inside each module

```text
api/             → Axum routes, DTOs, extractors, OpenAPI annotations
application/     → commands, queries, ports (traits), use-case services
domain/          → aggregates, value objects, events, invariants (pure Rust)
infrastructure/  → SQLx repositories and adapters implementing the ports
```

- Dependencies point inward: `api → application → domain`. `infrastructure` implements ports
  declared by `application`/`domain`.
- `domain` must not depend on Axum, SQLx, Redis, Tokio I/O, or any vendor SDK.
- Small modules may start with fewer files, but the dependency direction is the same from day one.

### 4. Enforcement

- **Compile time:** crate visibility (`pub(crate)`) and the acyclic crate graph.
- **CI:** a dependency-edge check rejects forbidden edges, such as a `domain` module importing
  `sqlx` or `axum`, or a module depending on a crate higher up the map. `cargo-deny` covers
  licences and duplicate dependencies.
- **CI:** a table-ownership check flags SQL in one module's crate that names another module's
  tables.
- **Review:** the PR checklist asks for module boundaries (constitution Principle IV).

### 5. Extraction criteria

A module is extracted into its own service only when **an ADR shows a measured need** against at
least one of these triggers, and a cheaper fix (a separate worker pool, a deployment unit, an
index, a cache) has been tried or ruled out:

| Trigger | Measured signal (example thresholds) |
| -- | -- |
| **Load** | The module's work dominates a unit's CPU or memory (> 50% sustained) and can't be split off as a separate deployment unit of the monolith. |
| **Isolation** | A security or compliance boundary requires a separate process, network zone or credentials (e.g. untrusted code execution, a regulated data store). |
| **Fault isolation** | Incidents in the module repeatedly take down unrelated functionality, and bulkheads inside the process haven't stopped it. |
| **Deployment cadence** | The module needs to ship on a schedule the rest can't follow, as shown by blocked releases over a quarter. |
| **Team ownership** | A separate team owns the module end to end and coordination cost is measured (release conflicts, review wait). |

Extraction uses the **Strangler Fig** pattern: the facade becomes a client, the module's tables
move with it, and events cross the boundary through the outbox. Likely first candidates are
`channels`/`gateway`, `ai_runtime` and `knowledge` ingestion (§21).

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Microservices from day one** | Multiplies deployment, observability and contract-versioning cost for one small team. Cross-context workflows (message → AI → escalation → assignment) need sagas instead of a transaction. No measured load requires it. |
| **Classic layered monolith** (global `controllers/services/repositories`) | Organises by technical layer instead of business context, so features cut across the whole codebase and nothing stops one feature from reaching into another's data. Extraction later is close to a rewrite. |
| **Modular monolith without hexagonal layers** (flat modules: `model`, `queries`, `routes`) | Keeps context boundaries but lets SQL and HTTP types into business rules. Domain logic can only be tested through a database, and swapping vendors touches business code. This is roughly the current state (see below). |
| **Serverless functions per use case** | Cold starts hurt streaming and SSE, long-lived connections don't fit, local development and tenant-context propagation get harder, and vendor lock-in is high. |
| **Separate deployable per module, one repository** ("distributed monolith") | Pays the network and operations cost of services while keeping shared-database coupling. The worst of both. |

## Consequences

**Positive**

- One deploy, one database, local transactions across contexts. Simple operations for a small team.
- Business rules are plain Rust, unit-testable without containers, and independent of vendors.
- Boundaries are enforced by the compiler and CI rather than by discipline alone.
- Extraction stays possible because modules already talk through facades and events.

**Negative and costs**

- More boilerplate: ports, adapters and DTO mapping for every module, even small ones.
- Module data rules forbid convenient cross-module SQL joins. Read-heavy screens need read models
  or facade calls, which cost extra code.
- A single binary means a bad deploy affects every module. Mitigated by deployment units, feature
  flags and canary releases, not eliminated.
- Build times grow with the workspace and need watching (incremental builds, `sccache` in CI).

**Follow-ups**

- Add the dependency-edge and table-ownership CI checks.
- Refactor modules to the hexagonal layout incrementally, starting with the most-changed ones.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| M1 | Modules are **flat** (`model.rs`, `queries.rs`, `routes.rs`), with SQL, DTOs and business rules mixed. No module has `domain/`, `application/` or `infrastructure/` layers or port traits. | `backend/crates/modules/*/src` | Amendment 1: new modules are layered from the first commit and every changed module moves the changed code into layers; `conversations` is layered first (MOH-192). The check below lists the flat modules, and the list may only shrink. |
| M2 | **Cross-module table access.** About 30 SQL references outside the `conversations` crate (a few in tests) read, join or update the `conversations` table directly, in `ai`, `analytics`, `billing`, `customers`, `escalations`, `feedback`, `notifications`, `platform`, `whatsapp` and `widgets`. | `backend/crates/modules/*` | Amendment 1: the table-ownership check exists now as a ratchet (137 SQL references to another module's table across the crates, listed in the generated module-dependencies document; the count may only fall). `analytics` is an accepted read-only exception. `identity` and `tenancy` directory facades retire the largest group (MOH-193), then `whatsapp`'s writes to `conversations` (MOH-194). |
| M3 | **Wide module coupling.** For example `ai` depends directly on nine other module crates (`authz`, `billing`, `identity`, `conversations`, `escalations`, `knowledge`, `notifications`, `tenancy`, `tools`). | `backend/crates/modules/ai/Cargo.toml` | Depend on ports owned by `ai` and implemented in the composition root, per §3 ("`ai_runtime` depends on interfaces"). `ai -> notifications` is a named exception until then (MOH-195). |
| M4 | ~~**One binary, one process.**~~ **Resolved (MOH-37) for run modes.** The `server` binary has four run modes selected with `APP_RUN_MODE` (`api`, `gateway`, `worker`, `all`) over one composition root (`server::router`). `worker` runs every outbox worker and sweeper (`spawn_workers` in `main.rs`) and serves only `/health`, `/ready` and `/metrics`; `api` and `gateway` start none; `all` is the default for local development and the e2e job. An unknown value stops the process. | `backend/crates/server/src/run_mode.rs`, `backend/crates/server/src/main.rs`, `backend/crates/server/src/router.rs`, `backend/crates/server/tests/run_modes.rs` | Done for the code. **Not yet safe to deploy split:** realtime fan-out and presence are in-process (ADR-009 RT1), and a `scheduler` mode is not split out (the sweepers run in `worker`). |
| M5 | ~~No CI check for forbidden dependency edges.~~ **Resolved (MOH-79):** `backend/tools/check-module-boundaries.mjs` runs in the Backend workflow (module edges, cycles, layering and import direction, table ownership, generated document). `cargo-deny` for licences and duplicates is separate (MOH-84). | `backend/tools/check-module-boundaries.mjs`, `backend/tools/module-boundaries.json`, `.github/workflows/backend.yml` | Done. |

## Amendment 1: layering for new and changed modules (MOH-79)

**Date:** 2026-11-17. **Decision:** adopt the hexagonal layering of section 3 for **new and changed
modules**, with an incremental plan, rather than amend Principle IV down to what the flat modules
do today. The four layers stay the target of every module; the rules below say when a module has to
get there and how CI holds the line meanwhile.

### Context

The first audit after this ADR (MOH-79) measured the code against it with `cargo metadata`, the
source tree and the migrations. The generated result is
[Module dependencies](../architecture/module-dependencies.md). In short:

- **No module has layers.** All 23 module crates (four of them one-line placeholders, MOH-80)
  are flat: `model.rs`, `queries.rs`, `routes.rs`. SQL and HTTP types sit next to business rules.
- **The crate graph is acyclic** (Cargo refuses a cycle in normal dependencies) but wide: `ai`
  depends on nine other modules, `tenancy` is used by fourteen, `identity` by thirteen.
- **One edge runs against section 2.3:** `ai -> notifications` (a producer calling an event
  consumer).
- **Table ownership is broken in 137 places.** ADR-001 estimated 30 references to the
  `conversations` table (29 remain, across `conversations`, `messages` and `message_attachments`).
  The real picture is wider: 14 modules and the server read or write other modules' tables,
  mostly lookups of `users` (identity), `tenant_memberships` and `tenants` (tenancy) in joins (62
  of the 137), then `conversations`, `messages` and `message_attachments` (reached by
  `whatsapp`, `widgets`, `escalations`), and `audit_logs` (written directly by `tenancy`, `tools`
  and the server).
- **`analytics`** reads `conversations`, `messages`, `escalations`, `ai_generations`,
  `ai_usage_records` and `conversation_feedback` directly (the issue listed four). It writes
  nothing and owns no table.

### Decision

1. **New modules are layered from the first commit.** A module crate that is not on the flat
   list has `domain/`, `application/`, `infrastructure/` and `api/` under `src/`. CI fails
   otherwise.
2. **A changed module moves the changed code into layers.** A PR that adds or rewrites a use case
   in a flat module creates the layers in that module and puts the new business rules in `domain/`
   and `application/`, the SQL in `infrastructure/` and the routes in `api/`. It does not move
   untouched code. A module with all four layers leaves the flat list in the same PR (CI fails if
   it stays on it), and the list may only shrink. Files still at the root of `src/` are not
   checked until they move. Review enforces "changed"; there is no way to detect it mechanically.
3. **Inside the layers the rules are checked:** `domain/` names no framework or vendor SDK
   (`sqlx`, `axum`, `redis`, `reqwest`, `hyper`, `tower`, `lettre`, `aws_sdk_s3`, `actix_*`) and
   imports no outer layer; `application/` imports neither `infrastructure/` nor `api/`, nor
   `axum`; `infrastructure/` does not import `api/`.
4. **Module edges are an allow-list.** Every module -> module dependency is listed in
   `backend/tools/module-boundaries.json`. A new edge fails CI until someone adds it and explains
   it; an edge that disappears must be removed from the list. Shared and adapter crates never
   depend on a module or on `server`; modules never depend on `server`; no module depends on an
   event consumer (`analytics`, `audit`, `notifications`) except through a named exception.
   The crate graph (normal, build and dev edges) has no cycle.
5. **Table ownership stays the rule, enforced as a ratchet.** Every table created by a migration
   has one owner in the same file (`outbox_events` and `secret_keks` are shared infrastructure).
   SQL that names another module's table is counted per crate and table; the count may not grow,
   and must be lowered when a refactor shrinks it, so progress is kept. The follow-ups below retire
   the largest groups. When the count reaches zero the ratchet becomes the plain rule.
6. **Accepted exception: `analytics` is a read-only read model.** It may read the six tables
   listed in `readModels.analytics` and write nothing. The reason is cost: its aggregates run over
   the operational tables at query time, and an event-built copy would duplicate those tables
   before any query is slow. CI fails if `analytics` writes any table or reads one that is not on
   the list. Revisit when an analytics query is shown to hurt the operational workload (a measured
   p95 on the conversation paths, or a lock or load incident), or when an owner's schema change
   breaks analytics twice; then move that table behind an event-built read model.
7. **Accepted exception: `ai -> notifications`.** `ai` calls `notifications::emit` directly for
   tool-approval and failed-response notifications. It is recorded in the config with its reason
   and is retired by MOH-195 (notifications consume the AI turn events).

### Alternatives considered

| Option | Why not |
| -- | -- |
| **Accept flat modules and amend Principle IV** to the rules worth keeping | Cheapest, and the boundary checks above would hold either way. Rejected because it gives up the property the ADR was written for: business rules testable without a database, and vendors replaceable without touching them. |
| **Refactor every module now** | A rewrite of 22 crates with no product change, in a codebase whose specs are still moving. The incremental rule gets the same end state at the pace of the changes that need it. |
| **Keep the rules and enforce nothing** | The state that produced the gap: the rules drifted for lack of a check. |

### Consequences

- New work costs more boilerplate (ports, adapters, DTO mapping) than a flat file, and flat and
  layered code coexist for a long time. The flat list and the SQL ratchet make the remaining debt
  visible and shrinking.
- Cross-module SQL stops growing the day this lands. Moving a query behind a facade is a
  mechanical change with a number that goes down.
- `docs/architecture/module-dependencies.md` is generated from the same data, so the module map
  cannot drift from the code unnoticed.
- Layering "changed" modules is a review rule. The PR checklist carries it.

### Follow-ups (Linear)

- MOH-192: layer the `conversations` module (the most changed and most reached into).
- MOH-193: `identity` and `tenancy` directory facades; retire the cross-module reads of `users`,
  `tenant_memberships` and `tenants`.
- MOH-194: route `whatsapp`, `widgets` and `escalations` access to `conversations`, `messages`,
  `message_attachments` and `customers` through the owners' facades.
- MOH-195: `ai` depends on ports it owns (M3) and `notifications` consume AI events (removes the
  `ai -> notifications` exception).
- MOH-196: write audit records through the `audit` module's public interface (`tenancy`, `tools`
  and the server write `audit_logs` directly).
