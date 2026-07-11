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

- [X] T001 Add static skeleton markup inside `<app-root>` in `frontend/apps/dashboard/src/index.html`: sidebar rail silhouette (248px, hidden via media query `< 768px`), topbar bar (60px), content block placeholder — neutral surfaces only, no text, no interactive elements, styled with inline `<style>` using literal `--app-*` token values (light + dark) since design-system CSS is not yet loaded
- [X] T002 Add an inline `<head>` script in `frontend/apps/dashboard/src/index.html` (before the skeleton styles) that reads `localStorage['app.themeMode']`, resolves `system` via `matchMedia('(prefers-color-scheme: dark)')`, and sets `data-theme` on `<html>` before first paint (mirrors the resolution logic already in `core/state/system-theme.ts` — reuse its exported pure function's logic inline since it can't be imported pre-bundle)

**Checkpoint**: Reloading the app shows a themed skeleton frame instead of a blank screen, before any user story work begins

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared identity/role and viewport primitives every user story consumes

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T003 [P] Create `frontend/apps/dashboard/src/app/core/identity/role-display.ts`: `TENANT_ROLE_LABELS`/`PLATFORM_ROLE_LABELS` maps (owner→Owner, admin→Admin, manager→Manager, agent→"Support Agent", viewer→Viewer; super_admin→"Super Admin", developer→Developer, support→"Support Engineer", sales→Sales, finance→Finance) and `roleLabel(user: MeResponse | null, activeTenant: TenantSummary | null): string | null` implementing the clarified rule (platform role label for staff; `"<TenantRoleLabel> · <TenantName>"` from the membership matching `activeTenant.id` for tenant users; `null` otherwise) + `frontend/apps/dashboard/src/app/core/identity/role-display.spec.ts` covering all 10 roles, no-role case, and mismatched/absent active tenant
- [X] T004 [P] Extend `LayoutStore` in `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.ts`: add `MOBILE_BREAKPOINT = 768`, `isMobile` computed (`viewportWidth() < 768`), `drawerOpen` state (default `false`); add `openDrawer()`/`closeDrawer()` methods; in `onInit`, inject `Router` and subscribe to `NavigationEnd` to call `closeDrawer()`; update `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.spec.ts` with cases for `isMobile` at `<768px`, `drawerOpen` toggling, and auto-close on navigation (depends on T003 only insofar as both touch `core`/`layout` — no actual code dependency; safe to parallelize)

**Checkpoint**: `role-display.ts` and the extended `LayoutStore` are available — user story implementation can begin

---

## Phase 3: User Story 1 - Signed-in users land in a role-appropriate shell (Priority: P1) 🎯 MVP

**Goal**: Header avatar menu (real identity + sign-out) replaces the fixture sidebar footer card; platform destinations move to a header control beside the tenant switcher; sidebar stays tenant-only and permission-filtered (unchanged logic, footer removed)

**Independent Test**: Sign in as a tenant user and as a platform user (with and without an active tenant); verify sidebar/switcher/platform-control visibility per the matrix in `contracts/ui-shell.md`, avatar menu shows real identity with working sign-out, and zero fixture data renders anywhere

### Implementation for User Story 1

