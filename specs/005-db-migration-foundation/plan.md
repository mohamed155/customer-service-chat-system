# Implementation Plan: Database & Migration Foundation

**Branch**: `master` (feature dir `005-db-migration-foundation`) | **Date**: 2026-07-07 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/005-db-migration-foundation/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

Establish the schema-evolution foundation for the platform: a documented SQLx migration workflow (local + CI, already partially wired by spec 004) and the four base tables — `users`, `tenants`, `tenant_memberships`, `audit_logs` — with the platform-wide data conventions baked in: DB-generated UUID primary keys, automatic `created_at`/`updated_at`, soft delete via `deleted_at` with partial unique indexes for active-row uniqueness, append-only audit logs enforced at the database level, non-nullable `tenant_id` on tenant-owned tables, and indexes on every tenant-aware access path. Schema correctness is verified by DB integration tests that run against the CI Postgres service and are skipped locally when no database is reachable (same pattern as spec 004's `live_deps.rs`).

## Technical Context

**Language/Version**: Rust, edition 2024 (workspace `backend/`)

**Primary Dependencies**: SQLx 0.8 (postgres, migrate, chrono, uuid features — already in workspace), sqlx-cli (dev/CI tool), uuid 1 (v4/v7), chrono 0.4

**Storage**: PostgreSQL 16 (pgvector/pgvector:pg16 image in dev compose and CI); extensions `vector` + `citext` already enabled by migration `0001_init.sql`; `pgcrypto`-free `gen_random_uuid()` (native in PG13+)

**Testing**: `cargo test` — DB integration tests in `crates/shared/db/tests/` gated on a reachable `DATABASE_URL` (skip-if-unreachable pattern from spec 004's `live_deps.rs`); CI provides a live Postgres service

**Target Platform**: Linux server (CI: ubuntu-latest; local dev: Docker Compose on any OS)

**Project Type**: Web service backend — Cargo workspace, migrations shared at `backend/migrations/`

**Performance Goals**: Tenant-scoped lookups on base tables served by indexes (no seq scans on `tenant_id` paths, verifiable via `EXPLAIN`); full migration run from empty DB completes in seconds

**Constraints**: Each migration atomic (SQLx wraps Postgres migrations in a transaction unless opted out); applied migrations immutable (SQLx checksum verification); no manual schema changes anywhere

**Scale/Scope**: 4 new tables + 2 trigger functions + ~1 workflow document; no production data exists — no backfill concerns

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Assessment |
|---|-----------|------------|
| I | Enterprise Modular Monolith | ✅ Migrations live in the shared `backend/migrations/` directory (single schema for the modular monolith); no cross-module coupling introduced. Module crates (identity, tenancy, audit) will consume these tables behind their own interfaces in later features. |
| II | Multi-Tenant Isolation | ✅ `tenant_memberships` carries non-nullable `tenant_id`; `audit_logs.tenant_id` is nullable **by explicit spec decision** (platform-level actions have no tenant) — tenant-scoped entries are indexed on `(tenant_id, created_at)`. `users` and `tenants` are platform-level identity tables, not tenant-owned. |
| III | Zero-Trust Security & RBAC | ✅ Role sets stored per constitution vocabulary (platform roles on `users`, tenant roles on `tenant_memberships`); `audit_logs` provides the who/what/when + details substrate for auditing sensitive operations. No secrets in migrations. |
| VIII | Database Integrity & Migration Discipline | ✅ This feature *is* the enforcement of Principle VIII: all changes via SQLx migrations, normalized schema, mandatory indexes on production query paths, `tenant_id` on tenant-owned tables. |
| VII | Test-First & Regression Discipline | ✅ Schema behavior (uniqueness, cascades, append-only, defaults) verified by DB integration tests written against acceptance scenarios before/alongside the migrations. |
| VI | Observability | ✅ No runtime code paths added; migration runs are logged by sqlx-cli/CI. Audit substrate supports future observability requirements. |
| X | Performance & Efficiency | ✅ Partial unique indexes double as lookup indexes; `tenant_id` and time-ordered audit indexes defined up front. |

**Gate result**: PASS — no violations, Complexity Tracking not required.

**Post-Phase-1 re-check**: PASS — the data model (see `data-model.md`) introduces no denormalization, every tenant-owned table has `tenant_id`, and DB-level enforcement (triggers, checks, partial unique indexes) keeps integrity independent of application code.

## Project Structure

### Documentation (this feature)

```text
specs/005-db-migration-foundation/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/
│   └── database-schema.md   # Schema contract: tables, constraints, indexes, conventions
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/                          # SQLx migrations (shared schema, applied in order)
│   ├── 0001_init.sql                    # existing: extensions (vector, citext) + set_updated_at()
│   ├── 0002_outbox.sql                  # existing: outbox_events
│   ├── 0003_users.sql                   # NEW: users table + conventions
│   ├── 0004_tenants.sql                 # NEW: tenants table
│   ├── 0005_tenant_memberships.sql      # NEW: memberships + soft-delete cascade triggers
│   ├── 0006_audit_logs.sql              # NEW: audit_logs + append-only trigger
│   └── README.md                        # NEW: migration workflow documentation (FR-007)
└── crates/shared/db/
    ├── src/lib.rs                       # existing: lazy_pool, run_migrations, PgHealthCheck
    └── tests/
        └── schema.rs                    # NEW: DB integration tests (gated on live DATABASE_URL)

.github/workflows/backend.yml            # existing: already runs `sqlx migrate run` against clean CI Postgres
```

**Structure Decision**: Migrations remain in the existing shared `backend/migrations/` directory that `sqlx::migrate!("../../../migrations")` (db crate) and CI's `sqlx migrate run` already point at. Schema tests live in `crates/shared/db/tests/` because the db crate owns pool construction and migration embedding; module crates (identity/tenancy/audit) stay untouched until their features arrive.

## Complexity Tracking

> No constitution violations — table intentionally left empty.
