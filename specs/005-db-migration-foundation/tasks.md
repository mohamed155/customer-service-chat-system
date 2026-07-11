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
- [X] T006 [US1] Execute quickstart scenarios 1–3 manually — validated with live Postgres

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
- [X] T015 [US2] Verify: all 38 schema tests pass against live Postgres

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

- [X] T021 Execute the full `specs/005-db-migration-foundation/quickstart.md` end-to-end (all 6 scenarios) — verified with live Postgres
- [X] T022 [P] Add a `005-db-migration-foundation` entry to the Recent Changes section of `CLAUDE.md` (migration workflow + four base tables + conventions)
- [X] T023 Run backend quality gates (`cargo fmt --check`, `cargo clippy`, `cargo test --workspace`) — all pass; schema tests gracefully skip when Postgres unavailable (design intent)

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

---

## Phase 8: Convergence

- [X] T026 CRITICAL Add `.github/workflows/backend.yml` to provision clean Postgres, apply the full migration history, and run live schema tests on every change per FR-003, SC-003, and US1/AC3 (missing)
- [X] T027 Add database enforcement and regression tests rejecting active memberships whose tenant or user is already soft-deleted per FR-020 (partial)
- [X] T028 Replace the cross-entity OR cascade with correctly scoped tenant and user membership cascades, including a UUID-collision regression test, per FR-020 (contradicts)
- [X] T029 Record successful tenant handle changes in `audit_logs` and verify the audit details per FR-015a (missing)
- [X] T030 Add an automatically populated `updated_at` column to `audit_logs` via a new fix-forward migration and verify it per FR-010 and US3/AC1-2 (missing)
- [X] T031 Enforce and test sufficient actor and affected-resource identity for traceable audit entries per FR-014, SC-006, and US2/AC3 (partial)
- [X] T032 Correct `backend/migrations/README.md` so migration creation produces sequential forward-only files without contradicting the no-down-migrations policy per FR-007 (contradicts)
- [X] T033 Strengthen the migration lifecycle test to assert every repository migration version is tracked in deterministic order per FR-005 and T003 (partial)
- [X] T034 Add query-plan verification that tenant-and-time audit lookups use `audit_logs_tenant_created_idx` per FR-017 and SC-005 (partial)
- [X] T035 Complete timestamp-default and update-advancement regression coverage across all applicable base tables per FR-010 and T016 (partial)

---

## Phase 9: Convergence

- [X] T036 Enforce and test the deleted-parent membership guard on reactivation and parent-changing UPDATE operations per FR-020 and T027 (partial)
- [X] T037 Lock parent rows during membership validation and add a concurrent insert/soft-delete regression test so no active membership can escape cascades per FR-020 (partial)
- [X] T038 Require caller identity for tenant handle changes and persist it in the generated audit entry per FR-014, FR-015a, and SC-006 (contradicts)
- [X] T039 Require affected-resource identity on every audit entry while preserving the documented system-actor representation per FR-014, SC-006, and T031 (partial)
- [X] T040 Strengthen the lifecycle test to compare `_sqlx_migrations` with the exact repository migration versions and descriptions per FR-005 and T033 (partial)
- [X] T041 Run `sqlx migrate run` against the clean CI database before executing the workspace tests per FR-003, SC-003, and T026 (partial)
- [X] T042 Re-query `created_at` after user, tenant, and membership updates and assert it remains unchanged per FR-010 and T035 (partial)
- [X] T043 Assert that the audit tenant-and-time query plan specifically uses `audit_logs_tenant_created_idx` per FR-017, SC-005, and T034 (partial)

---

## Phase 10: Convergence

- [X] T044 Reject tenant handle changes when valid caller identity is absent, persist actor context from the same explicit transaction/connection, and test authenticated success plus unauthenticated rejection per FR-014, FR-015a, SC-006, and T038 (contradicts)
- [X] T045 Add a deterministic two-connection regression test racing membership insertion against parent soft deletion and assert no active membership can reference the deleted parent per FR-020 and T037 (partial)

---

## Phase 11: Convergence

- [X] T046 Restore transaction-local audit actor context and execute actor setup plus tenant handle rename through the same explicit transaction/connection, with a regression proving actor identity does not leak across pooled transactions, per FR-014, FR-015a, SC-006, and T044 (contradicts)
- [X] T047 Replace the timing-only concurrency test sleep with deterministic coordination that proves the parent soft-delete is blocked until the membership transaction releases its lock per FR-020 and T045 (partial)

---

## Phase 12: Convergence

- [X] T048 Capture connection B's PostgreSQL backend PID and wait until `pg_stat_activity` reports it waiting on a lock before committing connection A, proving the soft-delete was blocked per FR-020 and T047 (partial)
- [X] T049 Make the actor-isolation regression reuse a verified identical PostgreSQL backend session across transactions, using a one-connection pool or `pg_backend_pid()`, per FR-014, FR-015a, and T046 (partial)

---

## Phase 13: Convergence

- [X] T050 Observe connection B's lock wait through a dedicated third connection or observer pool and use a null-safe `pg_stat_activity` predicate so the T048 race proof cannot deadlock on pool exhaustion or fail before `wait_event_type` becomes `Lock` per FR-020 and T048 (partial)

---

## Phase 14: Convergence

- [X] T051 Align `data-model.md` and `contracts/database-schema.md` with migrations 0010–0015: audit `updated_at`, required `resource_id`, database-enforced slug auditing, and same-transaction `set_audit_actor()` usage per plan schema contract and T029–T031/T038–T046 (contradicts)
- [X] T052 Make `audit_logs_full_insert_accepted` repeatable on the shared append-only test database by querying a generated unique resource marker or the returned audit row ID per plan test-discipline decision (partial)
- [X] T053 Document migration review steps and explicit destructive-change handling in `backend/migrations/README.md` per FR-007 (partial)

---

## Phase 15: Convergence

- [X] T054 Correct `backend/migrations/README.md` review guidance to permit coordinated application/docs/tests changes and require existing trigger or constraint changes through a new fix-forward migration rather than prohibiting cross-file evolution per FR-006, FR-007, and T053 (contradicts)

---

## Phase 16: Convergence

- [X] T055 Correct `quickstart.md` scenario 5 so its CI sequence matches `.github/workflows/backend.yml`: apply migrations to the clean database before running workspace tests per FR-003 and T041 (contradicts)
