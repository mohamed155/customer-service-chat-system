# Research: Dashboard Shell

**Feature**: 009-dashboard-shell | **Date**: 2026-07-11

All Technical Context unknowns resolved. Each decision below records what was chosen, why, and what else was evaluated.

## R1 — Initial-load skeleton strategy

**Decision**: Keep the existing blocking `provideAppInitializer` (which resolves `/me` before Angular renders anything) and add a **static skeleton to `index.html`** — neutral sidebar rail, header bar, and content placeholders drawn with plain HTML/CSS inside `<app-root>` (Angular replaces this content at bootstrap). Add a small inline script in `<head>` that reads the persisted `app.themeMode` from localStorage (resolving `system` via `matchMedia`) and stamps `data-theme` on `<html>` before first paint, so the skeleton renders in the user's theme.

**Rationale**: The clarification requires the shell frame to paint immediately with placeholders and role-dependent controls to never flash. The blocking initializer *already* guarantees zero flash (nothing renders until entitlement is known) — its only defect is the blank screen, which the static skeleton fixes without touching guards or services. The alternative (non-blocking boot) would require converting `CurrentUserService` to expose a loading status, making `authGuard`/`areaAccessGuard`/`permissionGuard` async-aware, and auditing every consumer for the "identity not yet known" state — a large blast radius for the same user-visible result, with new flash risk if any consumer is missed.

**Alternatives considered**: (a) Non-blocking bootstrap + in-shell skeleton components + async guards — rejected for guard complexity and flash risk as above. (b) Full-screen branded loader — rejected by clarification. (c) Blank until ready (status quo) — rejected by clarification.

**Consequences**: The skeleton markup duplicates the shell's outer dimensions (sidebar width, topbar height) statically; it must use hard-coded values matching the `--app-*` tokens and a media query for the narrow-viewport variant. In-shell skeleton placeholder components are **not needed** — by the time Angular renders, identity is resolved (spec FR-012's "placeholders resolve into real controls" is satisfied at the bootstrap boundary).

## R2 — Breadcrumb derivation

**Decision**: Derive the trail from router state, reusing the existing `pageTitle` route-data convention. A new `core/router/breadcrumb.ts` exposes `injectBreadcrumbs(): Signal<Crumb[]>` that walks the activated route snapshot from root to leaf (same traversal as `injectPageTitle`) and builds crumbs from route data: an area root crumb (label only — "Workspace" for `/tenant`, "Platform" for `/platform`; non-navigable since area roots redirect) followed by one crumb per routed segment carrying `pageTitle` data, resolved through `PAGE_TITLES`. Future detail routes get deeper trails automatically by declaring `pageTitle` (or an explicit `breadcrumb` label) on nested routes; ancestor crumbs link to their accumulated router path.

**Rationale**: Reuses the established, tested route-data mechanism instead of inventing a parallel registry; keeps labels in one place (`PAGE_TITLES`); zero configuration for existing pages. Walking accumulated paths gives correct ancestor links without hardcoding URLs (APP_PATHS discipline preserved).

**Alternatives considered**: (a) Static breadcrumb map keyed by URL — rejected: duplicates route structure, breaks on nested/detail routes. (b) Angular `TitleStrategy`-based derivation — rejected: titles serve the browser tab, conflating them with trails couples two concerns. (c) Per-page manual breadcrumb input — rejected: violates FR-014 reusability and invites drift.

## R3 — Dropdown pattern for user menu and platform nav

**Decision**: Follow the existing hand-rolled dropdown pattern established by `tenant-switcher.component.ts` (trigger button + positioned panel + `aria-expanded`, click/keyboard close), extracted-in-style but implemented per component, styled exclusively with `--app-*` tokens. The user menu trigger is the existing `app-avatar` component (initials from display name); the platform nav trigger is an icon button.

**Rationale**: The shell already ships one production dropdown with this pattern; matching it keeps interaction behavior and styling consistent (Constitution IX: patterns before features) and avoids introducing Taiga overlay/portal wiring that no other shell component uses. Taiga remains in use for icons per the established convention.

**Alternatives considered**: (a) Taiga `TuiDropdown`/`TuiDataList` — rejected for this feature: would make the third dropdown behave/style differently from the existing switcher unless the switcher were also migrated, which is out of scope; spec 003 rule also requires wrapping Taiga inside project components anyway. (b) Native `<details>`/popover API — rejected: inconsistent styling control and focus behavior across the supported browser range.

## R4 — Responsive drawer mechanics

**Decision**: Extend the existing `LayoutStore` signal store with a second breakpoint (`MOBILE_BREAKPOINT = 768`) and state: `isMobile` (computed from viewport width) and `drawerOpen` (boolean, default false, session-only). Behavior: at `<768px` the sidebar is hidden from the grid and renders as a fixed overlay (full-height, above a scrim) when `drawerOpen`; the topbar menu button toggles the drawer instead of collapse; the drawer closes on any router navigation (Router events subscription in the store), on scrim click, and on Escape. The existing `≥768px && <1024px` auto-collapse behavior is preserved unchanged.

