# ADR-004: Multi-tenancy — shared schema, Row-Level Security, composite foreign keys, tenant→cell mapping

- **Status:** Accepted (MOH-15)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §5](../architecture/system-architecture.md#5-multi-tenancy),
  constitution Principle I, ADR-001 (module data ownership), ADR-006 (tenant header validation
  and platform impersonation), ADR-007 (job payloads carry tenant context), ADR-013 (identifiers)

## Context

Every business on the platform is a tenant. Cross-tenant data exposure is the worst failure the
product can have (constitution Principle I): one leaked conversation or knowledge document ends
customer trust.

The decision has to balance these forces:

- **Isolation strength.** Application-only filtering fails the first time a developer forgets a
  `WHERE tenant_id = $1`. A second, independent layer is required.
- **Cost and operations.** Most tenants are small. A database or schema per tenant multiplies
  migrations, connections and backups by the tenant count.
- **Enterprise needs.** Large or regulated tenants will ask for dedicated infrastructure or data
  residency. That must be possible later without rewriting the application.
- **Performance.** Tenant filters run on every query. They must use indexes and not defeat
  connection pooling (PgBouncer in transaction mode).
- **Referential integrity.** A row in tenant A must never be able to point at a row in tenant B,
  even through a bug in application code.

## Decision

### 1. Pool model: shared database, shared schema, `tenant_id` on every tenant-owned row

Every tenant-owned table has `tenant_id uuid NOT NULL REFERENCES tenants(id)`. Every index on a
tenant table leads with `tenant_id`. Global tables (`users`, `tenants`, platform configuration)
are explicitly listed as non-tenant in the schema conventions.

### 2. Row-Level Security as the database layer of defence

- Every tenant-owned table has RLS **enabled and forced**:

  ```sql
  ALTER TABLE conversations ENABLE ROW LEVEL SECURITY;
  ALTER TABLE conversations FORCE ROW LEVEL SECURITY;
  CREATE POLICY tenant_isolation ON conversations
    USING      (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
  ```

- Each transaction sets the tenant **transaction-locally** before touching tenant data:
  `SELECT set_config('app.tenant_id', $1, true)` (equivalent to `SET LOCAL`). Transaction-local
  settings are safe with PgBouncer transaction pooling; session-level `SET` is forbidden.
- **Fail closed.** If `app.tenant_id` was never set on the connection, `current_setting(..., true)`
  returns `NULL`. Once any earlier transaction on that pooled connection has set it, it returns
  an **empty string** instead, and `''::uuid` would raise an error. `NULLIF(..., '')` maps both
  cases to `NULL`, the comparison is never true, and **no rows match**. A forgotten tenant context
  returns nothing and writes nothing.
- The Unit of Work (transaction wrapper) is the only place that sets `app.tenant_id`, and it takes
  a `TenantContext` that only the authorization layer can construct (ADR-006). Repositories
  accept `&TenantContext`, so forgetting the tenant is a compile error (application layer of
  defence).
- Platform-wide operations that legitimately span tenants (billing roll-ups, retention jobs,
  platform analytics) run through a **separate, audited code path** with a dedicated role or
  `SECURITY DEFINER` functions scoped to that one job — never by turning RLS off for the app.

### 3. Database roles: the app can't bypass RLS

| Role | Used by | Privileges |
| -- | -- | -- |
| `app_owner` | Migrations only (CI/CD step or `migrate` command) | Owns the schema and tables. `NOLOGIN` except in the migration job. |
| `app_runtime` | `api`, `gateway`, `worker`, `scheduler` | `SELECT/INSERT/UPDATE/DELETE` on tables, `USAGE` on sequences. **Not** the table owner, **no `BYPASSRLS`**, no DDL. |
| `app_platform` (later) | Cross-tenant platform jobs | Narrow grants or `SECURITY DEFINER` functions; every use audited. |

Because the app doesn't own the tables and RLS is `FORCE`d, policies apply to it even if a
migration forgets the owner exemption. The application stops running migrations at startup;
migrations run as `app_owner` in a separate step before rollout.

### 4. Composite foreign keys between tenant-owned tables

- Every tenant-owned table has `UNIQUE (tenant_id, id)`.
- Every reference from one tenant-owned table to another is composite:

  ```sql
  FOREIGN KEY (tenant_id, conversation_id)
    REFERENCES conversations (tenant_id, id)
  ```

- Single-column FKs are allowed only to **global** tables (`users`, `tenants`).
  This makes a cross-tenant reference impossible at the database level, independent of RLS.

### 5. Tenant → cell mapping from day one

- A `tenant_cells` table maps every tenant to a **cell** (a database plus its worker pool). All
  tenants map to `cell-1` today.
- Connection routing goes through a `CellRouter` port in the platform layer. With one cell it
  returns the single pool, so there is no runtime cost now.
- Moving a large or regulated tenant to a dedicated cell later is a data move plus a mapping change,
  not an application rewrite. Cross-cell queries are not supported; platform reporting reads from
  per-cell aggregates.

### 6. Everything outside PostgreSQL is tenant-namespaced

Redis keys `t:{tenant_id}:…`; object storage keys `tenants/{tenant_id}/…`; vector queries always
filtered by tenant (RLS applies to pgvector tables too); job and event payloads carry
`tenant_id` and the consumer re-establishes the tenant context before touching data; logs and
metrics carry `tenant_id` (constitution Principles I and VII).

### 7. Verification

- A shared **tenant-isolation test harness** creates two tenants and runs every repository and
  endpoint against the wrong tenant: read, list, update, delete, and guessed IDs.
- A CI schema check fails if a table with a `tenant_id` column lacks forced RLS and a policy, or
  if a tenant-to-tenant FK is single-column.
- A test asserts the runtime role has no `BYPASSRLS` and owns no tables.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Application-only filtering** (no RLS) | One forgotten `WHERE` leaks data. The constitution requires two independent layers. |
| **Schema per tenant** | Stronger isolation, but migrations and catalog size grow with the tenant count, connection pooling gets harder (`search_path` per tenant), and cross-tenant platform reporting needs dynamic SQL. |
| **Database per tenant** for everyone | Strongest isolation, highest cost and operational load. Kept as the **dedicated cell** option for enterprise tenants through the cell mapping. |
| **RLS with session-level `SET app.tenant_id`** | Breaks under PgBouncer transaction pooling: the setting leaks to whichever client gets the connection next. Transaction-local settings avoid this. |
| **RLS keyed on database roles per tenant** | One Postgres role per tenant doesn't scale (role count, connection pools per role) and complicates pooling. |
| **Single-column FKs plus RLS only** | RLS hides other tenants' rows from reads, but a write with a guessed foreign ID can still create a cross-tenant link if the check isn't tenant-aware. Composite FKs close that at the schema level. |
| **Add cell mapping when the first enterprise tenant arrives** | Retrofitting connection routing into every repository later is far more expensive than one table and one port now. |

## Consequences

**Positive**

- Two independent layers (typed `TenantContext` + forced RLS) plus schema-level integrity
  (composite FKs). A single bug no longer leaks data.
- A forgotten tenant context fails closed (no rows), not open.
- Cheapest operating model for many small tenants, with a clear path to dedicated cells.
- Separating migration and runtime roles also limits damage from SQL injection (no DDL, no RLS
  bypass).

**Negative and costs**

- Every tenant transaction pays a `set_config` call; RLS adds a predicate to every query (cheap
  with `tenant_id`-leading indexes, but plans must be checked).
- Composite keys widen indexes and FKs, and every tenant table needs `UNIQUE (tenant_id, id)`.
- Cross-tenant platform jobs need a deliberate, audited path, which is more work than a plain
  query.
- Retrofitting RLS onto the existing schema is a large, careful migration (see below).
- Debugging with `psql` needs the tenant setting, or a privileged role, which must itself be
  audited.

**Follow-ups**

- A corrective spec for RLS, roles and composite FKs (the constitution's COMPLIANCE_GAP TODO).

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| T1 | **No Row-Level Security.** No migration enables RLS or creates a policy, and no code sets `app.tenant_id`. Tenant isolation relies on application `WHERE tenant_id = …` filters only, which is one layer instead of the required two. **Progress (MOH-44):** `db::UnitOfWork` now sets `app.tenant_id` transaction-locally and the tenant-owned queries in the backend modules run through it (see [Tenant Unit of Work](../security/tenant-unit-of-work.md) for how, and for the queries that deliberately do not). No policy exists yet, so the gap stays open until the table groups in MOH-47 to MOH-49. | `backend/migrations/*`, `backend/crates/shared/db/src/lib.rs` | **High priority.** Corrective spec: Unit of Work that sets `app.tenant_id` transaction-locally, then enable and force RLS table by table (behind tests), starting with `conversations`, `messages`, `customers` and knowledge tables. |
| T2 | **The app's database role would bypass RLS.** In local dev the app connects as `customer_service`, which is the Postgres image's `POSTGRES_USER` and therefore a **superuser**. Superusers bypass RLS even when it's forced, so policies would never apply to the app as configured. The app also runs migrations at startup on the same connection, so the runtime role owns the tables and can run DDL. Staging reads `DATABASE_URL` from `.env.staging`, which isn't in the repo; it needs the same check. **Progress (MOH-23):** the local Postgres init script now creates `app_owner` and `app_runtime`, and `infra/postgres/verify-roles.sh` proves `app_runtime` cannot bypass a sample forced-RLS policy. The backend still connects as the superuser and still runs migrations at startup, so the gap stays open until MOH-31 switches it over. | `backend/.env.example`, `infra/docker-compose.yml`, `infra/postgres/init/02-roles.sh`, `infra/postgres/verify-roles.sh`, `backend/crates/server/src/main.rs` (`db::run_migrations`) | **High priority, and a prerequisite for T1.** Create `app_owner` / `app_runtime` roles (`app_runtime`: `NOSUPERUSER NOBYPASSRLS`, not the owner), move migrations to a pre-deploy step as `app_owner`, connect the app as `app_runtime`. Update `docs/deployment.md` and `docs/configuration.md`. |
| T3 | **Composite FKs are partial.** There are about 36 composite tenant FKs (e.g. migrations `0027`, `0033`) next to about 80 single-column `REFERENCES x(id)` FKs. Some of those point at tenant-owned tables, e.g. `REFERENCES conversations(id)` in `0048` and `0049`. | `backend/migrations/0025`–`0056` | Inventory single-column FKs; convert the tenant-to-tenant ones to composite with `UNIQUE (tenant_id, id)` on the targets. Add the CI schema check. |
| T4 | **No tenant→cell mapping.** There is no `tenant_cells` table and no routing port. | — | Add the table (all tenants → `cell-1`) and a `CellRouter` that returns the single pool. |
| T5 | Cross-tenant tests exist, but they're written case by case per feature at the endpoint level (e.g. `conversations.rs`, `customers.rs`, `engine_isolation.rs`). There's no shared harness that runs every repository and endpoint against the wrong tenant, so coverage depends on each author remembering. | `backend/crates/server/tests/` | Build the shared harness alongside T1 and require it for new modules. |