- [X] T005 [P] [US1] Create `frontend/apps/dashboard/src/app/layout/topbar/user-menu.component.ts`: `app-avatar` trigger (initials from `CurrentUserService.currentUser().displayName`) opening a dropdown (pattern from `tenant-switcher.component.ts` — trigger + panel + `aria-expanded`/`aria-haspopup="menu"`, outside-click + Escape close) showing display name, email, and `roleLabel()` (from T003, computed against `CurrentUserService.currentUser()` + `TenantContextService.activeTenant()`; omit the role line when `null`) and a **Sign out** action calling `AuthService.logout()`; styled with `--app-*` tokens only + `user-menu.component.spec.ts` covering: identity display, role-label variants (platform staff / tenant user / no role), sign-out delegation, dropdown open/close
- [X] T006 [P] [US1] Create `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts`: typed `PLATFORM_DESTINATIONS` constant (`{ label: 'Platform overview', path: \`/${APP_PATHS.platform.base}\`, permission: 'platform.admin' }`), filtered through `PermissionsService.has()`; renders nothing when the filtered list is empty; otherwise an icon-button trigger opening the same dropdown pattern as T005/`tenant-switcher`, listing destinations, navigating and closing on selection + `platform-nav.component.spec.ts` covering: hidden for tenant users, hidden for platform roles without `platform.admin`, visible + navigates for Super Admin
- [X] T007 [US1] Wire `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts`: import `UserMenuComponent` and `PlatformNavComponent`; render `<app-platform-nav />` adjacent to `<app-tenant-switcher />` (both inside the existing `@if (isPlatformUser())` block per the visibility matrix — `platform-nav` self-hides further by permission); replace the bare `<app-icon-button icon="@tui.log-out" ...>` sign-out button with `<app-user-menu />`; remove now-unused `isAuthenticated`/`signOut` members if `user-menu` fully owns sign-out; update `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.spec.ts` — replace the `[aria-label="Sign out"]` icon-button assertions with avatar-menu-open-then-sign-out assertions, add a platform-control visibility case (depends on T005, T006)
- [X] T008 [US1] Remove the `<footer>` block (avatar, name/role/company text, sign-out icon button) from `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts`, and remove the now-unused `SIDEBAR_USER`/`user`/`AvatarComponent`/`IconButtonComponent` references if no longer used elsewhere in the template; update `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.spec.ts` to drop footer assertions (depends on T007 so sign-out is not lost mid-refactor)
- [X] T009 [US1] Delete `SIDEBAR_USER` from `frontend/apps/dashboard/src/app/shared/fixtures/settings.fixtures.ts` and the now-unused `SidebarUserFixture` interface from `frontend/apps/dashboard/src/app/shared/fixtures/fixture.models.ts`; grep the app for any remaining reference and remove it (depends on T008)

**Checkpoint**: User Story 1 fully functional — sign in as any role and verify the visibility matrix and real-identity avatar menu independently of US2–US4

---

## Phase 4: User Story 2 - Users always know where they are (Priority: P2)

**Goal**: Every page shows a breadcrumb trail and a shared in-content page header; all pages already share `PageContainerComponent` (pre-existing) for width/padding

**Independent Test**: Navigate to every page; verify the breadcrumb trail matches location and ancestor links navigate; verify every page renders `<app-page-header>` inside `<app-page-container>` with consistent title placement

### Implementation for User Story 2

