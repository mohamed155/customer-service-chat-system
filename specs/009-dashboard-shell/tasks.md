# Tasks: Dashboard Shell

**Input**: Design documents from `/specs/009-dashboard-shell/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui-shell.md, quickstart.md

**Tests**: Included where the codebase's established convention already covers the surface being changed (component/store/guard specs exist for every comparable piece today — sidebar, topbar, layout store). Not exhaustive TDD narration; each implementation task's matching spec file is listed alongside it.

**Organization**: Tasks are grouped by user story per spec.md. US1 (role-appropriate shell) is the MVP; US2 (wayfinding), US3 (responsive), US4 (loading/empty) build on it.

**Note on plan.md accuracy**: while researching, `layout/page-container/page-container.component.ts` (`PageContainerComponent`) was found to **already exist and already be adopted by all 8 tenant pages, `tenant-select`, and (partially) the platform placeholder** — plan.md's Source Code tree listed this as new under `shared/components/`. Tasks below reflect the real current state: no new container component is created; only `app-page-header` adoption (currently used by exactly one page) is genuinely missing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3 / US4 per spec.md
- Exact file paths in every description

## Path Conventions

Angular dashboard app at `frontend/apps/dashboard/src/app/` (spec 002 layering: `core/`, `shared/`, `layout/`, `features/`). No backend changes.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Pre-boot skeleton and theme-restore script that every user story's "no flash / no blank screen" requirement depends on

- [ ] T001 Add static skeleton markup inside `<app-root>` in `frontend/apps/dashboard/src/index.html`: sidebar rail silhouette (248px, hidden via media query `< 768px`), topbar bar (60px), content block placeholder — neutral surfaces only, no text, no interactive elements, styled with inline `<style>` using literal `--app-*` token values (light + dark) since design-system CSS is not yet loaded
- [ ] T002 Add an inline `<head>` script in `frontend/apps/dashboard/src/index.html` (before the skeleton styles) that reads `localStorage['app.themeMode']`, resolves `system` via `matchMedia('(prefers-color-scheme: dark)')`, and sets `data-theme` on `<html>` before first paint (mirrors the resolution logic already in `core/state/system-theme.ts` — reuse its exported pure function's logic inline since it can't be imported pre-bundle)

**Checkpoint**: Reloading the app shows a themed skeleton frame instead of a blank screen, before any user story work begins

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared identity/role and viewport primitives every user story consumes

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T003 [P] Create `frontend/apps/dashboard/src/app/core/identity/role-display.ts`: `TENANT_ROLE_LABELS`/`PLATFORM_ROLE_LABELS` maps (owner→Owner, admin→Admin, manager→Manager, agent→"Support Agent", viewer→Viewer; super_admin→"Super Admin", developer→Developer, support→"Support Engineer", sales→Sales, finance→Finance) and `roleLabel(user: MeResponse | null, activeTenant: TenantSummary | null): string | null` implementing the clarified rule (platform role label for staff; `"<TenantRoleLabel> · <TenantName>"` from the membership matching `activeTenant.id` for tenant users; `null` otherwise) + `frontend/apps/dashboard/src/app/core/identity/role-display.spec.ts` covering all 10 roles, no-role case, and mismatched/absent active tenant
- [ ] T004 [P] Extend `LayoutStore` in `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.ts`: add `MOBILE_BREAKPOINT = 768`, `isMobile` computed (`viewportWidth() < 768`), `drawerOpen` state (default `false`); add `openDrawer()`/`closeDrawer()` methods; in `onInit`, inject `Router` and subscribe to `NavigationEnd` to call `closeDrawer()`; update `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.spec.ts` with cases for `isMobile` at `<768px`, `drawerOpen` toggling, and auto-close on navigation (depends on T003 only insofar as both touch `core`/`layout` — no actual code dependency; safe to parallelize)

**Checkpoint**: `role-display.ts` and the extended `LayoutStore` are available — user story implementation can begin

---

## Phase 3: User Story 1 - Signed-in users land in a role-appropriate shell (Priority: P1) 🎯 MVP

**Goal**: Header avatar menu (real identity + sign-out) replaces the fixture sidebar footer card; platform destinations move to a header control beside the tenant switcher; sidebar stays tenant-only and permission-filtered (unchanged logic, footer removed)

**Independent Test**: Sign in as a tenant user and as a platform user (with and without an active tenant); verify sidebar/switcher/platform-control visibility per the matrix in `contracts/ui-shell.md`, avatar menu shows real identity with working sign-out, and zero fixture data renders anywhere

### Implementation for User Story 1

- [ ] T005 [P] [US1] Create `frontend/apps/dashboard/src/app/layout/topbar/user-menu.component.ts`: `app-avatar` trigger (initials from `CurrentUserService.currentUser().displayName`) opening a dropdown (pattern from `tenant-switcher.component.ts` — trigger + panel + `aria-expanded`/`aria-haspopup="menu"`, outside-click + Escape close) showing display name, email, and `roleLabel()` (from T003, computed against `CurrentUserService.currentUser()` + `TenantContextService.activeTenant()`; omit the role line when `null`) and a **Sign out** action calling `AuthService.logout()`; styled with `--app-*` tokens only + `user-menu.component.spec.ts` covering: identity display, role-label variants (platform staff / tenant user / no role), sign-out delegation, dropdown open/close
- [ ] T006 [P] [US1] Create `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`: typed `PLATFORM_DESTINATIONS` constant (`{ label: 'Platform overview', path: \`/${APP_PATHS.platform.base}\`, permission: 'platform.admin' }`), filtered through `PermissionsService.has()`; renders nothing when the filtered list is empty; otherwise an icon-button trigger opening the same dropdown pattern as T005/`tenant-switcher`, listing destinations, navigating and closing on selection + `platform-nav.component.spec.ts` covering: hidden for tenant users, hidden for platform roles without `platform.admin`, visible + navigates for Super Admin
- [ ] T007 [US1] Wire `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts`: import `UserMenuComponent` and `PlatformNavComponent`; render `<app-platform-nav />` adjacent to `<app-tenant-switcher />` (both inside the existing `@if (isPlatformUser())` block per the visibility matrix — `platform-nav` self-hides further by permission); replace the bare `<app-icon-button icon="@tui.log-out" ...>` sign-out button with `<app-user-menu />`; remove now-unused `isAuthenticated`/`signOut` members if `user-menu` fully owns sign-out; update `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.spec.ts` — replace the `[aria-label="Sign out"]` icon-button assertions with avatar-menu-open-then-sign-out assertions, add a platform-control visibility case (depends on T005, T006)
- [ ] T008 [US1] Remove the `<footer>` block (avatar, name/role/company text, sign-out icon button) from `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts`, and remove the now-unused `SIDEBAR_USER`/`user`/`AvatarComponent`/`IconButtonComponent` references if no longer used elsewhere in the template; update `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.spec.ts` to drop footer assertions (depends on T007 so sign-out is not lost mid-refactor)
- [ ] T009 [US1] Delete `SIDEBAR_USER` from `frontend/apps/dashboard/src/app/shared/fixtures/settings.fixtures.ts` and the now-unused `SidebarUserFixture` interface from `frontend/apps/dashboard/src/app/shared/fixtures/fixture.models.ts`; grep the app for any remaining reference and remove it (depends on T008)

