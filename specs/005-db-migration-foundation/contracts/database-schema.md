# Contract: Database Schema & Migration Workflow

**Feature**: 005-db-migration-foundation
**Consumers**: every backend module crate (identity, tenancy, audit, rbac, …), CI, developer tooling.

This feature exposes no HTTP API. Its public interfaces are (A) the migration workflow contract that all future schema changes must follow, and (B) the schema contract that application code may rely on. Column-level detail lives in [data-model.md](../data-model.md); this file states the *guarantees*.

## A. Migration workflow contract

| Guarantee | Contract |
|-----------|----------|
| Location | All migrations live in `backend/migrations/`, named `NNNN_description.sql`, applied in ascending order. |
| Single command | `sqlx migrate run` (from `backend/`, with `DATABASE_URL` set) brings any database — including an empty one — to the current schema. Programmatic equivalent: `db::run_migrations(&pool)`. |
| Exactly-once | SQLx records applied migrations in `_sqlx_migrations`; re-running is a no-op and exits 0. |
| Immutability | SQLx checksums applied migrations; editing an applied file causes subsequent runs to fail with a version-mismatch error. Fix = new migration, never an edit. |
| Atomicity | Each `.sql` file runs inside a transaction (Postgres). A failing migration leaves the database at the previous version. |
| CI gate | `.github/workflows/backend.yml` applies the full history to a clean Postgres service on every push/PR; a failing migration fails the build. |
| Reversibility policy | Fix-forward only. No down migrations. |
| Workflow docs | `backend/migrations/README.md` documents how to add, name, review, and (not) modify migrations. |

## B. Schema contract (what application code may rely on)

### Invariants — all tables in this feature

- Primary key `id UUID`, database-generated; inserts MUST NOT supply `id` (and MUST NOT rely on any ordering property of it).
- `created_at`/`updated_at` are database-maintained; application code MUST NOT write them.
- Soft delete: a row is active iff `deleted_at IS NULL`. Application queries for "current" data MUST filter `deleted_at IS NULL`. Physical `DELETE` on `users`, `tenants`, `tenant_memberships` is a contract violation (and will be blocked by RESTRICT FKs where children exist).

### users

- Lookup by email: `WHERE email = $1 AND deleted_at IS NULL` — index-served, case-insensitive (CITEXT), at most one row.
- A soft-deleted user's email MAY be reused by a new active user.
- `platform_role IS NOT NULL` ⇔ the user is platform staff; values: `super_admin|developer|sales|support|finance`.

### tenants

- Lookup by slug: `WHERE slug = $1 AND deleted_at IS NULL` — index-served, case-insensitive, at most one row.
- `status ∈ {active, suspended}`; suspended ≠ deleted. Termination = soft delete.
- Slug is mutable; the database **automatically** writes an `audit_logs` row with `action = 'tenant.slug_changed'` and the old/new slugs in `details` (migrations 0009+0012+0015). The application must call `set_audit_actor(<user_id>)` **within the same explicit transaction** as the slug UPDATE; the UPDATE is rejected otherwise.

### tenant_memberships

- At most one **active** membership per `(tenant_id, user_id)`; inserting a duplicate active pair fails with a unique violation.
- `role ∈ {owner, admin, manager, agent, viewer}` — exactly one per membership.
- Soft-deleting a tenant or a user automatically soft-deletes its active memberships (database trigger — migrations 0005+0008). Application code MUST NOT assume memberships outlive their parents.
- An active membership cannot reference a soft-deleted parent (inserts, reactivation, and reparenting updates are all rejected by the `reject_membership_with_deleted_parent` trigger from migration 0009+0011).
- The guard uses `SELECT ... FOR UPDATE` on the parent user and tenant rows, so a concurrent parent soft-delete blocks the membership write until the membership transaction commits.
- Tenant-scoped listing (`WHERE tenant_id = $1 AND deleted_at IS NULL`) and user-scoped listing (`WHERE user_id = $1`) are index-served.

### audit_logs

- INSERT-only. `UPDATE`/`DELETE` raise a database exception — do not build any code path that attempts them.
- Required on insert: `action`, `resource_type`, and `resource_id` (NOT NULL since migration 0013 — the `audit_logs_resource_required` CHECK requires every entry to identify the affected resource).
- Optional: `actor_user_id` (NULL ⇔ system actor), `tenant_id` (NULL ⇔ platform-level action), `details` (defaults to `{}`).
- Tenant timeline (`WHERE tenant_id = $1 ORDER BY created_at DESC`) is index-served.
- Entries remain readable after their actor/tenant is soft-deleted.
- `updated_at` column is present (migration 0010) but ineffective because the append-only `forbid_mutation()` trigger prevents all updates.

## C. Compatibility promises to future features

- New tenant-owned tables MUST follow the conventions table in data-model.md (UUID PK, timestamps + trigger, `tenant_id UUID NOT NULL REFERENCES tenants(id)`, `tenant_id` index, soft delete where the entity is a business record).
- Auth features build on `users` without modifying its contract (credentials live in their own table).
- Runtime tenant-isolation enforcement (automatic query scoping) is a later feature; this schema guarantees it is *possible* (tenant_id + indexes), not that it is *applied*.
