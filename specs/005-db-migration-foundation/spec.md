# Feature Specification: Database & Migration Foundation

**Feature Branch**: `005-db-migration-foundation`

**Created**: 2026-07-07

**Status**: Draft

**Input**: User description: "Database & Migration Foundation — Set up safe database schema evolution. Scope: SQLx migrations, migration workflow, base tables, UUID primary keys, timestamps, soft delete strategy where needed, indexing rules. Create initial tables for: platform users, organizations/tenants, tenant memberships, audit logs. Acceptance: migrations run locally and in CI, schema reproducible from scratch, all tenant-owned tables include tenant_id, basic indexes exist for tenant-aware queries."

## Clarifications

### Session 2026-07-07

- Q: How much detail should each audit log entry capture about the change it records? → A: Actor, action, resource, time, tenant context, plus a flexible structured details payload (changed fields / before-after values where relevant).
- Q: Which lifecycle states should a tenant support at this foundation stage? → A: Active and Suspended; termination is represented by soft delete, richer onboarding/billing states deferred to later features.
- Q: When a user is soft-deleted, what happens to their tenant memberships? → A: All of the user's memberships are soft-deleted with them, mirroring the tenant-deletion cascade rule.
- Q: Can a tenant's unique lookup handle (slug) be changed after creation? → A: Yes — renamable, with uniqueness re-checked against active tenants; the change is an audited sensitive action.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reproducible Schema via Migrations (Priority: P1)

A developer joining the project (or a CI pipeline provisioning a throwaway environment) starts from a completely empty database and, by running a single documented command, ends up with the complete, current database schema. Every schema change ever made is captured as an ordered, versioned migration in the repository — nothing is ever applied by hand.

**Why this priority**: This is the foundation everything else depends on. Without a reliable, repeatable way to build and evolve the schema, no table designed in this feature (or any future feature) can be trusted to exist consistently across developer machines, CI, and future deployment environments. The project constitution (Principle VIII) mandates that schema changes never bypass migrations.

**Independent Test**: Can be fully tested by pointing the migration command at a freshly created empty database and verifying that (a) it completes without errors, (b) running it a second time is a no-op, and (c) the resulting schema matches what the migrations describe. Delivers value even before any tables exist beyond the tracking table itself.

**Acceptance Scenarios**:

1. **Given** an empty database, **When** a developer runs the documented migration command locally, **Then** all migrations apply in order and the command reports success.
2. **Given** a database where all migrations have already been applied, **When** the migration command runs again, **Then** no changes are made and the command still exits successfully (idempotent re-run).
3. **Given** a pull request that adds a new migration, **When** CI runs, **Then** the pipeline provisions a clean database, applies every migration from scratch, and fails the build if any migration errors.
4. **Given** a migration file that has already been applied to an environment, **When** its content is modified after the fact, **Then** the tooling detects the mismatch and refuses to proceed, rather than silently diverging.

---

### User Story 2 - Foundational Identity & Tenancy Tables (Priority: P2)

The platform team needs the core tables that every future feature builds on: the people who use the platform (platform staff and tenant users), the customer organizations (tenants), the link between users and the tenants they belong to (memberships with a role), and an audit trail of sensitive actions. After migrations run, these tables exist with correct relationships and constraints.

**Why this priority**: These four tables are the minimum data model required before any authentication, tenant management, or auditing feature can be implemented. They depend on User Story 1 (the migration mechanism) being in place.

**Independent Test**: Can be tested by applying migrations to a clean database and then inserting representative rows: a platform user, a tenant, a membership linking a user to that tenant with a role, and an audit log entry — verifying constraints (uniqueness, required fields, referential integrity) accept valid data and reject invalid data.

**Acceptance Scenarios**:

1. **Given** a migrated database, **When** a user record is created with an email already used by another active user, **Then** the database rejects it (email uniqueness among active users).
2. **Given** a migrated database, **When** a membership is created linking a user to a tenant, **Then** it must reference an existing user and an existing tenant, carry exactly one role, and a second membership for the same user-tenant pair is rejected.
3. **Given** a migrated database, **When** an audit log entry is recorded, **Then** it captures who acted, what action was taken, what was affected, when it happened, and the tenant context where applicable.
4. **Given** a tenant-owned table (memberships, tenant-scoped audit entries), **When** its structure is inspected, **Then** it includes a `tenant_id` column that cannot be null.

---

### User Story 3 - Safe Data Lifecycle Conventions (Priority: P3)

