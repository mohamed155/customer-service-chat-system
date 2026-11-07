# Architecture Decision Records

An Architecture Decision Record (ADR) captures one significant architectural decision: the
context that forced it, what we decided, the alternatives we rejected, and the consequences.

The [system architecture](../architecture/system-architecture.md) marks decisions that need an
ADR with **ADR-NNN**. Its §22 holds the Phase 0 backlog, which this index tracks.

## Writing an ADR

1. Copy [`0000-template.md`](0000-template.md) to `NNNN-kebab-case-title.md`, using the number
   reserved below (or the next free number for a new decision).
2. Open it in its own PR with the status set to **Accepted** and the date, and update its row in
   this index in the same PR. The open PR is the proposal; the file only reaches `master` by
   being merged, so nothing on `master` says "Proposed".
3. If the PR is closed without merging and the decision is worth keeping on record, merge it
   as **Rejected** instead.
4. Accepted ADRs are not rewritten. To change a decision, write a new ADR that supersedes it,
   and mark the old one **Superseded by ADR-XXX**.

## Statuses

| Status | Meaning |
|---|---|
| Not started | Number reserved in the backlog; no draft yet. |
| Proposed | Under review in an open PR. Not used on `master`; see step 2 above. |
| Accepted | Decision is in force. Code is expected to follow it. |
| Rejected | Considered and declined. Kept for the record. |
| Deprecated | No longer applies, with no replacement. |
| Superseded by ADR-XXX | Replaced by a later ADR. |

## Index

| ADR | Decision | Status |
|---|---|---|
| [ADR-001](0001-modular-monolith-hexagonal-modules.md) | Modular monolith with hexagonal modules | Accepted |
| [ADR-002](0002-rust-backend-stack.md) | Rust/Axum/SQLx backend stack | Accepted |
| [ADR-003](0003-frontend-state-ownership.md) | Angular 22 + NgRx Store/SignalStore state ownership | Accepted |
| [ADR-004](0004-multi-tenancy-rls-cells.md) | Multi-tenancy: shared schema + RLS + composite FKs + cell mapping | Accepted |
| [ADR-005](0005-authentication-tokens.md) | Authentication: short JWT + rotating refresh tokens in HttpOnly cookies | Accepted |
| [ADR-006](0006-authorization-model.md) | Authorization: permission-based RBAC + policy layer | Accepted |
| ADR-007 | PostgreSQL job queue + transactional outbox | Not started |
| ADR-008 | pgvector HNSW + hybrid search with RRF | Not started |
| ADR-009 | SSE for streaming, WebSocket for bidirectional chat | Not started |
| ADR-010 | Separate `gateway` deployment unit for public traffic | Not started |
| ADR-011 | Widget as lightweight Web Component (non-Angular) | Not started |
| ADR-012 | API conventions: `/api/v1`, OpenAPI 3.1, RFC 9457 errors, keyset pagination | Not started |
| ADR-013 | UUIDv7 identifiers | Not started |
| ADR-014 | Envelope encryption for tenant secrets | Not started |
| ADR-015 | Monorepo layout and tooling | Not started |
