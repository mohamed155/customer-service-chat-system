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

# Create a new migration file
sqlx migrate add -r description_of_change
```

Run these from `backend/` with `DATABASE_URL` set.

## Naming

`NNNN_description.sql` — sequential zero-padded number, underscore-separated
description (e.g. `0003_users.sql`). One concern per migration file.

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

## CI

`.github/workflows/backend.yml` (when present) applies the full migration
history to a clean Postgres service on every push/PR. A failing migration or
checksum mismatch fails the build.