- [X] T010 [P] [US2] Create `frontend/apps/dashboard/src/app/core/router/breadcrumb.ts`: `injectBreadcrumbs(): Signal<Crumb[]>` — reuse the root→leaf traversal pattern from `injectPageTitle()` in `core/router/page-title.ts`; build `[areaRoot, ...pageCrumbs]` where the area root is `{ label: 'Workspace' | 'Platform', link: null }` (derived from the first path segment) and each routed segment with `data.pageTitle` contributes `{ label: PAGE_TITLES[key].title, link: accumulatedPath }`, with the final crumb's `link` forced to `null`; reactive to `NavigationEnd` like `injectPageTitle` + `breadcrumb.spec.ts` covering a tenant page trail, the platform trail, and the tenant-select route (no `pageTitle` data → single non-navigable crumb or empty trail — pick one and assert it)
- [X] T011 [P] [US2] Create `frontend/apps/dashboard/src/app/layout/breadcrumb/breadcrumb.component.ts`: consumes `injectBreadcrumbs()`, renders an `<nav aria-label="Breadcrumb"><ol>` with `routerLink` on entries whose `link` is non-null and plain text for the rest, `aria-current="page"` on the last entry + `breadcrumb.component.spec.ts` asserting link vs. non-link rendering and current-page marking (depends on T010)
- [X] T012 [US2] Mount `<app-breadcrumb />` in `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts`, inside `<main>` above `<router-outlet />`; update `app-shell.component.spec.ts` to assert the breadcrumb nav renders (depends on T011)
- [X] T013 [US2] Add an optional `description` input to `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.ts` (rendered as a `<p>` under the `<h1>` when provided, keeping the existing `<ng-content />` slot for actions) and update `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.spec.ts` to cover the new input
- [X] T014 [P] [US2] Add `<app-page-header title="..." [description]="...">` as the first child inside `<app-page-container>` in each of: `frontend/apps/dashboard/src/app/features/tenant/overview/overview.component.ts`, `.../conversations/conversations.component.ts`, `.../customers/customers.component.ts`, `.../ai-agent/ai-agent.component.ts`, `.../knowledge-base/knowledge-base.component.ts`, `.../integrations/integrations.component.ts`, `.../analytics/analytics.component.ts`, `.../settings/settings.component.ts` — reuse each page's existing `PAGE_TITLES` title/subtitle text for consistency with the topbar; add `PageHeaderComponent` to each component's `imports` array; update each page's existing `.spec.ts` where header text assertions are needed (depends on T013)
- [X] T015 [US2] Add `<app-page-container>` wrapping the existing content in `frontend/apps/dashboard/src/app/features/platform/overview-placeholder/platform-overview-placeholder.component.ts` (it currently renders `<app-page-header>` directly with no container) and update its spec if present (depends on T013)

**Checkpoint**: US1 + US2 independently functional — breadcrumbs and consistent page headers on every route

---

## Phase 5: User Story 3 - The shell works on any screen size (Priority: P3)

**Goal**: Sidebar becomes an overlay drawer below 768px (foundation state already added in T004); desktop collapse behavior (≥1024px) unchanged

**Independent Test**: Resize to desktop/tablet/phone widths; verify sidebar mode (persistent/collapsible vs. drawer), header condensation (existing), and no horizontal overflow

### Implementation for User Story 3

