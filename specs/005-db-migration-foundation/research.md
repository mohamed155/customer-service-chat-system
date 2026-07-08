# Research: Database & Migration Foundation

**Feature**: 005-db-migration-foundation | **Date**: 2026-07-07

No `NEEDS CLARIFICATION` markers existed in Technical Context (stack is fixed by the constitution and spec 004's groundwork). Research therefore resolves the design decisions the spec leaves to planning.

## R1. Migration tool & invocation

- **Decision**: SQLx migrations in `backend/migrations/`, applied with `sqlx migrate run` (sqlx-cli) in dev and CI; migrations also embedded in the binary via the existing `db::run_migrations` (`sqlx::migrate!`) for programmatic use by tests. Applying migrations at server startup remains a deployment decision deferred to a later ops feature — dev/CI apply explicitly.
- **Rationale**: Already the de-facto standard in this repo: CI runs `sqlx migrate run` (backend.yml), the db crate embeds `sqlx::migrate!("../../../migrations")`, and two migrations exist. SQLx's `_sqlx_migrations` table gives exactly-once ordered application (FR-005), checksum verification rejects modified applied migrations (FR-006), and Postgres migrations run inside a transaction by default → atomic per migration (FR-018).
- **Alternatives considered**: `refinery` and `flyway` (extra tool, no benefit over what's wired); auto-migrate on server startup (rejected for now — racy with multiple replicas, and CI/dev don't need it).

## R2. Migration naming & conflict handling

- **Decision**: Keep the existing sequential `NNNN_description.sql` scheme (next: `0003_…`). One concern per migration file. Concurrent-branch numbering collisions are resolved at rebase time by renumbering the unapplied migration; CI applying the full history to a clean database catches any collision or ordering break before merge. Down/revert migrations are not used — the policy is fix-forward via a new migration (no production data exists; spec Assumptions).
- **Rationale**: Matches the two existing files; sequential numbers make order obvious in review. SQLx supports timestamps too, but mixing schemes mid-stream hurts readability.
- **Alternatives considered**: Timestamp prefixes (avoid renumber conflicts but this team is small and CI catches collisions anyway); reversible `.up.sql`/`.down.sql` pairs (rejected — fix-forward policy, halves the files to maintain).

## R3. UUID primary key generation

- **Decision**: `id UUID PRIMARY KEY DEFAULT gen_random_uuid()` — database-generated UUIDv4.
- **Rationale**: FR-009 requires generation "without caller involvement"; a DB default is the only mechanism that holds for every writer (app code, fixtures, psql). `gen_random_uuid()` is native in PostgreSQL 13+ (no extension). UUIDv7's index-locality benefit is real but native support arrives in PG18; app-side v7 generation would violate the "no caller involvement" guarantee.
- **Alternatives considered**: App-generated UUIDv7 via the uuid crate (better b-tree locality, but pushes generation onto every caller); `uuid-ossp` extension (obsolete next to `gen_random_uuid()`); BIGSERIAL (rejected — spec mandates UUID; enumerable IDs leak tenant scale).

## R4. Timestamps

- **Decision**: `created_at TIMESTAMPTZ NOT NULL DEFAULT now()` and `updated_at TIMESTAMPTZ NOT NULL DEFAULT now()` on every table, with the existing `set_updated_at()` trigger function (from `0001_init.sql`) attached as a `BEFORE UPDATE` trigger per table.
- **Rationale**: FR-010 requires automatic population; the trigger function already exists precisely for this. TIMESTAMPTZ is the non-negotiable Postgres best practice (UTC-normalized).
- **Alternatives considered**: App-maintained timestamps (unenforceable for out-of-band writers); `updated_at` nullable until first update (complicates queries for no benefit).

## R5. Soft delete & active-row uniqueness

- **Decision**: `deleted_at TIMESTAMPTZ NULL` on `users`, `tenants`, `tenant_memberships` (not on `audit_logs`). Active-row uniqueness via **partial unique indexes** `… WHERE deleted_at IS NULL`: users on email, tenants on slug, memberships on `(tenant_id, user_id)`. Soft-delete cascade (tenant→memberships, user→memberships; FR-020) enforced by `AFTER UPDATE` triggers on `tenants` and `users` that stamp `deleted_at` onto active memberships when the parent transitions from active to deleted.
- **Rationale**: Partial unique indexes are the canonical Postgres pattern for "unique among active" (FR-011, FR-015) and double as the lookup indexes for those columns (FR-017). DB-level cascade triggers make FR-020 testable in this feature — no service layer exists yet, and integrity that lives in the DB can't be bypassed by a buggy client (Principle II's spirit).
- **Alternatives considered**: Unique constraint on `(email, deleted_at)` (broken — NULLs never conflict... actually multiple active rows would conflict correctly but multiple deletions at the same instant collide; partial index is cleaner); app-layer cascade only (untestable in this feature, bypassable); `is_deleted BOOLEAN` (loses the deletion timestamp the spec requires).

## R6. Email & slug column types

- **Decision**: `CITEXT` for `users.email` and `tenants.slug` (extension already enabled in `0001_init.sql`), with a `CHECK` enforcing slug format (`^[a-z0-9](-?[a-z0-9])*$`, length ≤ 63) and a minimal email sanity `CHECK` (`position('@' in email) > 1`).
- **Rationale**: Case-insensitive uniqueness at the type level ("Foo@Bar.com" = "foo@bar.com") without `lower()` expression indexes everywhere. Slug format check keeps handles URL-safe from day one; slug is renamable (spec clarification) so uniqueness re-validation is just the partial unique index doing its job on UPDATE.
- **Alternatives considered**: `TEXT` + `lower()` expression indexes (works, but every query must remember to lower); full RFC-5322 email validation in a CHECK (rejected — validation belongs in the application; the DB check only guards against garbage).