**Checkpoint**: User Story 1 fully functional — sign in as any role and verify the visibility matrix and real-identity avatar menu independently of US2–US4

---

## Phase 4: User Story 2 - Users always know where they are (Priority: P2)

**Goal**: Every page shows a breadcrumb trail and a shared in-content page header; all pages already share `PageContainerComponent` (pre-existing) for width/padding

**Independent Test**: Navigate to every page; verify the breadcrumb trail matches location and ancestor links navigate; verify every page renders `<app-page-header>` inside `<app-page-container>` with consistent title placement

### Implementation for User Story 2

- [ ] T010 [P] [US2] Create `frontend/apps/dashboard/src/app/core/router/breadcrumb.ts`: `injectBreadcrumbs(): Signal<Crumb[]>` — reuse the root→leaf traversal pattern from `injectPageTitle()` in `core/router/page-title.ts`; build `[areaRoot, ...pageCrumbs]` where the area root is `{ label: 'Workspace' | 'Platform', link: null }` (derived from the first path segment) and each routed segment with `data.pageTitle` contributes `{ label: PAGE_TITLES[key].title, link: accumulatedPath }`, with the final crumb's `link` forced to `null`; reactive to `NavigationEnd` like `injectPageTitle` + `breadcrumb.spec.ts` covering a tenant page trail, the platform trail, and the tenant-select route (no `pageTitle` data → single non-navigable crumb or empty trail — pick one and assert it)
- [ ] T011 [P] [US2] Create `frontend/apps/dashboard/src/app/layout/breadcrumb/breadcrumb.component.ts`: consumes `injectBreadcrumbs()`, renders an `<nav aria-label="Breadcrumb"><ol>` with `routerLink` on entries whose `link` is non-null and plain text for the rest, `aria-current="page"` on the last entry + `breadcrumb.component.spec.ts` asserting link vs. non-link rendering and current-page marking (depends on T010)
- [ ] T012 [US2] Mount `<app-breadcrumb />` in `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts`, inside `<main>` above `<router-outlet />`; update `app-shell.component.spec.ts` to assert the breadcrumb nav renders (depends on T011)
- [ ] T013 [US2] Add an optional `description` input to `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.ts` (rendered as a `<p>` under the `<h1>` when provided, keeping the existing `<ng-content />` slot for actions) and update `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.spec.ts` to cover the new input
- [ ] T014 [P] [US2] Add `<app-page-header title="..." [description]="...">` as the first child inside `<app-page-container>` in each of: `frontend/apps/dashboard/src/app/features/tenant/overview/overview.component.ts`, `.../conversations/conversations.component.ts`, `.../customers/customers.component.ts`, `.../ai-agent/ai-agent.component.ts`, `.../knowledge-base/knowledge-base.component.ts`, `.../integrations/integrations.component.ts`, `.../analytics/analytics.component.ts`, `.../settings/settings.component.ts` — reuse each page's existing `PAGE_TITLES` title/subtitle text for consistency with the topbar; add `PageHeaderComponent` to each component's `imports` array; update each page's existing `.spec.ts` where header text assertions are needed (depends on T013)
- [ ] T015 [US2] Add `<app-page-container>` wrapping the existing content in `frontend/apps/dashboard/src/app/features/platform/overview-placeholder/platform-overview-placeholder.component.ts` (it currently renders `<app-page-header>` directly with no container) and update its spec if present (depends on T013)

