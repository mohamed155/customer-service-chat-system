# Quickstart: Database & Migration Foundation

**Feature**: 005-db-migration-foundation
**Proves**: migrations run locally and in CI, schema reproducible from scratch, base tables behave per [contracts/database-schema.md](contracts/database-schema.md).

## Prerequisites

- Docker (for local Postgres) — `infra/docker-compose.yml` provides `pgvector/pgvector:pg16`
- Rust toolchain + sqlx-cli: `cargo install sqlx-cli --no-default-features --features rustls,postgres`
- `DATABASE_URL` exported (see `backend/.env.example`), e.g.
  `postgres://customer_service:customer_service_dev@localhost:5432/customer_service`

## 1. Schema from scratch (SC-001, FR-004)

```bash
docker compose -f infra/docker-compose.yml up -d postgres
cd backend
sqlx database reset -y     # drop + recreate empty DB, then apply ALL migrations
```

**Expected**: exits 0; every migration `0001…000N` listed as applied.

Verify the tables exist:

```bash
psql "$DATABASE_URL" -c "\dt"    # users, tenants, tenant_memberships, audit_logs, outbox_events, _sqlx_migrations
```

## 2. Idempotent re-run (FR-005)

```bash
sqlx migrate run
```

**Expected**: exits 0, applies nothing ("no migrations to run" / silent).

## 3. Immutability tripwire (FR-006)

Temporarily append a comment to an already-applied migration file, then:

```bash
sqlx migrate run    # EXPECTED: error — checksum mismatch for the modified version
git checkout -- migrations/   # restore
```

## 4. Behavioral verification — schema tests (FR-009…FR-020)

```bash
cd backend
cargo test -p db --test schema
```

Runs the DB integration tests (they skip with a notice if `DATABASE_URL` is unreachable). **Expected**: all pass, covering:

- UUID PK + timestamps auto-populated on bare insert
- email uniqueness among active users; reuse allowed after soft delete
- tenant slug uniqueness + rename; status CHECK rejects unknown values
- membership FK enforcement, duplicate active membership rejected
- soft-delete cascade: deleting tenant/user soft-deletes memberships
- audit log UPDATE/DELETE raise database errors (append-only)
- `EXPLAIN` shows index usage for `tenant_id`-filtered membership queries (SC-005)

## 5. CI validation (FR-003, SC-003)

Push a branch / open a PR. `.github/workflows/backend.yml` already:

1. boots a clean `pgvector:pg16` service,
2. installs `sqlx-cli` and runs `sqlx migrate run` against the clean service to apply the full migration history,
3. runs `cargo test --workspace` (schema tests reuse the migrated service).

**Expected**: pipeline green; any broken migration or schema regression fails the build.

## 6. Workflow documentation check (FR-007)

`backend/migrations/README.md` exists and documents: naming (`NNNN_description.sql`), one concern per file, never edit applied migrations (fix-forward), renumbering on rebase collisions, and the local commands above.
