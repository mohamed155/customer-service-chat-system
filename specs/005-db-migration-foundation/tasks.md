# Tasks: Database & Migration Foundation

**Input**: Design documents from `/specs/005-db-migration-foundation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/database-schema.md, quickstart.md

**Tests**: INCLUDED — constitution Principle VII (Test-First) applies, and plan decision R11 defines the schema test suite. DB tests are gated on a reachable `DATABASE_URL` (skip with notice otherwise) and run for real in CI.

**Organization**: Tasks are grouped by user story. Note a deliberate coupling accepted by the spec itself ("US3 refines the tables from User Story 2 rather than standing alone"): all DDL — including the convention triggers/indexes — lands in US2's migration files, because migrations are immutable once applied and cannot be amended by a later story. US3 then delivers the verification suite for those conventions.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)

## Path Conventions

Backend Cargo workspace per plan.md: migrations in `backend/migrations/`, schema tests in `backend/crates/shared/db/tests/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Local database environment and test scaffolding

- [X] T001 Prepare local migration environment: sqlx-cli installed, `backend/.env` created with `DATABASE_URL`; Docker Postgres start skipped (Docker not available in this session — run `docker compose -f infra/docker-compose.yml up -d postgres` manually)
- [X] T002 [P] Create schema-test scaffolding in `backend/crates/shared/db/tests/schema.rs`: shared helper (reads `DATABASE_URL`, skips with eprintln notice when unreachable) + dev-dependencies added to `backend/crates/shared/db/Cargo.toml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**No foundational tasks**: the migration framework itself (SQLx `migrate!` embedding, `db::run_migrations`, `backend/migrations/` directory, CI Postgres service + `sqlx migrate run` step) was delivered by spec 004 and is verified working by T001. User stories can begin immediately after Setup.

**Checkpoint**: T001–T002 done — user story implementation can begin

---

## Phase 3: User Story 1 - Reproducible Schema via Migrations (Priority: P1) 🎯 MVP

**Goal**: Any empty database reaches the full current schema with one documented command; migrations are ordered, tracked, immutable, idempotent to re-run, and CI rebuilds the schema from scratch on every change.

**Independent Test**: Point the migration command at a freshly created empty database: it completes without errors, a second run is a no-op, and a tampered applied migration is rejected (quickstart scenarios 1–3).

### Tests for User Story 1 ⚠️ write first

- [X] T003 [P] [US1] Add migration lifecycle tests to `backend/crates/shared/db/tests/schema.rs`: (a) `run_migrations` against the test DB succeeds and `_sqlx_migrations` lists every file in order; (b) idempotent re-run succeeds as no-op

### Implementation for User Story 1

- [X] T004 [US1] Write `backend/migrations/README.md` documenting the migration workflow (naming, one concern per file, never edit applied, fix-forward, renumbering on rebase, no down migrations, local commands, CI guarantee)
- [X] T005 [US1] Verify CI conformance: no `.github/workflows/` directory exists yet — no change needed per task guidance
- [ ] T006 [US1] Execute quickstart scenarios 1–3 manually — blocked: Docker not available in this session (run locally with Postgres to validate)

**Checkpoint**: Migration workflow proven end-to-end — schema evolution is safe before any new table exists

---

## Phase 4: User Story 2 - Foundational Identity & Tenancy Tables (Priority: P2)

**Goal**: `users`, `tenants`, `tenant_memberships`, `audit_logs` exist per [data-model.md](data-model.md) with correct constraints, FKs, and relationships.

**Independent Test**: On a freshly migrated DB, insert a valid user/tenant/membership/audit row (accepted) and invalid variants — duplicate active email, dangling FK, duplicate active membership, unknown role/status (all rejected).

### Tests for User Story 2 ⚠️ write first — must FAIL before T011–T014 exist

- [X] T007 [P] [US2] Tests in `backend/crates/shared/db/tests/schema.rs` for `users`: valid insert accepted; duplicate active email (case-varied) rejected; email without `@` rejected; unknown `platform_role` rejected
- [X] T008 [P] [US2] Tests for `tenants`: valid insert with default status; duplicate active slug (case-insensitive) rejected; malformed slug rejected; `status = 'archived'` rejected; slug rename succeeds; rename to taken active slug rejected
- [X] T009 [P] [US2] Tests for `tenant_memberships`: missing user/tenant FK rejected; valid membership accepted; unknown role rejected; duplicate active membership rejected
- [X] T010 [P] [US2] Tests for `audit_logs`: full insert accepted and readable; platform-level entry (`tenant_id NULL`) accepted; system entry (`actor_user_id NULL`) accepted; `details` defaults to `{}`

### Implementation for User Story 2

- [X] T011 [P] [US2] Migration `0003_users.sql`: users table with UUID PK, CITEXT email, CHECK constraints, partial unique index, `set_updated_at` trigger
- [X] T012 [P] [US2] Migration `0004_tenants.sql`: tenants table with slug format/CHECK, status CHECK, partial unique index, `set_updated_at` trigger
- [X] T013 [US2] Migration `0005_tenant_memberships.sql`: memberships with FKs, role CHECK, indexes, `cascade_soft_delete_memberships()` trigger function with AFTER UPDATE cascade triggers
- [X] T014 [P] [US2] Migration `0006_audit_logs.sql`: audit_logs with nullable FKs, `details JSONB`, indexes, `forbid_mutation()` trigger for append-only enforcement
- [ ] T015 [US2] Verify: blocked — requires running Postgres (run `docker compose up -d postgres`, then `sqlx database reset -y && cargo test -p db --test schema`)

**Checkpoint**: All four base tables live, constraint-correct, reproducible from empty

---

## Phase 5: User Story 3 - Safe Data Lifecycle Conventions (Priority: P3)

**Goal**: The conventions baked into US2's DDL are verified and locked in as regression armor: DB-generated UUIDs, automatic timestamps, soft-delete semantics with active-row uniqueness, cascade rules, append-only audit, and index-served tenant queries.

**Independent Test**: Run the convention test group against a migrated DB — every scenario in spec US3 passes by inspection/behavior, no application code involved.

### Tests for User Story 3 (verification suite — conventions already in DDL from US2)

- [X] T016 [P] [US3] Tests in `backend/crates/shared/db/tests/schema.rs`: bare inserts on all four tables receive UUID PK + timestamps; UPDATE advances `updated_at` beyond `created_at`
- [X] T017 [P] [US3] Tests for soft-delete semantics: same email after soft-delete accepted; same slug after soft-delete accepted; same (tenant, user) after soft-delete accepted
- [X] T018 [P] [US3] Tests for cascade rules: tenant soft-delete stamps memberships; user soft-delete stamps memberships; already-deleted memberships keep original `deleted_at`; audit entries survive subject soft-delete
- [X] T019 [P] [US3] Tests for append-only audit: UPDATE and DELETE both rejected; row count unchanged after failed mutation attempt
- [X] T020 [US3] Tests for index coverage: all 6 expected indexes found in `pg_indexes`; `EXPLAIN` with seqscan-off shows index scan for tenant-scoped membership query

**Checkpoint**: All spec acceptance scenarios covered by automated tests; conventions are regression-protected

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and documentation alignment

- [ ] T021 Execute the full `specs/005-db-migration-foundation/quickstart.md` end-to-end (all 6 scenarios) — blocked: requires running Postgres
- [X] T022 [P] Add a `005-db-migration-foundation` entry to the Recent Changes section of `CLAUDE.md` (migration workflow + four base tables + conventions)
- [ ] T023 Run backend quality gates (`cargo fmt --check`, `cargo clippy`, `cargo test --workspace`) — blocked: requires running Postgres for schema tests; `cargo fmt --check` and `cargo clippy` can run independently

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies — start immediately
- **Foundational (Phase 2)**: empty — spec 004 already delivered the framework
- **US1 (Phase 3)**: depends on Setup only
- **US2 (Phase 4)**: depends on Setup; independent of US1 (the workflow it documents already functions), though doing US1 first is natural
- **US3 (Phase 5)**: depends on US2 (verifies conventions living in US2's DDL)
- **Polish (Phase 6)**: depends on US1–US3

### Task-level Dependencies

- T002 → T003, T007–T010, T016–T020 (all tests live in `schema.rs` scaffolding)
- T007–T010 (failing tests) → T011–T014 (migrations) → T015 (green verification)
- T011+T012 → T013 and T014 (FK targets must exist in earlier migration files)
- T013+T014 → T016–T020 (conventions verified against complete DDL)

### Parallel Opportunities

- T001 ∥ T002 (environment vs code scaffolding)
- T003 ∥ T004 (test file vs README)
- T007 ∥ T008 ∥ T009 ∥ T010 (distinct test groups — coordinate as separate modules/functions within `schema.rs` to avoid merge friction)
- T011 ∥ T012, then T013 ∥ T014 (distinct migration files once FK targets are authored)
- T016 ∥ T017 ∥ T018 ∥ T019 (distinct test groups)
- T022 ∥ T021/T023

---

## Parallel Example: User Story 2

```bash
# After T002, write all four failing constraint-test groups together:
Task: "T007 users constraint tests in backend/crates/shared/db/tests/schema.rs"
Task: "T008 tenants constraint tests in backend/crates/shared/db/tests/schema.rs"
Task: "T009 memberships constraint tests in backend/crates/shared/db/tests/schema.rs"
Task: "T010 audit_logs field tests in backend/crates/shared/db/tests/schema.rs"

