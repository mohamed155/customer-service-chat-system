# ADR-019: Audit log shared crate for writing audit records (MOH-196)

- **Status:** Accepted
- **Date:** 2026-11-21
- **Deciders:** Platform engineering
- **Related:** [ADR-001](0001-modular-monolith-hexagonal-modules.md), [ADR-017](0017-module-layering-and-ci-checked-boundaries.md) (supersedes its MOH-196 follow-up), constitution Principle III, [System architecture §5](../architecture/system-architecture.md#5-audit-log), MOH-196

## Context

ADR-017 follow-up MOH-196 originally stated: "write audit records through the `audit` module's public interface." However, this creates a dependency cycle:

- Rule 4 of ADR-017 prohibits modules from depending on event consumers (`audit`, `analytics`, `notifications`).
- The `audit` module depends on `tenancy` (it reads `tenants`, `tenant_memberships` for its queries).
- The `tenancy` module itself writes audit records (member invitations, tenant signup, access denied).
- A call from `tenancy` to `audit` would create a cycle: `tenancy → audit → tenancy`.

Additionally, the `audit` module is an event consumer (it reads `audit_logs` for the audit API), and ADR-001 section 2.3 and ADR-017 rule 4 forbid producers from calling event consumers directly.

The forces:

- Every sensitive action must write an audit record (Constitution Principle III).
- The record must commit with the action: if the audit write fails, the action must not happen (transactional write).
- Some actions (tool execution, rate-limit refusals) have already happened and just need a best-effort record.
- The write must be tenant-scoped (RLS-enforced) or a platform event (restricted action list).
- No module should depend on the `audit` module.

## Decision

We introduce a new shared crate `audit-log` (`backend/crates/shared/audit-log/`) that provides the single, authoritative way to write audit records. It is the only place outside the `audit` module whose SQL names `audit_logs`.

The crate exposes two public functions:

1. **`record_in_tx(conn, action, actor_user_id, tenant_id, resource_type, resource_id, details)`** — writes the audit record in the **caller's transaction**. The record commits or rolls back with the caller's transaction. Returns `sqlx::Result<()>` so the caller must handle failure (and roll back the action). Use for anything a person did (sensitive actions whose audit record must commit atomically).

2. **`record(pool, action, actor_user_id, tenant_id, resource_type, resource_id, details)`** — writes the record in **a transaction of its own** and only logs a failure. Use for a record about something that has already happened and changes nothing that must stay consistent with it (e.g., tool execution, rate-limit refusal). Best-effort.

Both functions handle:

- Tenant-scoped rows: inserted directly inside the caller's `UnitOfWork` (RLS enforces tenant isolation).
- Platform-level events (no tenant): go through `xtenant.record_platform_event`, which accepts only the listed platform actions.
- Nil UUID as actor: stored as NULL (means "the system").

The `audit` module still **owns the `audit_logs` table**: it defines the table in its migrations, enforces append-only via trigger, and serves the read API. The `audit-log` crate only writes.

## Alternatives considered

| Option | Why not |
|---|---|
| **Call `audit` module from `tenancy`** | Creates dependency cycle `tenancy → audit → tenancy`. Violates ADR-017 rule 4 (no producer → event consumer). |
| **Event-based audit writes** | Audit record must commit with the action (Constitution III). Async event write cannot guarantee this. |
| **Each module writes `audit_logs` directly** | Duplicates SQL, no single place to enforce platform-event restrictions, multiple places to update if schema changes. |
| **Keep `tenancy::audit` and have other modules depend on `tenancy`** | `tenancy` is already widely depended on; adding audit there bloats it and doesn't solve the cycle for `tenancy` itself. |

## Consequences

**Positive**

- Single authoritative write path for audit records (no duplication, no drift).
- Transactional semantics are explicit: `record_in_tx` for atomicity, `record` for best-effort.
- Module boundaries respected: no module depends on `audit`; `audit-log` is a shared infrastructure crate like `db` or `outbox`.
- `audit` module remains the table owner and read API.
- Comprehensive integration tests cover transaction semantics, cross-tenant isolation, platform-event restrictions.

**Negative / costs**

- New shared crate adds a dependency edge from ~11 modules to `audit-log` (tracked in `module-boundaries.json`).
- Two APIs to choose from; documentation and code review must ensure correct choice.

**Follow-ups**

- None. MOH-196 is complete.