- [X] T016 [US3] Update `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts`: inject `LayoutStore`'s `isMobile`/`drawerOpen`; when `isMobile()`, render the sidebar as a fixed-position overlay (via `[class.drawer]`/`[class.open]` host bindings on `<app-sidebar>`'s wrapper) with a scrim `<div>` behind it that closes the drawer on click and appears only when `drawerOpen()`; add an Escape keydown handler (`(keydown.escape)` on the shell root or a `HostListener`) calling `closeDrawer()`; update `app-shell.component.spec.ts` with mobile-width cases: drawer closed by default, opens via the menu button, closes on scrim click and Escape (depends on T004)
- [X] T017 [US3] Update `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts`: the existing `toggleSidebar()` dispatch (desktop collapse) branches on `LayoutStore.isMobile()` — when mobile, call `LayoutStore.openDrawer()`/`closeDrawer()` instead of dispatching `appUiActions.sidebarToggled()`; update `topbar.component.spec.ts` for the mobile-toggle branch (depends on T016)
- [X] T018 [US3] Update `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` CSS: at `<768px` (matching `LAYOUT_COLLAPSE_BREAKPOINT`'s sibling `MOBILE_BREAKPOINT` from T004) ensure `width` uses the expanded value even when the global `collapsed` signal is true (drawer always shows full labels), and verify no horizontal overflow contribution — this is a styles-only change, add a regression case to `sidebar.component.spec.ts` asserting expanded rendering under a mobile host class (depends on T016)

**Checkpoint**: US1 + US2 + US3 independently functional — shell adapts correctly at all supported widths

---

## Phase 6: User Story 4 - Waiting and emptiness look intentional (Priority: P4)

**Goal**: Confirm and lock in the initial-load skeleton (Phase 1) plus the already-existing shared `LoadingStateComponent`/`EmptyStateComponent` are the only loading/empty presentations in the shell — this phase is verification + closing the one remaining gap (tenant-select's inline empty-state markup)

**Independent Test**: Reload on a throttled connection (skeleton, no blank screen, no flash); visit a page with no data (consistent empty state)

### Implementation for User Story 4

- [X] T019 [US4] Replace the hand-written empty-state markup in `frontend/apps/dashboard/src/app/features/tenant/tenant-select/tenant-select.component.ts` (icon + `<h2>` + `<p>` + link, currently bespoke) with `<app-empty-state>` from `frontend/apps/dashboard/src/app/shared/components/empty-state/empty-state.component.ts`, projecting the "Back to tenant area" link as its action content; update the component's spec if present, asserting `app-empty-state` renders with the expected title/description
- [X] T020 [P] [US4] Audit `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-base.component.ts` (already imports `EmptyStateComponent`) and the other seven tenant pages for any remaining ad hoc "no data" markup that duplicates `app-empty-state`'s pattern; replaced any found (if none are found beyond knowledge-base's existing correct usage, this task closes as a no-op verification — record the finding in the task's commit message)

**Checkpoint**: All four user stories independently functional — skeleton verified via quickstart §5, empty states consistent across pages

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates and end-to-end validation

- [X] T021 [P] Run `pnpm ng build dashboard` from `frontend/` — confirm no new bundle-budget regressions beyond the pre-existing warning noted in `specs/008-rbac-permissions` convergence
- [X] T022 [P] Run `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` from `frontend/` — all green
- [ ] T023 **(manual — pending PR)** Execute `specs/009-dashboard-shell/quickstart.md` §1–§6 manually (role-appropriate shell across tenant/platform roles, breadcrumbs, responsive drawer down to 360px, throttled-reload skeleton, empty states, light/dark theme pass) and record results in the PR description

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

---

## Phase 9: Convergence

**Purpose**: Close the one residual gap from Phase 8. Phase 8's T024, T025, and T027 are fully verified resolved (`--app-text-2` repoint, `--app-fill-hover` defined for both themes, budget raised so the build emits no warning). T026 was only partially effective — the fix corrected the topbar title and final crumb but introduced a leading duplicate. All quality gates remain green (build ✓, 197 tests ✓, lint ✓, format ✓).

- [X] T028 Fix the platform breadcrumb duplication: `/platform/overview-placeholder` currently renders **"Platform / Platform / Platform overview"** because the area label ("Platform", from the first path segment) and the platform base route's `pageTitle: 'platform'` (`frontend/apps/dashboard/src/app/app.routes.ts:32`) both produce a "Platform" crumb, on top of the child's "Platform overview". Remove `pageTitle: 'platform'` from the platform base route in `app.routes.ts` (the area label already supplies "Platform"; the base route always redirects to `overview-placeholder`, and the deepest-route topbar title resolves to `platformOverview`, so nothing else regresses) — or add dedup in `frontend/apps/dashboard/src/app/core/router/breadcrumb.ts` to skip a leading page crumb whose label equals the area label. Add a breadcrumb spec case that mirrors the real two-`pageTitle` platform nesting so the regression is caught. Target the contract's "Platform / Platform overview" per FR-007 / `contracts/ui-shell.md` (partial)

---

## Phase 10: Convergence

**Purpose**: Close residual responsive, skeleton, safe-state, and interaction-contract gaps found by assessing the current implementation against the feature artifacts.

- [X] T029 Force the mobile drawer to render the sidebar expanded even when the session-scoped desktop sidebar state is collapsed (for example, pass an effective non-collapsed value from `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts` while `isMobile()`), and add regression coverage asserting brand, group, and navigation labels remain visible in drawer mode per FR-009 / US3/AC2 (contradicts)
- [X] T030 Condense `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts` below 768px so the shell has no horizontal overflow at 360px while the tenant switcher, theme control, and avatar remain reachable; hide or compact visual-only/title controls as needed and add responsive regression coverage per FR-010 / SC-004 (partial)
- [X] T031 Add neutral, noninteractive avatar and tenant/platform-control silhouettes to the pre-bootstrap topbar skeleton in `frontend/apps/dashboard/src/index.html`, with responsive behavior and no entitlement-bearing text, per FR-012 / US4/AC1 (partial)
- [X] T032 Make `frontend/apps/dashboard/src/app/features/tenant/tenant-select/tenant-select.component.ts` render truthful no-access copy for users with no platform role or tenant memberships and omit the looping "Back to tenant area" action while preserving account-menu sign-out access; add component coverage per FR-015 / no-role edge case (partial)
- [X] T033 Close `drawerOpen` in `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.ts` when a resize crosses from mobile to 768px or wider, and add a store regression test for open drawer → desktop resize → closed per `contracts/ui-shell.md` responsive contract (missing)
- [X] T034 Constrain the pre-bootstrap content placeholder in `frontend/apps/dashboard/src/index.html` to the contracted 1320px maximum width with shared-container-equivalent centering and padding per `contracts/ui-shell.md` skeleton contract (partial)
- [X] T035 Change the Platform overview destination in `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts` to the canonical `/${APP_PATHS.platform.base}` path and update its navigation spec per `contracts/ui-shell.md` platform-nav contract (contradicts)

- [X] T036 Move `<app-user-menu />` to the far-right position in `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts` while preserving platform-control/tenant-switcher adjacency, and update ordering coverage per `contracts/ui-shell.md` avatar-menu contract (contradicts)
- [X] T037 Correct `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts` dropdown accessibility: add `aria-haspopup="menu"`, render the panel as `role="menu"`, expose destinations as menu items, and implement sensible open/close focus behavior with component coverage per `contracts/ui-shell.md` platform-nav contract (partial)

---

## Phase 11: Convergence

**Purpose**: Close residual automated-acceptance, session-safety, responsive, shared-pattern, and layout-consistency gaps found after Phase 10.

- [X] T038 CRITICAL Add automated browser-level dashboard-shell acceptance coverage for role-appropriate tenant/platform surfaces, breadcrumb ancestor navigation, drawer close paths and control reachability without horizontal overflow at 360px, pre-bootstrap skeleton replacement without entitlement flash, and light/dark/live-system-theme rendering per Constitution VII / SC-003 / SC-004 / SC-005 / SC-006 (missing)
- [X] T039 Make `frontend/apps/dashboard/src/app/core/auth/auth.service.ts` clear local identity and tenant state and navigate to the sign-in route even when the server logout request fails or reports an expired session; add regression tests for rejected and already-invalid logout responses per FR-006 / US1/AC4 (contradicts)
- [X] T040 Replace the narrow-screen topbar behavior in `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts` with a compact arrangement that prevents platform-user overflow while keeping tenant switching, platform navigation, theme selection, and account access visibly operable at 360px; replace stylesheet/DOM-presence assertions with rendered visibility, activation, and `scrollWidth <= clientWidth` coverage per FR-010 / FR-011 / SC-004 (contradicts)
- [X] T041 Integrate `frontend/apps/dashboard/src/app/shared/components/loading-state/loading-state.component.ts` into data-backed routed-page loading branches and add shared-component plus page-level loading-to-content/empty regression coverage per US4/AC2 / FR-013 / FR-014 / SC-007 (missing)
- [X] T042 Add `PageHeaderComponent` as the first child of the shared page container in `frontend/apps/dashboard/src/app/features/tenant/tenant-select/tenant-select.component.ts`, using the canonical Select Tenant title and appropriate description, with component coverage per FR-008 / US2/AC3 (missing)
- [X] T043 Make `frontend/apps/dashboard/src/app/layout/breadcrumb/breadcrumb.component.ts` handle deep trails and long labels accessibly on narrow viewports without clipping or shell horizontal overflow, and add long-label/multi-ancestor responsive regression coverage per FR-007 / FR-010 (partial)
- [X] T044 Align the shell breadcrumb content to the same centered 1320px maximum-width column and horizontal padding as `PageContainerComponent`, preferably through the existing shared layout primitive or tokens rather than duplicated layout values, per FR-008 / Constitution IX (partial)

---

## Phase 12: Convergence

**Purpose**: Replace simulated acceptance checks and inert presentation branches with real browser and data-lifecycle coverage, and complete shared empty-state adoption.

- [X] T045 CRITICAL Add a real browser-run E2E setup and dashboard-shell suite covering tenant, platform, and no-role identities; tenant switching; breadcrumb navigation; drawer dismissal/navigation; actual 360px overflow and essential-control activation; delayed identity resolution from pre-bootstrap skeleton to shell without entitlement flash; and light/dark/live-system-theme behavior, and include it in the frontend quality gates per Constitution VII / T038 / SC-004 / SC-005 / SC-006 (contradicts)
- [X] T046 Replace the permanently-false local loading signals added to routed tenant pages with loading state driven by their actual store/service/resolver lifecycle, including initial and tenant-change loads where applicable, and add lifecycle-driven pending-to-content and pending-to-empty tests rather than directly mutating presentation signals per US4/AC2 / FR-013 / T041 (partial)
- [X] T047 Render the shared `EmptyStateComponent` for zero-data and zero-filter-result branches in the customer and conversation list views, with context-appropriate reset or create actions where applicable, and add regression coverage for each branch per US4/AC3 / FR-013 / FR-014 / SC-007 (partial)

---

## Phase 13: Convergence

**Purpose**: Close residual browser-gate enforcement, acceptance coverage, routed-data lifecycle, breadcrumb navigation, and shared empty-state interaction gaps found after Phase 12.

- [X] T048 CRITICAL Install the Playwright browser in `.github/workflows/frontend.yml`, run `pnpm test:e2e` as a required frontend CI step, and align the canonical frontend quality-gate documentation so browser acceptance coverage cannot be bypassed per Constitution VII / T045 (missing)
- [X] T049 Extend `frontend/e2e/dashboard-shell.spec.ts` to activate breadcrumb ancestors and platform destinations, verify successful and failed/expired logout, cover refresh and tenant-switch loading/entitlement transitions without stale-control flash, and assert light/dark shell and shared-state surfaces are visibly styled rather than checking only `data-theme` per T045 / SC-003 / SC-004 / SC-005 / SC-006 (partial)
- [X] T050 Replace the inert `RoutedPageDataService.load(): Promise<void>` and duplicate component-side `load(null)` calls with a single typed, tenant-aware routed-page lifecycle that owns pending/data/empty/error state, preserves tenant context, and reloads on tenant changes; add initial-load and tenant-A-to-tenant-B pending-to-content/empty tests for customers and conversations per T046 / US4/AC2 / FR-013 (partial)
- [X] T051 Give Workspace and Platform area-root crumbs canonical navigable targets in `frontend/apps/dashboard/src/app/core/router/breadcrumb.ts`, then add component and Playwright coverage that activates each ancestor and verifies the destination URL per FR-007 / US2/AC2 / SC-003 (contradicts)
- [X] T052 Add shared `EmptyStateComponent` zero-data branches to every applicable data-backed routed page, distinguish initial emptiness from zero-filter-results such as in knowledge base, provide reset/create actions only where supported, and add page-level regression coverage per FR-013 / FR-014 / SC-007 (partial)
- [X] T053 Wire the customer `Create customer` and conversation `Start conversation` empty-state controls to supported routes or commands, or remove them when creation is unavailable, and test activation and resulting behavior so no inert action is presented per T047 / US4/AC3 / Constitution IX (partial)

---

## Phase 14: Convergence

**Purpose**: Replace the residual inert and race-prone routed-data implementation with a typed tenant-safe lifecycle, restore truthful page states, and complete the required component and browser regressions.

- [X] T054 CRITICAL Make `RoutedPageStore` latest-tenant-wins by cancelling or generation-tagging requests, clearing prior tenant data immediately when a new load begins, and preventing an older request's success, failure, or completion path from mutating the newer request's state; add deterministic A-pending → B-pending → B-resolves → A-resolves/rejects-late tests proving only tenant B remains per SC-005 / Constitution II (contradicts)
- [X] T055 Replace `RoutedPageDataService.load(): Promise<unknown>` and the `unknown`/`undefined` store sentinels with typed page payload contracts and a discriminated `pending | data | empty | error` lifecycle, supplying each page from its intended fixture or service source with no component casts per T050 / US4/AC2 / FR-013 (contradicts)
- [X] T056 Restore each routed page's intended production content through the typed data source, derive genuine zero-data and zero-filter-result branches from loaded payloads, and render a shared visible error state with retry behavior instead of treating errors or `undefined` as emptiness; cover pending-to-content, pending-to-empty, and pending-to-error on every applicable page per T052 / FR-013 / SC-007 (contradicts)
- [X] T057 Add customer and conversation component tests driven by active-tenant changes covering tenant A content → tenant B pending with A content removed → tenant B content or empty, including out-of-order completion assertions that stale tenant A content cannot reappear per T050 / Constitution VII (missing)
- [X] T058 Complete `frontend/e2e/dashboard-shell.spec.ts` with authenticated refresh without entitlement flash; expired-session 401 logout with local-state clearing; delayed tenant A → tenant B routed-data switching that visibly enters the shared loading state, removes A content, and renders B content/empty without stale reappearance; and computed-style/legibility assertions in both themes for sidebar, topbar, breadcrumb, menus, page header/container, loading state, and empty state per T049 / SC-005 / SC-006 / FR-006 (partial)

---

## Phase 15: Convergence

**Purpose**: Replace the remaining placeholder routed-data path with truthful typed page payloads, make tenant transitions and retries safe, and strengthen browser and accessibility acceptance evidence.

- [X] T059 Replace `RoutedPageDataService.load(): Promise<unknown>` with page-specific typed payload contracts and intended tenant-aware data sources; type `RoutedPageStore` without `unknown` sentinels, remove component casts and direct production fixture rendering, and derive each page's data, initial-empty, filtered-empty, and error branches from its loaded payload, including a functional knowledge-base filter reset per T055 / T056 / FR-013 (contradicts)
- [X] T060 Add a store-owned retry operation that reuses the latest active tenant and generation, replace every routed page's `load(null)` retry, and prove retry after tenant A → tenant B cannot request tenant-neutral or stale tenant-A data per T050 / T056 / SC-005 (contradicts)
- [X] T061 Add pending-to-content, pending-to-empty, pending-to-error, and successful tenant-preserving retry component coverage for overview, AI agent, knowledge base, integrations, analytics, settings, customers, and conversations, including concrete typed payload assertions and zero-filter reset behavior where applicable per T056 / SC-007 (partial)
- [X] T062 Strengthen customer and conversation active-tenant component tests to assert tenant A content is removed immediately while tenant B is pending, the shared loading state is visible, tenant B can resolve to content or empty, and late tenant-A success or rejection cannot mutate tenant-B state per T057 (partial)
- [X] T063 Replace the post-bootstrap `.skeleton-shell` tenant-switch E2E expectation with a real two-tenant routed-data scenario that renders tenant A content, visibly enters the shared in-app loading state with A removed, resolves tenant B to content or empty, and proves late tenant-A completion cannot reappear per T058 / SC-005 (contradicts)
- [X] T064 Make refresh and logout browser acceptance deterministic by delaying `/me` during authenticated reload to assert the neutral skeleton and absence of entitlement controls, and by configuring unshadowed 401/500 logout responses before asserting local-state clearing and sign-in navigation per T049 / T058 / FR-006 / SC-005 (partial)
- [X] T065 Target sidebar, topbar, breadcrumb, menus, page header, page container, loading state, and empty state directly in Playwright and assert meaningful computed foreground/background/border styling and legibility in both light and dark themes, including live-system-theme transitions while shared states are visible per T058 / SC-006 (partial)
- [X] T066 Complete accessible popup behavior for `user-menu.component.ts` and `tenant-switcher.component.ts`: coherent menu/listbox semantics, menu items, trigger and option focus management, Escape and outside-click dismissal, and keyboard/browser regression coverage per FR-006 / FR-014 / plan: accessible dropdown decision (partial)
- [X] T067 Run the canonical `pnpm test:e2e` package script in `.github/workflows/frontend.yml` instead of invoking Playwright directly so CI cannot bypass future quality-gate script changes per T048 (partial)

---

## Phase 16: Convergence

**Purpose**: Align the routed-page lifecycle with the current frontend constitution and replace tenant-neutral fixture loading with truthful tenant-scoped data sources.

- [X] T068 CRITICAL Convert `RoutedPageDataService` and `RoutedPageStore` from the synthetic Promise/`async` lifecycle to typed RxJS observable sources and operator composition (including latest-tenant cancellation, lifecycle mapping, error handling, and tenant-preserving retry), and update store/component tests to prove cancellation and stale-result suppression per Constitution Frontend asynchronous-logic mandate (contradicts)
- [X] T069 Replace the ignored `tenantId` and direct production fixture returns in `frontend/apps/dashboard/src/app/features/tenant/routed-page-data.service.ts` with each page's intended tenant-scoped source, preserving typed data/empty/error states and adding distinct tenant A/tenant B source assertions; if real page APIs are unavailable, constrain and document the scaffolding so it does not claim tenant-dependent production data per T055 / T059 / FR-013 (contradicts)

---

## Phase 17: Convergence

**Purpose**: Close residual constitution compliance, tenant-data truthfulness, browser acceptance, theme evidence, and popup-focus gaps found after Phase 16.

- [ ] T070 CRITICAL Refactor `frontend/apps/dashboard/src/app/layout/topbar/tenant-switcher.component.ts` loading and tenant-selection orchestration to RxJS observable/operator composition with cancellation and localized error handling; remove `firstValueFrom` and component-level Promise flows except at inherently Promise-based boundaries, and update component coverage per Constitution frontend asynchronous-logic mandate (contradicts)
- [ ] T071 Replace `frontend/apps/dashboard/src/app/features/tenant/routed-page-data.service.ts` fixture dispatch with truthful tenant-scoped page sources and tests asserting tenant IDs propagate to each source; if APIs remain unavailable, make the source explicitly tenant-neutral scaffolding, remove magic tenant-ID behavior, and document that it must not represent production tenant-dependent data per T069 / FR-013 (contradicts)
- [ ] T072 Add a real browser-level two-tenant routed-page scenario with controllable tenant-scoped data sources: render tenant A content, switch to tenant B, assert the shared loading state and immediate removal of tenant A, resolve tenant B to content and empty variants, then release or reject tenant A late and assert it never reappears per T058 / T063 / SC-005 / Constitution VII (missing)
- [ ] T073 Strengthen `frontend/e2e/dashboard-shell.spec.ts` theme coverage by holding a routed-page load pending and directly targeting `app-loading-state`, `app-empty-state`, and `app-page-container`; assert meaningful foreground, background, border, and legibility behavior in light, dark, and live-system-theme transitions while each shared state is visible per T058 / T065 / FR-011 / FR-013 / SC-006 (partial)
- [ ] T074 Move focus to the first platform menu item whenever `frontend/apps/dashboard/src/app/layout/topbar/platform-nav.component.ts` opens, preserve trigger focus on Escape, dismissal, and selection, and add component plus Playwright keyboard-focus regressions per T037 / plan: accessible dropdown decision / FR-014 (contradicts)
- [ ] T075 Complete `frontend/apps/dashboard/src/app/layout/topbar/tenant-switcher.component.ts` listbox focus management: retain a trigger reference, restore trigger focus after Escape, dismissal, and selection, implement keyboard traversal and selection with coherent active-option semantics, and add component plus Playwright keyboard regressions per T066 / FR-014 (partial)