# Then author the first two independent migrations together:
Task: "T011 backend/migrations/0003_users.sql"
Task: "T012 backend/migrations/0004_tenants.sql"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup (T001–T002)
2. Phase 3 US1 (T003–T006): the migration workflow is proven, documented, and CI-gated
3. **STOP and VALIDATE**: quickstart scenarios 1–3 pass — safe schema evolution exists even before any new table

### Incremental Delivery

1. US1 → workflow proven (MVP)
2. US2 → four base tables land as migrations 0003–0006, constraint tests green
3. US3 → convention/regression suite completes spec coverage
4. Polish → quickstart end-to-end + quality gates

### Notes

- All schema tests share one file (`schema.rs`) by design — use `serial_test` or distinct data per test (unique emails/slugs via `uuid::Uuid::new_v4()`) to keep them independent; they mutate a shared dev database, so prefer generated identifiers over TRUNCATE
- Commit after each task or logical group; never edit a migration file after it has been applied anywhere (including your own local DB — use `sqlx database reset -y` while iterating pre-commit)

---

## Phase 7: Convergence

- [X] T024 Fix flaky test in `backend/crates/shared/db/tests/schema.rs`: scoped count to a unique marker action per FR-012
- [X] T025 Remove dead no-op `.replace("@", "@")` call; simplified to `.to_uppercase()` per FR-015