The platform team establishes and applies the data conventions that keep the schema safe to evolve and query at scale: globally unique identifiers as primary keys, creation/modification timestamps on every table, recoverable ("soft") deletion for business records that may need restoration or must be retained for accountability, and indexes covering tenant-aware and common lookup queries.

**Why this priority**: These conventions matter most as the schema grows, but they must be baked into the very first tables — retrofitting identifier strategy, timestamps, or delete semantics later is disruptive. They refine the tables from User Story 2 rather than standing alone.

**Independent Test**: Can be tested by inspecting the migrated schema: every table exposes a UUID primary key and created/updated timestamps; user, tenant, and membership tables support soft deletion; audit logs are append-only; tenant-scoped lookup columns are indexed.

**Acceptance Scenarios**:

1. **Given** any base table, **When** a row is created, **Then** it receives a UUID primary key and a creation timestamp automatically, without the caller supplying them.
2. **Given** an existing row, **When** it is updated, **Then** its last-modified timestamp reflects the update.
3. **Given** a user, tenant, or membership record, **When** it is deleted through the application's conventions, **Then** the row is marked deleted (with a deletion timestamp) rather than physically removed, and it no longer conflicts with uniqueness rules for active records (e.g., a new user can register with a soft-deleted user's email).
4. **Given** the audit log table, **When** its conventions are inspected, **Then** entries are append-only: no update or delete pathway is part of the design.
5. **Given** a query that filters a tenant-owned table by tenant, **When** its execution is analyzed, **Then** it is served by an index rather than a full table scan.

---

### Edge Cases

- What happens when a migration fails partway through? Each migration must apply atomically — a failed migration leaves the database in its prior state, not half-migrated.
- What happens when two developers create migrations concurrently on different branches? The workflow must define how ordering conflicts are detected and resolved before merge (CI applying all migrations from scratch catches collisions).
- How does the system handle a soft-deleted tenant that still has memberships? Deleting a tenant must define the fate of its memberships (they are soft-deleted along with the tenant) so no active membership points at a deleted tenant.
- How does the system handle a soft-deleted user that still has memberships? Soft-deleting a user also soft-deletes all of that user's memberships, so no active membership points at a deleted user.
- How are audit log entries handled when the user or tenant they reference is later soft-deleted? Audit entries are immutable history — they must remain valid and readable even when their subjects are deleted.
- What happens if someone attempts to change the schema manually outside a migration? The workflow must treat migration files as the single source of truth; drift detection (schema rebuilt from scratch in CI) surfaces manual changes as failures.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: All database schema changes MUST be expressed as ordered, versioned migration files stored in version control; manual schema changes against any environment are prohibited.
- **FR-002**: A developer MUST be able to apply all pending migrations to a local database with a single documented command.
- **FR-003**: CI MUST apply the full migration history to a clean database on every change and fail the build if any migration errors.
- **FR-004**: The complete current schema MUST be reproducible from scratch by running the migration history against an empty database, with no additional manual steps.
- **FR-005**: The system MUST track which migrations have been applied so each migration runs exactly once per database, in a deterministic order, and re-running the command on an up-to-date database is a safe no-op.
- **FR-006**: Applied migrations MUST be immutable: modifying an already-applied migration file MUST be detected and rejected rather than silently accepted.
- **FR-007**: The migration workflow (how to create, name, review, and apply a migration, and how destructive changes are handled) MUST be documented in the repository.
- **FR-008**: The initial migrations MUST create tables for platform users, tenants (organizations), tenant memberships, and audit logs.
- **FR-009**: Every base table MUST use a UUID as its primary key, generated without caller involvement.
- **FR-010**: Every base table MUST record when each row was created and when it was last modified, populated automatically.
- **FR-011**: User, tenant, and membership records MUST support soft deletion (a deletion marker with timestamp) instead of physical removal; soft-deleted rows MUST be excluded from active-record uniqueness rules.
- **FR-012**: Audit log entries MUST be append-only: the design provides no update or delete pathway for recorded entries.
- **FR-013**: Every tenant-owned table MUST include a non-nullable `tenant_id` column referencing the tenants table.
- **FR-014**: Audit log entries MUST capture the actor, the action performed, the affected resource, the time of the action, the tenant context when the action is tenant-scoped, and a flexible structured details payload describing the change (e.g., changed fields with before/after values where relevant); platform-level actions (no tenant context) MUST also be recordable.
- **FR-015**: A user's email MUST be unique among active (non-deleted) users; a tenant's identifying handle (slug/name used for lookup) MUST be unique among active tenants; a user MUST NOT hold more than one active membership in the same tenant.
- **FR-015a**: A tenant's handle MUST be changeable after creation, with uniqueness re-validated against active tenants at change time; a handle change is a sensitive action that MUST be recorded in the audit log.
- **FR-016**: A membership MUST reference an existing user and an existing tenant and carry exactly one role from the tenant role set (Owner, Admin, Manager, Agent, Viewer); platform users carry a role from the platform role set (Super Admin, Developer, Sales, Support, Finance).
- **FR-017**: Indexes MUST exist for tenant-aware access paths (at minimum `tenant_id` on every tenant-owned table) and for the common lookups defined by this feature (user by email, tenant by handle, membership by user, audit entries by tenant and time).
- **FR-018**: Each individual migration MUST apply atomically: a failure leaves the database in the state it was in before that migration started.
- **FR-019**: A tenant MUST carry exactly one status, either Active or Suspended; termination is represented by soft delete, not a status value.
- **FR-020**: Soft-deleting a tenant MUST soft-delete all of its memberships, and soft-deleting a user MUST soft-delete all of that user's memberships — no active membership may reference a soft-deleted tenant or user.

