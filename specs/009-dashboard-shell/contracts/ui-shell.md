# Contract: Dashboard Shell UI

**Feature**: 009-dashboard-shell. UI behavior contracts for the authenticated shell. No REST changes — the shell consumes `GET /me` exactly as specified in `specs/008-rbac-permissions/contracts/rest-api.md`.

## Shell surfaces & visibility matrix

| Surface | Tenant user | Platform user (no active tenant) | Platform user (active tenant) |
|---------|-------------|----------------------------------|-------------------------------|
| Sidebar tenant navigation | ✅ permission-filtered | — (no entries) | ✅ filtered by `staffTenantPermissions` |
| Tenant switcher (header) | ❌ never | ✅ | ✅ |
| Platform nav control (header) | ❌ never | ✅ iff holds a platform-destination permission (today: `platform.admin`) | same |
| Avatar menu (header) | ✅ | ✅ | ✅ |
| Sidebar footer user card | **removed** | **removed** | **removed** |
| Theme control (header) | ✅ unchanged | ✅ | ✅ |

Visibility gating is presentation-only; the server (feature 008) remains the enforcement boundary.

## Avatar menu (user-menu component)

- Trigger: `app-avatar` (initials) button at the far right of the topbar; `aria-expanded` reflects state; `aria-haspopup="menu"`.
- Content: display name, email, role line per the context-aware rule — platform role display name for staff; `"<Role> · <TenantName>"` for tenant users with an active tenant; no role line otherwise. Role display names: Owner, Admin, Manager, Support Agent, Viewer / Super Admin, Developer, Support Engineer, Sales, Finance.
- Actions: **Sign out** — calls the existing `AuthService.logout()` flow; on completion (or on failure/expired session) local state is cleared and the user lands on `/auth/login`. The topbar's previous bare sign-out icon button is removed.
- Dismissal: outside click, Escape, selecting an action.
- Identity values come from `/me` only — zero fixture data (SC-001).

## Platform nav control (platform-nav component)

- Placement: topbar, immediately adjacent to the tenant switcher.
- Visible iff the permission-filtered `PlatformDestination` list is non-empty (never renders an empty menu).
- Content: one entry per permitted destination; today `Platform overview → /platform` (requires `platform.admin`).
- Selecting an entry navigates and closes the menu. Dropdown interaction pattern identical to the tenant switcher (trigger + panel, `aria-expanded`, outside-click/Escape close).

## Breadcrumb contract

- Every routed page inside the shell renders a trail: `<Area> / <Page>` (deeper trails append one crumb per nested routed segment with `pageTitle`/`breadcrumb` data).
- Area labels: `Workspace` for `/tenant/*`, `Platform` for `/platform/*`. Area crumbs are non-navigable labels (area roots are redirects); intermediate ancestor crumbs are links to their accumulated path; the final crumb is the current page, non-navigable.
- Source of labels: existing `PAGE_TITLES` via route `data.pageTitle` — adding a page with `pageTitle` data yields its crumb with no extra configuration.
- Placement: within the content area above the page header (rendered by the shell, not per page).

## Responsive contract

| Viewport | Sidebar | Topbar |
|----------|---------|--------|
| ≥ 1024px | Persistent; user-collapsible to 68px icon rail (existing behavior, session-scoped) | Full: title, search, switcher/platform control (per role), theme, avatar |
| 768–1023px | Auto-collapsed to rail on entry (existing behavior preserved) | Search narrows (existing) |
| < 768px | Hidden; opens as overlay drawer (expanded width) above a scrim | Menu button toggles drawer; search hidden (existing); essential controls (switcher, theme, avatar) remain reachable |

- Drawer closes on: navigation, scrim click, Escape, viewport growing ≥ 768px.
- At every width: no horizontal scrolling of the shell (`SC-004`); all navigation reachable within two interactions (open drawer → tap item).

## Initial-load skeleton contract

- `index.html` ships static placeholder markup inside `<app-root>`: sidebar rail silhouette, topbar bar, content block — neutral surfaces, no text, no interactive elements, no role-dependent controls.
- An inline `<head>` script stamps `data-theme` from localStorage `app.themeMode` (resolving `system` via `prefers-color-scheme`) before first paint; skeleton colors derive from the same theme variables so both themes render correctly.
- The skeleton is fully replaced at Angular bootstrap, which occurs only after `/me` resolves (blocking initializer, unchanged) — therefore role-dependent controls never flash (FR-012, SC-005).
- Skeleton dimensions mirror the shell: sidebar 248px (hidden < 768px), topbar 60px, content max-width 1320px.

## Page pattern contract

- Every routed page renders as: breadcrumb (shell-provided) → `app-page-header` (title + optional description + optional actions via content projection) → `app-page-container` (max-width 1320px, standard padding) wrapping the page body.
- Loading: pages use `app-loading-state` inside the container while data resolves.
- Empty: pages use `app-empty-state` (icon, title, description, optional projected action) for no-data views; the no-role user's tenant-select landing uses this pattern with sign-out reachable via the avatar menu (FR-015).
- No page re-implements header/container/loading/empty markup (SC-007).
