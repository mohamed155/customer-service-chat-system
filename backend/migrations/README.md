# Database Migrations

## Workflow

All schema changes go through SQLx migrations in this directory. Migrations are
applied in ascending numerical order and are tracked by the `_sqlx_migrations`
table.

## Commands

```bash
# Apply all pending migrations
sqlx migrate run

# Drop and recreate the database, then apply all migrations from scratch
sqlx database reset -y

# Create a new migration file (forward-only, no down script)
sqlx migrate add description_of_change
# Then rename the generated timestamp-prefixed file to the sequential
# NNNN_description.sql format (e.g. 0008_cascade_fix.sql)
```

Run these from `backend/` as the schema owner, `app_owner`: pass
`--database-url "$MIGRATION_DATABASE_URL"` (sqlx-cli reads `DATABASE_URL` by default, which is
the `app_runtime` role and cannot change the schema), or use `cargo run -p server -- migrate`.
The server never runs migrations on startup.

Tables and sequences a migration creates are readable and writable by `app_runtime` through the
default privileges set in `0066_runtime_role_grants.sql`; don't add per-table grants.

A migration that changes **rows** (an `UPDATE`, a backfill, a data fix) in a table with forced
Row-Level Security touches zero rows when run as `app_owner`, because the policy applies to the owner
too, and it reports success. Start such a migration with `SET LOCAL ROLE app_platform;` (BYPASSRLS) and end
it with `RESET ROLE;`, or set `app.tenant_id` for one tenant. Check `relforcerowsecurity` in `pg_class`
for the tables that are covered.

A function that must read across tenants (a lookup before the tenant is known, a job that spans
tenants) goes in the `xtenant` schema, owned by `app_platform`: start the file with `SET LOCAL ROLE
app_platform;`, create the function with `SECURITY DEFINER` and `SET search_path = pg_catalog,
public, pg_temp`, `REVOKE ALL ... FROM PUBLIC`, `GRANT EXECUTE ... TO app_runtime` (still as
`app_platform`), and end with `RESET ROLE;`. Call `xtenant._audit` when it returns or changes
something. See `0068_cross_tenant_lookups.sql` and [the audited cross-tenant
path](../../docs/security/tenant-unit-of-work.md#the-audited-cross-tenant-path). `cargo nextest run
-p db --test xtenant` checks every function in the schema.

## Naming

`NNNN_description.sql` — sequential zero-padded number, underscore-separated
description (e.g. `0003_users.sql`). One concern per migration file.
After `sqlx migrate add`, rename the generated file to follow this convention.

## Rules

- **Never edit an applied migration.** SQLx stores a checksum for each applied
  migration; modifying the file after it has been applied will cause `sqlx
  migrate run` to fail with a version-mismatch error.
- **Fix forward.** To correct a mistake, write a new migration that applies the
  fix — never edit history.
- **Renumber on rebase collisions.** If two branches add migrations with the
  same number, renumber the unapplied one before merging. CI catches ordering
  breaks by applying the full history to a clean database.
- **No down migrations.** The policy is fix-forward only.

## Review

Every migration is reviewed before merge:

1. The PR may include the new migration file **and any coordinated
   application, documentation, or test changes required to keep the
   codebase in a working state** (for example, the application code
   that calls `set_audit_actor()` or the test that asserts a new
   CHECK). Edits to an **already-applied** migration file are not
   allowed — see *Rules* above.
2. Existing triggers, functions, and CHECK constraints must be
   evolved through a **new fix-forward migration** that
   `CREATE OR REPLACE`s the function and `DROP TRIGGER IF EXISTS` /
   re-creates the trigger, or that adjusts the constraint. Do not
   amend the migration that originally introduced the object.
3. The author must call out: new tables / columns, new or modified
   triggers, new or modified constraints, new indexes, and any new
   runtime expectations placed on application code (for example,
   requiring `set_audit_actor()` before `tenants` slug updates, or
   the `audit_logs.resource_id NOT NULL` contract from migration
   0013).
4. The reviewer must run `sqlx database reset -y` against a local
   Postgres and re-run the schema test suite (`cargo test -p db
   --test schema`) before approving.

## Destructive changes

Some migrations cannot be cleanly undone with a follow-up. Treat these
as **destructive** and require an extra review step:

- Dropping or renaming a table or column.
- Changing a column's type, nullability, or CHECK constraint in a way
  that fails for existing rows.
- Removing a trigger, function, or index.
- Adding a `NOT NULL` constraint on a table that already contains data
  (e.g. `audit_logs.resource_id` in migration 0013 — applied only
  because the codebase was clean at the time).
- Soft-delete semantics: a migration that changes the meaning of
  `deleted_at` or removes the soft-delete cascade.

For destructive changes:

1. The PR must include a *Rollout & rollback* section explaining how
   the team would recover if the migration fails halfway. Fix-forward
   is the default; a real rollback requires a new migration that
   restores the dropped state.
2. Run the schema suite against a database that already has the
   previous migration applied *and* representative seed data
   (`cargo test -p db --test schema`).
3. The PR must be approved by a second reviewer familiar with the
   affected tables.

## pgvector Requirements

The `vector` extension (pgvector) is required for embedding storage and similarity search.

- **Minimum version:** ≥ 0.8.0
- **Check version:**
  ```sql
  SELECT extversion FROM pg_extension WHERE extname = 'vector';
  ```
- **Why 0.8.0:** Earlier versions lack `hnsw.iterative_scan` support, which is required for filtered-ANN queries (combining vector similarity with metadata filters in a single index scan).

## CI

`.github/workflows/backend.yml` (when present) applies the full migration
history to a clean Postgres service on every push/PR. A failing migration or
checksum mismatch fails the build.
