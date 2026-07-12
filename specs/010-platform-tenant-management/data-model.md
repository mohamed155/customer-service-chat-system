# Data Model: Platform Tenant Management

**Feature**: 010-platform-tenant-management | **Date**: 2026-07-11

One schema change (migration `0016_tenant_business_metadata.sql`); everything else is code-level. Canonical wire shapes live in [contracts/rest-api.md](./contracts/rest-api.md).

## Tenant (extended — `tenants` table)

| Column | Type | Constraint | Change |
|--------|------|-----------|--------|
| id | UUID | PK | unchanged |
| name | TEXT | NOT NULL, length 1–200 (CHECK) | unchanged |
| slug | CITEXT | NOT NULL, format `^[a-z0-9](-?[a-z0-9])*$`, ≤63 (CHECK), unique among live rows (partial index) | unchanged |
| status | TEXT | NOT NULL DEFAULT 'active', CHECK ∈ {active, suspended} | unchanged |
| **plan** | TEXT | NOT NULL DEFAULT 'trial', CHECK ∈ {trial, starter, professional, enterprise} | **NEW** |
| **contact_name** | TEXT | NULL, CHECK (NULL or length 1–200) | **NEW** |
| **contact_email** | TEXT | NULL — format validated app-side (research R1) | **NEW** |
| created_at / updated_at / deleted_at | TIMESTAMPTZ | existing conventions | unchanged |

- Existing rows adopt `plan='trial'` via the column default; contact fields start NULL.
- The `tenants_slug_change_audit` trigger (0015) remains in force: any slug UPDATE requires `set_audit_actor()` in the same transaction and emits a `tenant.slug_changed` audit row itself.

## Status lifecycle

```
active  ── PATCH status='suspended' (platform.tenants.manage) ──▶  suspended
suspended ── PATCH status='active'  (platform.tenants.manage) ──▶  active
```

- Only these two states; transitions are explicit administrator actions, each audited (`platform.tenant_status_changed`).
- Enforcement consequence (existing, unchanged): tenant-context middleware refuses tenant principals of suspended tenants per request; platform staff retain visibility/switch.

## Plan (code-level vocabulary)

`trial | starter | professional | enterprise` — display/reporting metadata only; grants or restricts nothing (spec assumption). Backend: serde-validated enum or CHECK-backed string; frontend: `TenantPlan` string-literal union in `core/api/tenant-api.models.ts`.

## Wire models (frontend `core/api/tenant-api.models.ts`)

| Type | Fields | Notes |
|------|--------|-------|
| `TenantSummary` (extended) | id, name, slug, status, **plan** | additive — switcher/tenant-select unaffected |
| `PlatformTenantDetail` (new) | id, name, slug, status, plan, contactName (nullable), contactEmail (nullable), createdAt, updatedAt | detail page + create/update responses |
| `CreateTenantPayload` (new) | name, slug, plan?, contactName?, contactEmail? | plan defaults server-side to trial |
| `UpdateTenantPayload` (new) | name?, slug?, plan?, contactName?, contactEmail?, status? | partial; only provided fields applied |

## Permission (authz catalog delta)

| Code | Variant | Granted to |
|------|---------|-----------|
| `platform.tenants.manage` | `Permission::PlatformTenantsManage` | Super Admin, Support Engineer |

Catalog count 25→26; frontend `Permission` union extended; 008 parity test list updated. Full matrix delta in [contracts/permissions.md](./contracts/permissions.md).

## Audit records (append-only `audit_logs`, existing shape)

| Action | Emitted by | Details payload |
|--------|-----------|-----------------|
| `platform.tenant_created` | app (`audit::record`) | name, slug, plan |
| `platform.tenant_updated` | app | changed fields with old/new values (slug excluded — see below) |
| `platform.tenant_status_changed` | app | old_status, new_status |
| `tenant.slug_changed` | DB trigger (existing) | old_slug, new_slug; actor from `set_audit_actor()` |

## Feature state (frontend `tenants.store.ts` SignalStore)

| Field | Type | Notes |
|-------|------|-------|
| items | `TenantSummary[]` | accumulated pages |
| query | string | debounced search input |
| statusFilter | `'active' \| 'suspended' \| null` | null = all |
| nextCursor / hasMore | string \| null / boolean | from `Page` envelope |
| loading / error | request state | drives shared loading/empty states |

- Transitions: query or filter change → reset items+cursor, reload (`switchMap`); load-more → append (`concatMap`-safe); create/update success → invalidate/refresh affected entries.
