# Data Model: Database & Migration Foundation

**Feature**: 005-db-migration-foundation | **Date**: 2026-07-07
**Depends on**: [research.md](research.md) decisions R3–R10

## Conventions (apply to every table below unless noted)

| Convention | Mechanism |
|------------|-----------|
| Primary key | `id UUID PRIMARY KEY DEFAULT gen_random_uuid()` (R3) |
| Created / updated | `created_at`/`updated_at TIMESTAMPTZ NOT NULL DEFAULT now()`; `BEFORE UPDATE` trigger `set_updated_at()` (R4) |
| Soft delete | `deleted_at TIMESTAMPTZ NULL`; row is **active** iff `deleted_at IS NULL` (R5) |
| Active-row uniqueness | Partial unique index `WHERE deleted_at IS NULL` (R5) |
| Tenant ownership | Non-nullable `tenant_id UUID REFERENCES tenants(id)` (constitution II) |
| FK delete behavior | `ON DELETE RESTRICT` — hard deletes are design errors (R9) |

`audit_logs` is exempt from `updated_at` and `deleted_at` (append-only, R8).

## Entity: users (migration 0003)

All human accounts — platform staff and tenant participants (spec assumption: single identity table).

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| email | CITEXT | NOT NULL; CHECK `position('@' in email) > 1`; unique among active rows |
| display_name | TEXT | NOT NULL; CHECK `length(display_name) BETWEEN 1 AND 200` |
| platform_role | TEXT | NULL; CHECK in (`super_admin`,`developer`,`sales`,`support`,`finance`); NULL = not platform staff |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL, default `now()`; trigger-maintained |
| deleted_at | TIMESTAMPTZ | NULL |

**Indexes**
- `users_email_active_uniq` — UNIQUE `(email) WHERE deleted_at IS NULL` (FR-015; doubles as email-lookup index)

**Triggers**
- `set_updated_at` BEFORE UPDATE
- `users_soft_delete_cascade` AFTER UPDATE: when `deleted_at` transitions NULL → NOT NULL, stamp the same `deleted_at` onto this user's active `tenant_memberships` (FR-020)

**State transitions**: active → soft-deleted (set `deleted_at`). Restoration (clearing `deleted_at`) is permitted by the schema but only if it doesn't violate active-email uniqueness; restoring memberships is a separate explicit act (cascade is one-way).

**Note**: No credential columns — authentication is explicitly out of scope (spec Assumptions); a later auth feature adds its own table(s).

## Entity: tenants (migration 0004)

Customer organizations; the root of all tenant-owned data.

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| name | TEXT | NOT NULL; CHECK `length(name) BETWEEN 1 AND 200` |
| slug | CITEXT | NOT NULL; CHECK `slug ~ '^[a-z0-9](-?[a-z0-9])*$' AND length(slug) <= 63`; unique among active rows; **renamable** (spec clarification — uniqueness re-validated by the index on UPDATE, change must be audited by callers per FR-015a) |
| status | TEXT | NOT NULL DEFAULT `'active'`; CHECK in (`active`,`suspended`) (FR-019) |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL, default `now()`; trigger-maintained |
| deleted_at | TIMESTAMPTZ | NULL |

**Indexes**
- `tenants_slug_active_uniq` — UNIQUE `(slug) WHERE deleted_at IS NULL` (FR-015; doubles as slug-lookup index)

**Triggers**
- `set_updated_at` BEFORE UPDATE
- `tenants_soft_delete_cascade` AFTER UPDATE: when `deleted_at` transitions NULL → NOT NULL, stamp the same `deleted_at` onto this tenant's active `tenant_memberships` (FR-020)

**State transitions**: `status`: active ⇄ suspended. Lifecycle: active → soft-deleted (termination is soft delete, not a status — spec clarification).

## Entity: tenant_memberships (migration 0005) — tenant-owned

The user↔tenant link carrying exactly one tenant role.

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| tenant_id | UUID | NOT NULL, FK → `tenants(id)` (FR-013) |
| user_id | UUID | NOT NULL, FK → `users(id)` (FR-016) |
| role | TEXT | NOT NULL; CHECK in (`owner`,`admin`,`manager`,`agent`,`viewer`) (FR-016) |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL, default `now()`; trigger-maintained |
| deleted_at | TIMESTAMPTZ | NULL |

