# ADR-013: UUIDv7 identifiers

- **Status:** Accepted (MOH-18)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §7.1 and §19](../architecture/system-architecture.md#71-postgresql-conventions),
  constitution Technology Standards and Principle VIII (newtype IDs), ADR-004 (composite keys),
  ADR-007 (outbox ordering), ADR-012 (keyset pagination)

## Context

Every row needs a primary key, and many IDs appear in URLs, events, logs and client code.

The decision has to balance these forces:

- **No enumeration.** Sequential IDs in URLs (`/conversations/1042`) reveal volume and invite
  guessing other records.
- **Index locality.** Random UUIDs (v4) scatter inserts across the B-tree, causing page splits,
  cache misses and write amplification on large, write-heavy tables (messages, events, audit).
- **Generated anywhere.** IDs should be creatable in application code before insert (for outbox
  events, idempotency, client-side optimistic updates) and across future cells without
  coordination.
- **Ordering.** Time order is useful for keyset pagination and for processing the outbox in order.
- **Type safety.** IDs of different entities must not be mixed up in code.

## Decision

1. **All new primary keys are UUIDv7** (RFC 9562): a 48-bit Unix millisecond timestamp followed
   by random bits. They're time-ordered, so inserts append to the right edge of the B-tree.
2. **Generated in the application** with `uuid::Uuid::now_v7()`, behind a `kernel::ids` helper
   so tests can use a deterministic generator. Database defaults are only a fallback; on
   PostgreSQL 18+ that is the built-in `uuidv7()`, and on earlier versions there's no default.
3. Stored as the native PostgreSQL `uuid` type (16 bytes), never as `text`.
4. Exposed to clients as the standard hyphenated string.
5. **Newtypes in Rust** for every entity ID (`TenantId`, `ConversationId`, …) so they can't be
   mixed up; the frontend uses branded string types where it matters.
6. The embedded timestamp is **not trusted** for business logic (it's set by whatever created
   the ID, and clocks drift). Use explicit `created_at` columns for time.
7. **Privacy note:** a UUIDv7 reveals roughly when a record was created. That's acceptable for
   our resources. Where creation time is sensitive, use a v4 public identifier instead.
8. Existing UUIDv4 keys stay as they are. They're valid UUIDs and coexist with v7 in the same
   column; only newly generated IDs change.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **`bigserial` / identity integers** | Small and fast, but enumerable in URLs, need the database to allocate, and collide across cells or shards. |
| **UUIDv4** (random) | No enumeration, but random insert positions hurt B-tree locality on large tables. This is the current default (see below). |
| **ULID** | Similar ordering properties, but a non-standard text encoding; storing it as `uuid` loses its point. UUIDv7 is now the standard (RFC 9562). |
| **Snowflake-style 64-bit IDs** | Compact, but need worker-ID coordination, and are larger than JavaScript's safe integer range, so they'd travel as strings anyway. |
| **Integer PK + separate public UUID** | Two identifiers per row to keep in sync, more joins and indexes. |

## Consequences

**Positive**

- Better insert locality and index cache behaviour on large tables.
- IDs are generated anywhere, without coordination, and sort by creation time.
- No enumerable IDs in URLs.

**Negative and costs**

- IDs leak approximate creation time.
- Mixed v4/v7 keys in existing tables until data ages out; ordering by ID is only meaningful
  for v7 rows (use `created_at` for ordering when both exist).
- 16-byte keys are larger than integers, especially in composite indexes (ADR-004).

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| U1 | **New IDs are mostly UUIDv4.** There are about 360 `Uuid::new_v4()` calls in Rust and about 40 `gen_random_uuid()` defaults in migrations, against one `Uuid::now_v7()`. The `uuid` crate already has the `v7` feature enabled. | `backend/crates/**`, `backend/migrations/*` | Add `kernel::ids::new_id()` returning v7, switch call sites (mechanical), and drop `gen_random_uuid()` defaults in new migrations. Start with the highest-volume tables: messages, outbox, audit, AI turns. |
| U2 | Newtype IDs exist only for some entities; many signatures take raw `Uuid`. | `backend/crates/shared/kernel` | Add newtypes as modules are refactored (ADR-001 M1). |