### Key Entities

- **User (platform user)**: A person with an account on the platform — either platform staff (Super Admin, Developer, Sales, Support, Finance) or a person who participates in tenants. Key attributes: unique email among active users, display name, optional platform role for staff, lifecycle timestamps, soft-delete marker.
- **Tenant (organization)**: A customer organization that owns its own isolated data. Key attributes: display name, unique lookup handle among active tenants, status (Active or Suspended — termination is represented by soft delete), lifecycle timestamps, soft-delete marker. Everything a tenant owns elsewhere in the system points back to it via `tenant_id`.
- **Tenant Membership**: The link between a user and a tenant, carrying exactly one tenant role (Owner, Admin, Manager, Agent, Viewer). Tenant-owned (includes `tenant_id`). At most one active membership per user per tenant. Soft-deletable.
- **Audit Log Entry**: An immutable record of a sensitive action: who (actor), what (action and affected resource), when, in which tenant context (nullable for platform-level actions), and a structured details payload describing the change (changed fields / before-after values where relevant). Append-only; never updated or deleted.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer with an empty database reaches the full current schema with one documented command in under 5 minutes, with zero manual schema steps.
- **SC-002**: 100% of schema changes from this feature onward exist as versioned migration files in the repository — zero undocumented schema drift between environments built from the same migration history.
- **SC-003**: Every change that includes a broken migration is caught by CI before merge — the pipeline rebuilds the schema from scratch on every change.
- **SC-004**: 100% of tenant-owned tables include a non-nullable `tenant_id`, verifiable by schema inspection.
- **SC-005**: 100% of tenant-filtered lookups on the base tables are served by an index (verifiable via query-plan inspection), with no full-table scans on tenant-scoped access paths.
- **SC-006**: A representative sensitive action can be traced end-to-end from the audit log alone: who did it, what was affected, when, and in which tenant.

## Assumptions

- The four base tables (users, tenants, memberships, audit logs) are the complete scope; domain tables (conversations, messages, AI configuration, etc.) belong to later features.
- A single users table serves both platform staff and tenant participants, distinguished by an optional platform role and by tenant memberships — this matches the constitution's Platform User / Tenant User distinction while avoiding duplicate identity records.
- Soft deletion applies to users, tenants, and memberships (business records that may need restoration or retention); audit logs are append-only and never deleted; permanent purge policies (e.g., for legal compliance) are out of scope for this feature.
- Deleting (soft-deleting) a tenant also soft-deletes its memberships, and deleting a user also soft-deletes that user's memberships — no active membership references a deleted tenant or a deleted user.
- Authentication, sessions, password storage, and login flows are out of scope — this feature provides only the identity and tenancy data foundation they will build on.
- Runtime enforcement of tenant isolation in the data-access layer (automatic tenant filtering of queries) is a follow-on concern; this feature guarantees the schema supports it (`tenant_id` columns and indexes).
- No production data exists yet, so no data backfill or migration-of-existing-data concerns apply; rollback strategy for this feature is "fix forward" via new migrations.
- CI infrastructure capable of provisioning a disposable database (from spec 004's backend core infrastructure work) is available or will be provided as part of this feature's workflow setup.
