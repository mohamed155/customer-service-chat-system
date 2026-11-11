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

U1 was resolved by MOH-42; U2 is still follow-up work.

| # | Gap | Where | Status |
| -- | -- | -- | -- |
| U1 | **New IDs were mostly UUIDv4.** There were about 50 `Uuid::new_v4()` calls in production Rust code (about 360 counting tests) and 40 `gen_random_uuid()` defaults in migrations, against one `Uuid::now_v7()`. | `backend/crates/**`, `backend/migrations/*` | **Resolved (MOH-42).** See below. |
| U2 | Newtype IDs exist only for some entities; many signatures take raw `Uuid`. | `backend/crates/shared/kernel` | Open: add newtypes as modules are refactored (ADR-001 M1). |

### How U1 was resolved

- **`kernel::ids::new_id()`** returns a UUIDv7 (`Uuid::now_v7()`). Tests can replace the generator
  with `ids::install_generator(...)`; `SequentialIdGenerator` gives a fixed, ordered sequence.
- **Application-generated keys** now use it: the outbox events of conversations, notifications,
  escalations, knowledge, tools and invitations; customers; knowledge items; AI generations.
- **Database defaults.** Migration `0075_uuidv7_defaults.sql` adds a `uuidv7()` function and makes it
  the default of the `id` column of all 40 tables that had `DEFAULT gen_random_uuid()`. That covers
  every insert that does not name an id: `messages`, `audit_logs`, the rows written by SQL functions
  and the low-volume tables. The function is a stand-in for PostgreSQL 18's built-in; after an
  upgrade the built-in is found first and ours can be dropped. Existing rows keep their v4 ids.
- **What stays v4, on purpose:** session token IDs (`jti`) and the claim tokens of the outbox
  workers. They are tokens, not keys, and should not reveal a time. Each carries a
  `// uuid-v4-ok: <why>` comment.
- **Ordering and pagination** were reviewed. Every list ordered by time uses `(created_at, id)`, so
  `id` is only a tie-breaker. The few lists ordered by `id` alone (the platform tenant list and the
  row locks on memberships) only need a total order, which UUIDs have whatever their version. A test
  pages through a mix of v4 and v7 tenants (`list_tenants_pagination_handles_a_mix_of_v4_and_v7_ids`).
  Do not read a creation time from an id; use `created_at` (decision point 6).
- **Enforcement.** `server/tests/uuid_hygiene.rs` fails on a `Uuid::new_v4()` without the
  `uuid-v4-ok` comment, on a migration after 0075 that uses `DEFAULT gen_random_uuid()` (or
  `uuid_generate_v4()`), and, against a migrated database, on any column that still defaults to a
  random v4. It also checks that the new default gives version 7 values that sort by creation time,
  for the runtime role.
- **Left as it was:** application code that does not set an id and relies on the database default
  gets a v7 from the function, so nothing else needed to change. New tables should either have the
  application supply the id or use `DEFAULT uuidv7()`.