**Indexes**
- `tenant_memberships_tenant_user_active_uniq` — UNIQUE `(tenant_id, user_id) WHERE deleted_at IS NULL` (FR-015: one active membership per user per tenant; leading `tenant_id` also serves tenant-scoped listing, FR-017)
- `tenant_memberships_user_idx` — `(user_id)` (memberships-by-user lookup, FR-017)

**Triggers**
- `set_updated_at` BEFORE UPDATE

**Relationships**: N:1 → tenants; N:1 → users. Soft-deleted by its own flag, by tenant cascade, or by user cascade.

## Entity: audit_logs (migration 0006) — append-only, tenant-aware

Immutable record of sensitive actions (FR-012, FR-014).

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| actor_user_id | UUID | NULL, FK → `users(id)`; NULL = system/automated actor |
| action | TEXT | NOT NULL; CHECK `length(action) BETWEEN 1 AND 100` (e.g. `tenant.slug_changed`, `membership.role_changed`) |
| resource_type | TEXT | NOT NULL |
| resource_id | TEXT | NULL (TEXT, not UUID — may reference non-UUID resources, R8) |
| tenant_id | UUID | NULL, FK → `tenants(id)`; NULL = platform-level action (FR-014) |
| details | JSONB | NOT NULL DEFAULT `'{}'` — structured change payload (changed fields / before-after), spec clarification |
| created_at | TIMESTAMPTZ | NOT NULL DEFAULT `now()` |

No `updated_at`, no `deleted_at`.

**Indexes**
- `audit_logs_tenant_created_idx` — `(tenant_id, created_at DESC)` (tenant timeline, FR-017)
- `audit_logs_created_idx` — `(created_at DESC)` (platform-wide review)

**Triggers**
- `audit_logs_append_only` BEFORE UPDATE OR DELETE: `RAISE EXCEPTION` — immutability enforced in the database (FR-012)

**Relationships**: optional N:1 → users (actor), optional N:1 → tenants. FKs remain valid under soft delete because parent rows are never physically removed.

## Trigger functions (migration 0005 & 0006 introduce; 0001 already has set_updated_at)

| Function | Purpose |
|----------|---------|
| `set_updated_at()` | existing (0001) — maintains `updated_at` |
| `cascade_soft_delete_memberships()` | shared by the users/tenants AFTER UPDATE triggers: on NULL→NOT NULL `deleted_at` transition, `UPDATE tenant_memberships SET deleted_at = NEW.deleted_at WHERE (tenant_id/user_id) = NEW.id AND deleted_at IS NULL` (parameterized by trigger argument or two thin functions) |
| `forbid_mutation()` | raises exception; attached to `audit_logs` for UPDATE/DELETE |

## Relationship diagram

```text
users 1 ──── N tenant_memberships N ──── 1 tenants
  │                                          │
  └──(actor_user_id, nullable)               │
        N                                    │
     audit_logs N ──(tenant_id, nullable)────┘
```

## Validation rules traceability

| Spec requirement | Enforced by |
|------------------|-------------|
| FR-009 UUID PKs, no caller involvement | `DEFAULT gen_random_uuid()` on every PK |
| FR-010 automatic timestamps | column defaults + `set_updated_at` triggers |
| FR-011 soft delete + active uniqueness | `deleted_at` + partial unique indexes |
| FR-012 append-only audit | `forbid_mutation` trigger |
| FR-013 tenant_id on tenant-owned tables | `tenant_memberships.tenant_id NOT NULL` FK |
| FR-014 audit fields + details payload | audit_logs columns incl. `details JSONB` |
| FR-015 active uniqueness (email, slug, membership) | three partial unique indexes |
| FR-015a renamable slug | UPDATE allowed; partial unique index re-validates |
| FR-016 FK integrity + single role | FKs + `role` CHECK; platform roles via `users.platform_role` CHECK |
| FR-017 tenant-aware indexes | index set above |
| FR-019 tenant status | `status` CHECK + default |
| FR-020 soft-delete cascades | `cascade_soft_delete_memberships` triggers |
