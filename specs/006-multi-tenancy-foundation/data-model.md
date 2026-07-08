# Data Model: Multi-Tenancy Foundation

**Feature**: 006-multi-tenancy-foundation | **Date**: 2026-07-08
**Depends on**: feature 005 schema (`users`, `tenants`, `tenant_memberships`, `audit_logs`) — **this feature adds no tables, columns, or migrations.** The model below is runtime types + audit vocabulary + API DTOs.

## Runtime entities (backend)

### Principal (identity module)

The authenticated caller, attached as a request extension by the identity middleware.

| Field | Type | Source | Notes |
|-------|------|--------|-------|
| user_id | UUID | `users.id` | resolved from dev identity header (FR-019) until real auth |
| display_name | String | `users.display_name` | for `/me` |
| email | String | `users.email` | for `/me` |
| platform_role | Option\<PlatformRole\> | `users.platform_role` | `Some(_)` ⇔ platform user |

`PlatformRole` enum: `SuperAdmin | Developer | Sales | Support | Finance` (string-mapped to 005's CHECK values).

**Derived classification**: `PrincipalKind = Platform | Tenant` — `Platform` iff `platform_role.is_some()`.

**Lifecycle**: created per request by identity middleware; absent ⇒ endpoints requiring identity return 401. Never cached across requests (FR-009).

### TenantContext (tenancy module)

The validated, authorized working tenant, attached as a request extension by the tenant-context middleware — only present on tenant-scoped routes after authorization succeeded.

| Field | Type | Notes |
|-------|------|-------|
| tenant_id | UUID | the only tenant id handlers/data access may use (FR-003, FR-008) |
| tenant_status | TenantStatus (`Active \| Suspended`) | suspended only reachable by platform principals (FR-006/FR-007) |
| principal_kind | PrincipalKind | how access was granted (membership vs platform role) |

**State transitions**: none — stateless, rebuilt every request (clarification #2). Resolution pipeline: header present → UUID-parse → tenant exists & not deleted → principal authorized → context attached. Failures map per [contracts/http-api.md](contracts/http-api.md).

## Authorization rules (queries against 005 tables)

| Rule | Query shape | Index used |
|------|-------------|------------|
| Tenant exists (FR-002) | `tenants WHERE id = $1 AND deleted_at IS NULL` | PK |
| Tenant-user access (FR-005) | `tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL` (must exist) AND tenant status = `active` (FR-007) | `tenant_memberships_tenant_user_active_uniq` |
| Platform-user access (FR-006) | tenant exists (any status) AND `principal.platform_role IS NOT NULL` | PK |
| Principal resolution (FR-019) | `users WHERE id = $1 AND deleted_at IS NULL` | PK |

No new indexes required — every lookup above is already index-served by 005's schema.

## Audit vocabulary (rows in 005's `audit_logs`)

| action | actor_user_id | tenant_id | resource_type / resource_id | details | When |
|--------|--------------|-----------|------------------------------|---------|------|
| `platform.tenant_switched` | platform user | target tenant | `tenant` / target tenant id | `{ "tenant_slug": "<slug>" }` | switch action succeeds (FR-012); written synchronously |
| `tenant.access_denied` | principal if known, else NULL | **NULL** (requested tenant may not exist; FK-safe) | `tenant` / NULL | `{ "requested_tenant_id": "<raw header value>", "reason": "no_membership" \| "suspended" \| "not_found" }` | FR-005/FR-007 denials (FR-013); insert failure logged, 403 unaffected |

Missing/malformed-header rejections (400s) are not audited (no probe signal; trace-visible only).

## API DTOs (shared shape backend ⇄ frontend)

### MeResponse — `GET /api/v1/me`

```text
MeResponse {
  id: UUID, email: string, displayName: string,
  platformRole: 'super_admin'|'developer'|'sales'|'support'|'finance'|null,
  memberships: MembershipSummary[]           // active only
}
MembershipSummary { tenantId: UUID, tenantName: string, tenantSlug: string, role: 'owner'|'admin'|'manager'|'agent'|'viewer' }
```

### TenantSummary — directory rows, switch response, `GET /api/v1/tenant`

```text
TenantSummary { id: UUID, name: string, slug: string, status: 'active'|'suspended' }
```

`GET /api/v1/platform/tenants` returns `Page<TenantSummary>` (kernel cursor pagination; optional `q` name/slug filter).

## Frontend state (dashboard)

### `tenantContext` NgRx feature slice (core/state)

| Field | Type | Notes |
|-------|------|-------|
| activeTenant | TenantSummary \| null | single source of truth for `X-Tenant-ID` (FR-014) |
| status | 'idle' \| 'switching' \| 'error' | switch-in-flight / forbidden feedback (FR-017) |

**Persistence**: effect mirrors `activeTenant` to localStorage key `app.tenant` for platform users; rehydrates at init; a rehydrated tenant that fails validation (switch/`/me` check) is discarded → `activeTenant = null` (FR-016).

**Population rules**: tenant principal → auto-set from sole/primary membership in `/me` (FR-015); platform principal → set only via switcher action (which calls the switch endpoint first — FR-012).

### CurrentUser signal (core/tenant/current-user.service.ts)

Holds `MeResponse | null` fetched once at shell bootstrap; `isPlatformUser` computed signal drives switcher visibility (FR-015) and the platform/tenant area guard.

## Traceability

| Requirement | Model element |
|-------------|---------------|
| FR-001–FR-004 | TenantContext resolution pipeline |
| FR-005–FR-009 | Authorization rules table (no caching; per-request rebuild) |
| FR-010–FR-012 | TenantSummary directory + switch action + `platform.tenant_switched` |
| FR-013 | `tenant.access_denied` audit row (NULL tenant_id, details payload) |
| FR-014–FR-017 | `tenantContext` slice, interceptor source-of-truth, persistence + discard rules |
| FR-019 | Principal resolution rule (env-gated dev header) |
