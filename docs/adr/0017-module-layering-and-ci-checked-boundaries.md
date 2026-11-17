# ADR-017: Layering for new and changed modules, and CI-checked module boundaries

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** Platform engineering
- **Related:** [ADR-001](0001-modular-monolith-hexagonal-modules.md) (this ADR amends its section 3
  and 4 timing and closes its follow-up M5), constitution Principle IV (amended in the same PR,
  version 2.1.0), [System architecture §3–4](../architecture/system-architecture.md#3-bounded-contexts-and-module-map),
  [Module dependencies](../architecture/module-dependencies.md) (generated), MOH-79

## Context

ADR-001 decided that every module is hexagonal (`domain`, `application`, `infrastructure`,
`api`), owns its tables and depends only downward, and listed the gaps between that decision and
the code as follow-ups (M1 to M5). MOH-79 measured the gaps with `cargo metadata`, the source tree
and the migrations. The generated result is [Module dependencies](../architecture/module-dependencies.md).

- **No module has layers.** All 23 module crates (four of them one-line placeholders, MOH-80) are
  flat: `model.rs`, `queries.rs`, `routes.rs`. SQL and HTTP types sit next to business rules.
- **The crate graph is acyclic** (Cargo refuses a cycle in normal dependencies) but wide: `ai`
  depends on nine other modules, `tenancy` is used by fourteen, `identity` by thirteen.
- **One edge runs against ADR-001 section 2.3:** `ai -> notifications`, a producer calling an event
  consumer.
- **Table ownership is broken in 120 places.** ADR-001 estimated 30 references to the
  `conversations` table (23 remain, across `conversations`, `messages` and `message_attachments`).
  The wider picture: 14 modules and the server read or write other modules' tables, mostly lookups
  of `users` (identity), `tenant_memberships` and `tenants` (tenancy) in joins (59 of the 120),
  then `conversations`, `messages` and `message_attachments` (reached by `whatsapp`, `widgets`,
  `escalations`), and `audit_logs` (written directly by `tenancy`, `tools` and the server).
- **`analytics`** reads `conversations`, `messages`, `escalations`, `ai_generations`,
  `ai_usage_records` and `conversation_feedback` directly (the issue listed four). It writes
  nothing and owns no table.
- **Nothing checks any of it.** ADR-001 section 4 promised a dependency-edge and a table-ownership
  check in CI; neither existed, and the rules drifted.

The forces: rewriting 23 crates with no product change is expensive and risky while the specs are
still moving, but leaving the rules unchecked is how the gap grew, and layered code is what makes
business rules testable without a database.

## Decision

We keep ADR-001's target and change when it applies and how it is held:

1. **New modules are layered from the first commit.** A module crate that is not on the flat list
   has `domain/`, `application/`, `infrastructure/` and `api/` under `src/`. CI fails otherwise.
2. **A changed module moves the changed code into layers.** A PR that adds or rewrites a use case
   in a flat module creates the layers in that module and puts the new business rules in
   `domain/` and `application/`, the SQL in `infrastructure/` and the routes in `api/`. It does not
   move untouched code. A module that has all four layers leaves the flat list in the same PR (CI
   fails if it stays on it), so the list only shrinks. Files still at the root of `src/` are not
   checked until they move. "Changed" cannot be detected mechanically; the PR checklist and
   review enforce it.
3. **Inside the layers the rules are checked.** `domain/` names no framework or vendor SDK
   (`sqlx`, `axum`, `redis`, `reqwest`, `hyper`, `tower`, `lettre`, `aws_sdk_s3`, `actix_*`) and
   imports no outer layer; `application/` imports neither `infrastructure/` nor `api/`, nor
   `axum`; `infrastructure/` does not import `api/`. Imports are resolved as Rust resolves them
   (`crate::`, `self::`, `super::` and grouped `use` lists).
4. **Module edges are an allow-list.** Every module to module dependency is listed in
   `backend/tools/module-boundaries.json`. A new edge fails CI until someone adds it and explains
   it in the PR; an edge that disappears must be removed from the list. Shared and adapter crates
   never depend on a module or on `server`; modules never depend on `server`; no module depends on
   an event consumer (`analytics`, `audit`, `notifications`) except through a named exception.
   The crate graph (normal, build and dev edges) has no cycle.
5. **Table ownership stays the rule and is enforced as a ratchet.** Every table created by a
   migration has one owner in the same file; `outbox_events` and `secret_keks` are shared
   infrastructure. SQL that names another module's table is counted per crate and table. The count
   may not grow, and must be lowered when a refactor shrinks it, so progress is kept. The
   follow-ups below retire the largest groups, and when the count reaches zero the ratchet
   becomes the plain rule.
6. **Accepted exception: `analytics` is a read-only read model.** It may read the six tables
   listed under `readModels.analytics` and may write nothing, shared tables included. The reason is
   cost: its aggregates run over the operational tables at query time, and an event-built copy
   would duplicate those tables before any query is slow. CI fails if it writes, or reads a table
   that is not on the list. Revisit when an analytics query is shown to hurt the operational
   workload (a measured p95 on the conversation paths, or a lock or load incident), or when an
   owner's schema change breaks analytics twice; then that table moves behind an event-built
   read model.
7. **Accepted exception: `ai -> notifications`.** `ai` calls `notifications::emit` directly for
   tool-approval and failed-response notifications. It is recorded with its reason in the config
   and is retired by MOH-195.
8. **The module map is generated.** `docs/architecture/module-dependencies.md` is produced from
   `cargo metadata` and the config by `node backend/tools/check-module-boundaries.mjs --write-doc`;
   CI fails when it is stale. The check runs in the Backend workflow.

Principle IV of the constitution is amended to match (version 2.1.0).

## Alternatives considered

| Option | Why not |
|---|---|
| **Accept flat modules and amend Principle IV** down to the rules worth keeping | Cheapest, and the boundary checks above would hold either way. Rejected because it gives up what ADR-001 was written for: business rules testable without a database, and vendors replaceable without touching them. |
| **Refactor every module now** | A rewrite of 23 crates with no product change, in a codebase whose specs are still moving. The incremental rule reaches the same end state at the pace of the changes that need it. |
| **Keep the rules and enforce nothing** | The state that produced the gap. |
| **Enforce table ownership only after the count is zero** (ADR-001 M2) | Nothing would stop the count growing for the months it takes to get there. The ratchet freezes it today. |

## Consequences

**Positive**

- Cross-module SQL stops growing the day this lands, and every refactor that moves a query behind
  an owner's interface lowers a number that CI keeps.
- New edges between modules are decisions with a reason in a PR, not accidents.
- The module map cannot drift from the code unnoticed.
- New modules get testable business rules from the start.

**Negative / costs**

- New work costs more boilerplate (ports, adapters, DTO mapping) than a flat file, and flat and
  layered code coexist for a long time. The flat list and the SQL ratchet make that debt visible.
- The SQL check is a text scan, not a SQL parser: it finds the statement forms Rust code uses
  here (`FROM`, `JOIN`, `INSERT INTO`, `UPDATE`, `DELETE FROM`, `TRUNCATE`, with quoted and
  schema-qualified names) and counts them. Dynamic SQL built from fragments is invisible to it.
- "Changed module" is enforced by review, not by CI.

**Follow-ups**

- MOH-192: layer the `conversations` module (the most changed, and the most reached into).
- MOH-193: `identity` and `tenancy` directory facades; retire the cross-module reads of `users`,
  `tenant_memberships` and `tenants`.
- MOH-194: route `whatsapp`, `widgets` and `escalations` access to `conversations`, `messages`,
  `message_attachments` and `customers` through the owners' interfaces.
- MOH-195: `ai` depends on ports it owns (ADR-001 M3) and `notifications` consumes AI events
  (removes the `ai -> notifications` exception).
- MOH-196: write audit records through the `audit` module's public interface.
- MOH-80 removes the four placeholder crates; each removal also edits the config.