**Rationale**: `LayoutStore` already owns viewport tracking and the 1024px collapse rule — extending it keeps all viewport policy in one place (no second resize listener). Close-on-navigate via Router events is centralized rather than per-nav-item handlers. 768px matches the existing topbar CSS breakpoint where search hides, giving one coherent narrow-mode threshold.

**Alternatives considered**: (a) CSS-only drawer (checkbox hack / `:target`) — rejected: cannot close on navigation and is untestable state. (b) Separate DrawerService — rejected: fragments viewport policy across two singletons. (c) Taiga sidebar/drawer component — rejected: shell chrome is bespoke per spec 003 reference design.

## R5 — Context-aware role display

**Decision**: New `core/identity/role-display.ts` exposing a pure role-code→display-name map (`owner→Owner, admin→Admin, manager→Manager, agent→Support Agent, viewer→Viewer, super_admin→Super Admin, developer→Developer, support→Support Engineer, sales→Sales, finance→Finance`) and a `roleLabel(user, activeTenant)` helper implementing the clarified rule: platform role name for staff; `"<TenantRole> · <TenantName>"` for tenant users with an active tenant (from the membership matching the active tenant); `null` when neither applies (no role line rendered).

**Rationale**: Display names are pure presentation — this does **not** violate feature 008's FR-010 (no frontend role→*permission* mapping); permission sets continue to come exclusively from `/me`. Centralizing the map satisfies FR-014 (no per-component re-implementation) and gives specs one unit to pin the wording ("Support Agent", "Support Engineer" naming from the 008 spec).

**Alternatives considered**: (a) Server-provided display names in `/me` — rejected: requires a backend contract change for pure presentation text; the role *codes* are already a stable contract. (b) Inline formatting in the user-menu component — rejected: the tenant-select page and future surfaces need the same labels.

## R6 — Page container standardization

**Decision**: New `shared/components/page-container/page-container.component.ts` — a content wrapper applying the content max-width token (1320px per spec 003), horizontal page padding, and vertical rhythm. All routed pages (8 tenant pages, platform placeholder, tenant-select) migrate to `<app-page-container>` + the existing `app-page-header`. `PageHeaderComponent` gains an optional `description` input so pages can show the header subtitle pattern without bespoke markup.

**Rationale**: Pages currently self-manage padding/width, which is exactly the per-page duplication FR-008/SC-007 prohibit. A wrapper component (rather than a CSS class on `main`) keeps the pattern discoverable, enforceable in specs, and allows per-page exceptions (e.g., full-bleed views later) explicitly.

**Alternatives considered**: (a) Style `main` in the shell once — rejected: prevents any future full-bleed page and hides the pattern from page specs. (b) CSS utility class — rejected: not enforceable/testable as a pattern, violates the components-before-pages discipline.

## R7 — Platform navigation header control

**Decision**: New `layout/topbar/platform-nav.component.ts` rendered in the topbar adjacent to the tenant switcher, visible only when the signed-in user holds any platform-destination permission (currently `platform.admin` via `PermissionsService.has()`). It renders the R3 dropdown listing platform destinations — currently one entry, "Platform overview" → `/platform` — from a typed constant colocated with the component that pairs each destination with its `APP_PATHS` route and required permission, filtered by `PermissionsService`.

**Rationale**: Implements the clarified "sidebar stays tenant-only" decision with the same entitlement-driven visibility model the sidebar already uses (permission-filtered, presentation-only). A typed destination list makes adding future platform pages a one-line change (spec assumption: accommodate growth without rework). Note the deliberate asymmetry: the tenant *switcher* shows for any platform user (they all hold `platform.tenants.list/switch`), while the platform *nav control* shows only for roles holding platform page permissions (today: Super Admin) — this follows the 008 permission matrix exactly.

**Alternatives considered**: (a) Direct link button (no dropdown) while only one destination exists — rejected: the control's contract is a destination list; shipping it as a dropdown now avoids a behavioral change when the second destination lands. (b) Visibility keyed on `isPlatformUser()` — rejected: would show an empty menu to staff roles with no platform page permissions, violating SC-002's "zero navigation entries lead to a page the user cannot use".

## R8 — Sidebar footer removal & fixture cleanup

**Decision**: Remove the `<footer>` block from `sidebar.component.ts` entirely and delete the `SIDEBAR_USER` fixture from `shared/fixtures/settings.fixtures.ts` (plus its export/usages). The header avatar menu (R3/R5) is the single identity surface per the clarification.

**Rationale**: Leaving the fixture in place invites re-use; deleting it makes SC-001 (zero placeholder identity) structurally true rather than convention-dependent. Sidebar specs asserting footer content are updated in the same change (Constitution VII).

**Alternatives considered**: Keeping a compact real-identity footer — rejected by clarification (option C was declined).
