# ADR-001: Modular monolith with hexagonal modules, and when to extract a service

- **Status:** Accepted (MOH-15)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §2–4 and §21](../architecture/system-architecture.md#2-architectural-style),
  constitution Principle IV, ADR-002 (backend stack), ADR-007 (async processing), ADR-010
  (gateway deployment unit), ADR-015 (monorepo layout)

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
| M1 | Modules are **flat** (`model.rs`, `queries.rs`, `routes.rs`), with SQL, DTOs and business rules mixed. No module has `domain/`, `application/` or `infrastructure/` layers or port traits. | `backend/crates/modules/*/src` | Introduce the layers module by module, starting with `conversations` and `escalations`. Each refactor is its own spec with tests. |
| M2 | **Cross-module table access.** About 30 SQL references outside the `conversations` crate (a few in tests) read, join or update the `conversations` table directly, in `ai`, `analytics`, `billing`, `customers`, `escalations`, `feedback`, `notifications`, `platform`, `whatsapp` and `widgets`. | `backend/crates/modules/*` | Route writes through the `conversations` facade. Replace reads with facade queries or event-built read models (`analytics` first). Add the table-ownership CI check once the count reaches zero. |
| M3 | **Wide module coupling.** For example `ai` depends directly on nine other module crates (`authz`, `billing`, `identity`, `conversations`, `escalations`, `knowledge`, `notifications`, `tenancy`, `tools`). | `backend/crates/modules/ai/Cargo.toml` | Depend on ports owned by `ai` and implemented in the composition root, per §3 ("`ai_runtime` depends on interfaces"). |
| M4 | ~~**One binary, one process.**~~ **Resolved (MOH-37) for run modes.** The `server` binary has four run modes selected with `APP_RUN_MODE` (`api`, `gateway`, `worker`, `all`) over one composition root (`server::router`). `worker` runs every outbox worker and sweeper (`spawn_workers` in `main.rs`) and serves only `/health`, `/ready` and `/metrics`; `api` and `gateway` start none; `all` is the default for local development and the e2e job. An unknown value stops the process. | `backend/crates/server/src/run_mode.rs`, `backend/crates/server/src/main.rs`, `backend/crates/server/src/router.rs`, `backend/crates/server/tests/run_modes.rs` | Done for the code. **Not yet safe to deploy split:** realtime fan-out and presence are in-process (ADR-009 RT1), and a `scheduler` mode is not split out (the sweepers run in `worker`). |
| M5 | No CI check for forbidden dependency edges. | `.github/workflows/backend.yml` | Add the check (e.g. a `cargo metadata` script or `cargo-deny` bans) after M1 for each module. |
