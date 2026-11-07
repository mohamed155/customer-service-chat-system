# Architecture Decision Records

An Architecture Decision Record (ADR) captures one significant architectural decision: the
context that forced it, what we decided, the alternatives we rejected, and the consequences.

The [system architecture](../architecture/system-architecture.md) marks decisions that need an
ADR with **ADR-NNN**. Its §22 holds the Phase 0 backlog, which this index tracks.

## Writing an ADR

1. Copy [`0000-template.md`](0000-template.md) to `NNNN-kebab-case-title.md`, using the number
   reserved below (or the next free number for a new decision).
2. Open it as **Proposed** in its own PR, and update its row in this index in the same PR.
3. The ADR becomes **Accepted** when that PR merges.
4. Accepted ADRs are not rewritten. To change a decision, write a new ADR that supersedes it,
   and mark the old one **Superseded by ADR-XXX**.

## Statuses

| Status | Meaning |
|---|---|
| Not started | Number reserved in the backlog; no draft yet. |
| Proposed | Draft is written and under review. |
| Accepted | Decision is in force. Code is expected to follow it. |
| Rejected | Considered and declined. Kept for the record. |
| Deprecated | No longer applies, with no replacement. |
| Superseded by ADR-XXX | Replaced by a later ADR. |

## Index

| ADR | Decision | Status |
|---|---|---|
| ADR-001 | Modular monolith with hexagonal modules | Not started |
| ADR-002 | Rust/Axum/SQLx backend stack | Not started |
| ADR-003 | Angular 22 + NgRx Store/SignalStore state ownership | Not started |
| ADR-004 | Multi-tenancy: shared schema + RLS + composite FKs + cell mapping | Not started |
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