**Checkpoint**: US1 + US2 independently functional — breadcrumbs and consistent page headers on every route

---

## Phase 5: User Story 3 - The shell works on any screen size (Priority: P3)

**Goal**: Sidebar becomes an overlay drawer below 768px (foundation state already added in T004); desktop collapse behavior (≥1024px) unchanged

**Independent Test**: Resize to desktop/tablet/phone widths; verify sidebar mode (persistent/collapsible vs. drawer), header condensation (existing), and no horizontal overflow

### Implementation for User Story 3

- [ ] T016 [US3] Update `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts`: inject `LayoutStore`'s `isMobile`/`drawerOpen`; when `isMobile()`, render the sidebar as a fixed-position overlay (via `[class.drawer]`/`[class.open]` host bindings on `<app-sidebar>`'s wrapper) with a scrim `<div>` behind it that closes the drawer on click and appears only when `drawerOpen()`; add an Escape keydown handler (`(keydown.escape)` on the shell root or a `HostListener`) calling `closeDrawer()`; update `app-shell.component.spec.ts` with mobile-width cases: drawer closed by default, opens via the menu button, closes on scrim click and Escape (depends on T004)
- [ ] T017 [US3] Update `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts`: the existing `toggleSidebar()` dispatch (desktop collapse) branches on `LayoutStore.isMobile()` — when mobile, call `LayoutStore.openDrawer()`/`closeDrawer()` instead of dispatching `appUiActions.sidebarToggled()`; update `topbar.component.spec.ts` for the mobile-toggle branch (depends on T016)
- [ ] T018 [US3] Update `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` CSS: at `<768px` (matching `LAYOUT_COLLAPSE_BREAKPOINT`'s sibling `MOBILE_BREAKPOINT` from T004) ensure `width` uses the expanded value even when the global `collapsed` signal is true (drawer always shows full labels), and verify no horizontal overflow contribution — this is a styles-only change, add a regression case to `sidebar.component.spec.ts` asserting expanded rendering under a mobile host class (depends on T016)

**Checkpoint**: US1 + US2 + US3 independently functional — shell adapts correctly at all supported widths

---

## Phase 6: User Story 4 - Waiting and emptiness look intentional (Priority: P4)

**Goal**: Confirm and lock in the initial-load skeleton (Phase 1) plus the already-existing shared `LoadingStateComponent`/`EmptyStateComponent` are the only loading/empty presentations in the shell — this phase is verification + closing the one remaining gap (tenant-select's inline empty-state markup)

**Independent Test**: Reload on a throttled connection (skeleton, no blank screen, no flash); visit a page with no data (consistent empty state)

### Implementation for User Story 4

- [ ] T019 [US4] Replace the hand-written empty-state markup in `frontend/apps/dashboard/src/app/features/tenant/tenant-select/tenant-select.component.ts` (icon + `<h2>` + `<p>` + link, currently bespoke) with `<app-empty-state>` from `frontend/apps/dashboard/src/app/shared/components/empty-state/empty-state.component.ts`, projecting the "Back to tenant area" link as its action content; update the component's spec if present, asserting `app-empty-state` renders with the expected title/description
- [ ] T020 [P] [US4] Audit `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-base.component.ts` (already imports `EmptyStateComponent`) and the other seven tenant pages for any remaining ad hoc "no data" markup that duplicates `app-empty-state`'s pattern; replace any found (if none are found beyond knowledge-base's existing correct usage, this task closes as a no-op verification — record the finding in the task's commit message)

**Checkpoint**: All four user stories independently functional — skeleton verified via quickstart §5, empty states consistent across pages

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates and end-to-end validation

- [ ] T021 [P] Run `pnpm ng build dashboard` from `frontend/` — confirm no new bundle-budget regressions beyond the pre-existing warning noted in `specs/008-rbac-permissions` convergence
- [ ] T022 [P] Run `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` from `frontend/` — all green
- [ ] T023 Execute `specs/009-dashboard-shell/quickstart.md` §1–§6 manually (role-appropriate shell across tenant/platform roles, breadcrumbs, responsive drawer down to 360px, throttled-reload skeleton, empty states, light/dark theme pass) and record results in the PR description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: No dependency on Phase 1 (different files); BLOCKS all user stories
- **US1 (Phase 3)**: After Phase 2 (T005/T006 need `role-display.ts` from T003 conceptually but only T005 actually imports it — kept as a Foundational output for clarity)
- **US2 (Phase 4)**: After Phase 2; independent of US1 (different files) but conventionally sequenced after for a coherent shell demo
- **US3 (Phase 5)**: After Phase 2 (needs T004's `LayoutStore` extension); independent of US1/US2
- **US4 (Phase 6)**: After Phase 1 (skeleton) and Phase 2; independent of US1–US3
- **Polish (Phase 7)**: After all desired stories

### User Story Dependencies

- **US1 (P1)**: Only Foundational (T003) — independently testable via sign-in as each role
- **US2 (P2)**: Only Foundational — independently testable by visiting routes; does not require US1
- **US3 (P3)**: Only Foundational (T004) — independently testable via viewport resize; does not require US1/US2
- **US4 (P4)**: Only Setup (T001/T002) + the pre-existing `LoadingStateComponent`/`EmptyStateComponent` — independently testable via throttled reload

### Parallel Opportunities

- Phase 1: T001 then T002 (same file, sequential)
- Phase 2: T003 ∥ T004 (different files)
- US1: T005 ∥ T006 (different files) → T007 → T008 → T009
- US2: T010 → T011 → T012; T013 can run parallel to T010/T011 (different files) → T014 (8 files, internally parallel) ∥ T015
- US3: T016 → T017 → T018 (sequential — same shell/topbar coordination)
- US4: T019 ∥ T020 (different files)
- Polish: T021 ∥ T022

## Parallel Example: User Story 1

```bash
# After Phase 2 completes, launch in parallel:
Task: "T005 user-menu.component.ts in layout/topbar/"
Task: "T006 platform-nav.component.ts in layout/topbar/"
# Then sequentially: T007 (wires both into topbar) → T008 (sidebar footer removal) → T009 (fixture cleanup)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 Setup → Phase 2 Foundational (role-display + LayoutStore extension)
2. Phase 3 US1: avatar menu + platform nav control + sidebar footer removal + fixture deletion
3. **STOP and VALIDATE**: `pnpm ng test dashboard` green; sign in as a tenant user and a platform user and verify the visibility matrix (quickstart §2)
4. Deliverable: real identity everywhere, zero fixture data, correct platform/tenant control separation — deploy/demo if ready

### Incremental Delivery

1. Setup + Foundational → shell paints instantly, no flash, in either theme
2. US1 → role-appropriate identity/nav surfaces (MVP)
3. US2 → breadcrumbs + consistent page headers
4. US3 → mobile drawer
5. US4 → skeleton/empty-state consistency locked in
6. Polish → quality gates, quickstart run

---

## Notes

- No backend changes in this feature; `/me` contract from spec 008 is consumed unchanged.
- `PageContainerComponent` (`layout/page-container/page-container.component.ts`) is pre-existing and already adopted everywhere — do not recreate it.
- Commit after each task or logical group; every checkpoint is a safe stopping point.

---

## Phase 8: Convergence

**Purpose**: Close remaining gaps between the implemented shell and the spec/plan/contract. All four quality gates (build, test, lint, format) pass and the feature is functionally complete; the items below are visual-polish and contract-fidelity defects surfaced by post-implementation assessment. Ordered most-severe first.

- [X] T024 Repoint the undefined `--app-text-secondary` CSS custom property to the defined muted token `--app-text-2` in `frontend/apps/dashboard/src/app/layout/breadcrumb/breadcrumb.component.ts` (lines ~39 and ~53) and `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.ts` (line ~38); the token is not defined in `design-system/tokens/tokens.css` or `design-system/theme/themes.css`, so breadcrumb separators/inactive crumbs and the page-header description currently inherit the default text color instead of rendering muted, per FR-007 / FR-008 / SC-006 / Constitution IX (contradicts)
- [X] T025 Define `--app-fill-hover` for both light and dark in `frontend/apps/dashboard/src/app/design-system/theme/themes.css` (or repoint the references to an existing hover token such as `--app-panel-2`) so the hover backgrounds in `frontend/apps/dashboard/src/app/layout/topbar/user-menu.component.ts` (lines ~70, ~132) and `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts` (line ~120) actually apply; the token is currently undefined (the pre-existing `tenant-switcher.component.ts` shares the same reference and would also be fixed) per plan R3 / Constitution IX (contradicts)
- [X] T026 Make the platform breadcrumb read "Platform / Platform overview" instead of the current duplicate "Platform / Platform" — add a `pageTitle`/breadcrumb label to the `overview-placeholder` child route in `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts`, or suppress the area-root crumb in `frontend/apps/dashboard/src/app/core/router/breadcrumb.ts` when it duplicates the first page crumb — per FR-007 and `contracts/ui-shell.md` (partial)
- [X] T027 Resolve the initial-bundle budget regression (overage grew ~22.8 kB → ~30.6 kB after the new shell components): either raise the `budgets` threshold in `frontend/apps/dashboard/project.json`/`angular.json` to reflect the intended shell size or trim, so `pnpm ng build dashboard` no longer emits a *new* warning beyond the pre-existing baseline, per tasks T021 / Constitution X (partial)
