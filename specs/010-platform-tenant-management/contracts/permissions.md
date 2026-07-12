# Contract: Permission Catalog Delta — Platform Tenant Management

**Feature**: 010-platform-tenant-management. Delta on top of the canonical 008 catalog (`specs/008-rbac-permissions/contracts/permissions.md`, frozen as its point-in-time snapshot — the 008 spec explicitly anticipated later features declaring new permissions as they are built). After this feature the implemented catalog is the 008 list **plus** the addition below; the `catalog_parity_with_contract` unit test's expected list is updated to 26 codes referencing both documents.

## New permission

| Code | Grants |
|------|--------|
| `platform.tenants.manage` | Create tenants, edit tenant records (name, slug, plan, contact), activate/deactivate tenants |

## Platform role → platform permissions (delta row)

| Permission | Super Admin | Developer | Support Engineer | Sales | Finance |
|------------|:-----------:|:---------:|:----------------:|:-----:|:-------:|
| platform.tenants.manage | ✅ | — | ✅ | — | — |

(Clarified 2026-07-11: Super Admin + Support Engineer manage; Developer, Sales, Finance are view-only. All other rows unchanged from 008.)

`staff_tenant_permissions` (staff-inside-a-tenant) is **not** affected — `platform.tenants.manage` is a platform-scope capability only.

## Page → permission mapping (changes)

| Page / area | Before | After |
|-------------|--------|-------|
| Platform area base (`/platform`) | `platform.admin` | `platform.tenants.list` |
| Platform overview placeholder | (inherited area gate) | `platform.admin` (own route gate) |
| Tenants list / detail / new (`/platform/tenants…`) | — (new) | `platform.tenants.list` |

- In-page management actions (create button, edit form entry, activate/deactivate) are gated by `platform.tenants.manage` via `*appHasPermission`; the server enforces the same permission on POST/PATCH regardless of UI state.
- Platform-nav header control (009): gains a "Tenants" destination requiring `platform.tenants.list` — every platform role now sees the control with at least the Tenants entry; the existing empty-list hiding rule is unchanged.

## Visibility matrix (net effect)

| Capability | Super Admin | Support Engineer | Developer / Sales / Finance | Tenant users (all roles) |
|------------|:-----------:|:----------------:|:---------------------------:|:------------------------:|
| See tenant directory + detail pages | ✅ | ✅ | ✅ | ❌ (403 / no nav) |
| Create / edit / activate / deactivate | ✅ | ✅ | ❌ (403; actions hidden) | ❌ |
| Platform overview placeholder | ✅ | ❌ | ❌ | ❌ |