## R7. Role and status storage

- **Decision**: `TEXT` columns with `CHECK` constraints: `users.platform_role` nullable, one of `super_admin|developer|sales|support|finance`; `tenant_memberships.role` non-nullable, one of `owner|admin|manager|agent|viewer`; `tenants.status` non-nullable `DEFAULT 'active'`, one of `active|suspended` (FR-016, FR-019).
- **Rationale**: CHECK-constrained TEXT is trivially evolvable by migration (drop/re-add constraint) and reads naturally in every tool. Native Postgres enums can't remove or reorder values and complicate SQLx type mapping for no gain at this scale.
- **Alternatives considered**: Native `CREATE TYPE … AS ENUM` (rejected for evolvability); lookup/reference tables (over-normalized for fixed constitutional vocabularies).

## R8. Audit log shape & append-only enforcement

- **Decision**: `audit_logs` with `actor_user_id UUID NULL` (FK to users; NULL = system actor), `action TEXT NOT NULL`, `resource_type TEXT NOT NULL`, `resource_id TEXT NULL`, `tenant_id UUID NULL` (FK to tenants; NULL = platform-level action), `details JSONB NOT NULL DEFAULT '{}'`, `created_at` — no `updated_at`, no `deleted_at`. Append-only enforced by a `BEFORE UPDATE OR DELETE` trigger that raises an exception (FR-012).
- **Rationale**: Matches the clarified spec exactly (structured details payload; nullable tenant context). The exception trigger makes immutability a database property rather than a convention — the strongest guarantee available without table-level GRANT management (which the platform doesn't do yet since the app connects as owner). `resource_id` as TEXT keeps the log able to reference non-UUID resources (config keys, external IDs) without schema churn.
- **Alternatives considered**: `REVOKE UPDATE, DELETE` (ineffective while the app role owns the table); rules instead of triggers (deprecated practice); partitioning by month (premature — revisit when volume justifies it).

## R9. Foreign keys & referential integrity

- **Decision**: Real FKs everywhere they apply: `tenant_memberships.user_id → users(id)`, `tenant_memberships.tenant_id → tenants(id)`, `audit_logs.actor_user_id → users(id)`, `audit_logs.tenant_id → tenants(id)`. All `ON DELETE RESTRICT` (the default) — hard deletes don't exist in the design, so RESTRICT is a tripwire against them. Audit FKs stay valid under soft delete because rows are never physically removed (spec edge case: audit history survives subject deletion).
- **Rationale**: FR-016 demands memberships reference existing rows; FKs are the enforcement. RESTRICT turns any accidental hard delete into a loud error instead of silent orphaning.
- **Alternatives considered**: No FKs "for scale" (unjustified at this stage, violates Principle VIII normalization intent); `ON DELETE CASCADE` (dangerous — would make an accidental hard delete destroy child history).

## R10. Index set (FR-017)

- **Decision**:
  - `users`: partial unique `(email) WHERE deleted_at IS NULL` (doubles as lookup index)
  - `tenants`: partial unique `(slug) WHERE deleted_at IS NULL`
  - `tenant_memberships`: partial unique `(tenant_id, user_id) WHERE deleted_at IS NULL` (serves tenant-scoped listing + duplicate prevention); plain index on `(user_id)` (memberships-by-user)
  - `audit_logs`: `(tenant_id, created_at DESC)` (tenant timeline, dominant query); `(created_at DESC)` for platform-wide review
- **Rationale**: Every access path named in the spec (user by email, tenant by slug, membership by user, membership by tenant, audit by tenant+time) is index-served; nothing speculative beyond that (Principle X without index bloat).
- **Alternatives considered**: Index on `audit_logs.actor_user_id` (deferred — no named query path yet; add when an "actions by user" feature lands); GIN on `details` JSONB (deferred — no query pattern defined).

## R11. Schema verification tests

- **Decision**: Integration test file `crates/shared/db/tests/schema.rs` that connects using `DATABASE_URL`, runs `db::run_migrations`, and exercises the acceptance scenarios: idempotent re-run, email uniqueness among active users (and reuse after soft delete), membership FK + duplicate rejection, tenant status CHECK, audit append-only (UPDATE/DELETE must error), soft-delete cascade triggers, and `EXPLAIN`-based assertion that a `tenant_id` filter on memberships uses an index. Tests skip gracefully when the database is unreachable (spec 004's `live_deps.rs` pattern) so `cargo test` stays green without Docker; CI always has the Postgres service.
- **Rationale**: Principle VII requires tests; these map 1:1 to the spec's acceptance scenarios and run on every CI push against a from-scratch schema — which simultaneously satisfies FR-003/FR-004 (CI rebuilds schema from empty) with zero new CI plumbing.
- **Alternatives considered**: `sqlx::test` attribute macro (creates per-test databases — nice, but requires more setup and differs from the repo's established gated-live-test pattern); pure SQL assertion scripts (weaker diagnostics, another toolchain).

## R12. Pre-existing `outbox_events.tenant_id TEXT`

- **Decision**: Out of scope. `outbox_events` (spec 004) uses `tenant_id TEXT NULL`, which predates the `tenants` table. Aligning it to `UUID` is a one-line follow-up migration best done by the feature that first writes tenant-scoped events, when the semantics are clear.
- **Rationale**: The spec bounds this feature to the four base tables; touching outbox now would widen scope without a consumer to validate against.
