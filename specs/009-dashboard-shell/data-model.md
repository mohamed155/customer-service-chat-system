# Data Model: Dashboard Shell

**Feature**: 009-dashboard-shell | **Date**: 2026-07-11

Frontend-only feature: no database or API schema changes. All entities are code-level presentation models inside `frontend/apps/dashboard`, derived from the existing `GET /me` payload (feature 008 contract, unchanged) and router state.

## Entities

### NavigationItem (sidebar + platform control)

| Field | Type | Notes |
|-------|------|-------|
| label | string | Display text (e.g., "Conversations", "Platform overview") |
| icon | string | Taiga icon name |
| path | string | Built from `APP_PATHS` constants — never string literals |
| permission | `Permission` | From `PAGE_PERMISSIONS` / platform destination list; visibility gate |
| surface | `'sidebar' \| 'platform-control'` | Sidebar carries tenant-workspace pages only (clarified); the header platform control carries platform destinations |

- Validation: every item MUST declare a permission (deny-by-default visibility — an item with no permission never renders).
- Visibility rule: `PermissionsService.has(permission)`; groups with zero visible items collapse entirely (existing sidebar behavior, preserved).

### Crumb (breadcrumb trail entry)

| Field | Type | Notes |
|-------|------|-------|
| label | string | Resolved via `PAGE_TITLES[pageTitleKey].title`, or the area label for root crumbs |
| link | string \| null | Accumulated router path for ancestors; `null` for the area root (redirect-only) and for the final (current) crumb |

- Derivation: walk activated route snapshot root→leaf (see research R2); trail = `[areaRoot, ...routedSegmentsWithPageTitle]`.
- State transitions: recomputed on every `NavigationEnd` (signal from `injectBreadcrumbs()`).
- Invariant: final crumb always names the current page; only ancestor crumbs are navigable (FR-007).

### UserIdentitySummary (avatar menu content)

| Field | Type | Notes |
|-------|------|-------|
| displayName | string | `MeResponse.displayName` |
| email | string | `MeResponse.email` |
| initials | string | Derived from displayName (existing avatar convention) |
| roleLabel | string \| null | Context-aware (clarified): platform role display name for staff; `"<Role> · <TenantName>"` from the active-tenant membership for tenant users; `null` when neither applies |

- Sourcing: computed from `CurrentUserService.currentUser()` + `tenantContext.activeTenant()` — never stored separately (spec Key Entities).
- Role display names come from the `role-display.ts` map (research R5); permission data is NOT consulted for display.

### ShellLayoutState (extended `LayoutStore`)

| Field | Type | Notes |
|-------|------|-------|
| viewportWidth | number | Existing — resize-tracked |
| isNarrow | computed boolean | Existing — `< 1024`; drives auto-collapse |
| isMobile | computed boolean | NEW — `< 768`; switches sidebar to drawer mode |
| drawerOpen | boolean | NEW — mobile drawer visibility; session-only, default `false` |

- Transitions: `drawerOpen` → `false` on any router navigation, scrim click, Escape, or viewport growing past 768px; → `true` only via the topbar menu button while `isMobile`.
- Persistence: none (matches the established "sidebar state is not persisted" rule). Theme (`app.themeMode` in localStorage) is unchanged and additionally read by the pre-boot inline script (research R1).

### PlatformDestination (typed constant list)

| Field | Type | Notes |
|-------|------|-------|
| label | string | e.g., "Platform overview" |
| path | string | From `APP_PATHS.platform.*` |
| permission | `Permission` | Currently `platform.admin`; future destinations declare theirs |

- The header platform control renders the permission-filtered list; the control itself hides when the filtered list is empty (research R7).

## Relationships

```
MeResponse (/me, unchanged) ──→ UserIdentitySummary (+ activeTenant from tenantContext slice)
PermissionsService.effective ──→ NavigationItem.visible / PlatformDestination.visible
Router state (route data: pageTitle) ──→ Crumb[] (injectBreadcrumbs)
Viewport width ──→ ShellLayoutState.{isNarrow, isMobile} ──→ sidebar mode (rail | collapsed | drawer)
```
