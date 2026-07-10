# Data Model: RBAC & Permissions

**Feature**: 008-rbac-permissions | **Date**: 2026-07-10

No database schema changes. Roles already persist in `users.platform_role` (CHECK: `super_admin|developer|sales|support|finance`, nullable) and `tenant_memberships.role` (CHECK: `owner|admin|manager|agent|viewer`). All entities below are code-level (backend `authz` crate / frontend `core/authz`), serialized only through `GET /me`.

## Entities

### Permission (backend enum / frontend string-literal union)

The atomic capability. Serialized as a dot-scoped snake_case code (see `contracts/permissions.md` for the canonical catalog).

| Field | Type | Notes |
|-------|------|-------|
| code | string | e.g. `conversations.manage`; stable public identifier, used in `/me`, route declarations, and frontend checks |

- Validation: backend `FromStr` rejects unknown codes; frontend type union rejects unknown codes at compile time.
- Two scopes, distinguished by prefix: tenant permissions (no prefix) and platform permissions (`platform.` prefix). A permission belongs to exactly one scope.

### TenantRole (backend enum — NEW; DB values already exist)

| Variant | DB value | Display name |
|---------|----------|--------------|
| Owner | `owner` | Owner |
| Admin | `admin` | Admin |
| Manager | `manager` | Manager |
| Agent | `agent` | Support Agent |
| Viewer | `viewer` | Viewer |

- `Display`/`FromStr` round-trip exactly like the existing `PlatformRole`; unknown stored values parse to an error → treated as *no role* (deny + `error` log).

### PlatformRole (existing enum — unchanged)

`SuperAdmin (super_admin)`, `Developer (developer)`, `Sales (sales)`, `Support (support` — display "Support Engineer"`)`, `Finance (finance)`. Already defined in `identity`; `authz` consumes it.

### Role–Permission Mapping (backend static matrix)

Three total functions in `authz::matrix` (exhaustive `match` — the compiler guarantees every role is mapped):

| Function | Input | Output |
|----------|-------|--------|
| `tenant_role_permissions` | `TenantRole` | `&'static [Permission]` — the role's tenant-scope set |
| `platform_role_permissions` | `PlatformRole` | `&'static [Permission]` — the role's platform-scope set |
| `staff_tenant_permissions` | `PlatformRole`, `is_production: bool` | `&'static [Permission]` — tenant-scope set granted to platform staff inside a tenant; when `is_production == false`, always the full tenant set (FR-005a) |

Invariants (enforced by unit tests):

- Owner ⊇ Admin ⊇ Manager (set inclusion); Owner − Admin = {`billing.view`, `billing.manage`, `tenant.delete`, `owner.assign`} exactly (FR-002a).
- Manager ∌ any `settings.*` or `billing.*` permission (FR-002b).
- Viewer's set contains only `.view` permissions.
- `staff_tenant_permissions(SuperAdmin, true)` = full tenant set.
- `staff_tenant_permissions(r, false)` = full tenant set for every `r`.
- Every permission in the catalog is granted to at least one role (no orphans).

### PermissionSet (backend request-scoped value)

| Field | Type | Notes |
|-------|------|-------|
| permissions | set of `Permission` | Effective set for this request; `contains(Permission) -> bool` |

- Lifecycle: computed once per request by middleware, attached to request extensions, consumed by `require_permission` layers. Never cached across requests (FR-011).
- Construction paths: tenant scope — from `TenantRole` (tenant user) or `staff_tenant_permissions` (platform user); platform scope — from `PlatformRole`. Absent principal or unparseable role ⇒ empty set (deny by default).

### TenantContext (existing struct — extended)

| Field | Type | Change |
|-------|------|--------|
| tenant_id | Uuid | unchanged |
| tenant_status | String | unchanged |
| principal_kind | PrincipalKind | unchanged |
| tenant_role | `Option<TenantRole>` | NEW — the caller's membership role; `None` for platform staff |
| permissions | PermissionSet | NEW — effective tenant-scope set for this request |

- Sourcing change: `authorize::has_active_membership` (bool) becomes `authorize::fetch_membership_role` returning `Option<String>` from the same single query (no added round-trip).

### Effective permission payload (frontend, from `GET /me`)

| Field | Type | Notes |
|-------|------|-------|
| platformPermissions | `string[]` | Platform-scope set; empty for tenant users |
| memberships[].permissions | `string[]` | Tenant-scope set for that membership's role |
| staffTenantPermissions | `string[]` | Present only for platform users: environment-resolved tenant-scope set they hold inside any tenant |

Frontend derivation (in `PermissionsService`, all signals):

```
effective = platformPermissions
          ∪ (activeTenant ? (isPlatformUser ? staffTenantPermissions
                                            : membership(activeTenant).permissions)
                          : ∅)
```

- State transitions: recomputed automatically when `/me` snapshot or active tenant changes; `/me` is re-fetched on 403 `unauthorized`, on tenant switch, and on login (research R4/R7).

## Relationships

```
User 1 ── 0..1 PlatformRole ──→ platform_role_permissions ──→ PermissionSet (platform scope)
User 1 ── 0..* TenantMembership ── 1 TenantRole ──→ tenant_role_permissions ──→ PermissionSet (tenant scope)
PlatformRole × Environment ──→ staff_tenant_permissions ──→ PermissionSet (tenant scope, staff)
Route ── 1 required Permission (declared at registration; deny if undeclared)
```
