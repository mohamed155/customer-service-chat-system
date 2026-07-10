# Implementation Plan: Dashboard Shell

**Branch**: `009-dashboard-shell` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/009-dashboard-shell/spec.md`

## Summary

Consolidate the authenticated dashboard layout into a complete, role-aware shell: a header avatar menu becomes the single real-identity surface (the fixture sidebar footer card is removed), platform destinations move to a permission-gated header control beside the existing tenant switcher, every page gains breadcrumbs plus a shared page-header/container pattern, the sidebar gains an overlay-drawer mode for narrow viewports, and the identity-resolution window is covered by a pre-bootstrap static skeleton so the shell frame paints immediately with no blank screen and no entitlement flash.

Technical approach: frontend-only. The shell keeps the existing blocking `provideAppInitializer` (`/me` resolves before Angular renders — this is what already guarantees zero entitlement flash) and pairs it with a static skeleton + theme-restoring inline script in `index.html` to satisfy the immediate-paint requirement. New layout components (`user-menu`, `platform-nav`, `breadcrumb`) follow the established hand-rolled dropdown pattern from `tenant-switcher`; responsive behavior extends the existing `LayoutStore` signal store with a mobile breakpoint and drawer state. Identity/role display derives from `CurrentUserService` + the NgRx `tenantContext` slice; visibility gating reuses `PermissionsService` (presentation-only — enforcement stays server-side per feature 008).

## Technical Context

**Language/Version**: TypeScript ~6.0, Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Angular Router; NgRx 21 (existing `appUi` + `tenantContext` Store slices, SignalStore for `LayoutStore`); Taiga UI 5 (icons via `TuiIcon`; interactive dropdowns follow the existing hand-rolled `tenant-switcher` pattern wrapped in project components); existing `core/authz` (PermissionsService, PAGE_PERMISSIONS)

**Storage**: No backend/schema changes. localStorage only: existing `app.themeMode` (persisted theme, also read by the new pre-boot inline script) and `app.tenant` (existing). Sidebar/drawer state stays session-scoped in memory per the established state rules.

**Testing**: Vitest via `pnpm ng test dashboard`; quality gates `pnpm ng build dashboard`, `pnpm lint`, `pnpm format:check` (run in `frontend/`)

**Target Platform**: Evergreen browsers, viewports from ~360px (small phones) to desktop

**Project Type**: Web application — Angular dashboard app inside the existing pnpm workspace (`frontend/apps/dashboard`); zero backend changes (consumes the existing `GET /me` contract from feature 008 unchanged)

**Performance Goals**: First visual paint is the static `index.html` skeleton (no blank screen, no JS needed); no network requests added beyond the existing `/me` initializer; drawer/sidebar transitions use CSS only (SC-004/SC-005)

**Constraints**: No frontend role→permission mapping (feature 008 FR-010 — role *display names* are presentation and allowed, permission sets are not); permission checks in the shell are presentation-only (Constitution II); `apps/widget` and `libs/*` untouched; no raw Taiga styling in feature pages (spec 003 rule); `--app-*` design tokens only

**Scale/Scope**: 3 new layout components (user menu, platform nav control, breadcrumb), 1 new shared component (page container), 1 static skeleton, ~4 modified layout pieces (shell, sidebar, topbar, layout store), ~10 pages migrated to the shared header/container pattern, 1 fixture removal (`SIDEBAR_USER`)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Backend untouched; frontend keeps the spec-002 layering (`layout/` for shell chrome, `shared/` for reusable presentation, `core/` for singletons) | ✅ Pass |
| II. Multi-Tenant Isolation | All shell visibility checks (platform control, switcher, nav) are presentation-only; server enforcement from feature 008 is unchanged and remains the boundary | ✅ Pass |
| III. Zero-Trust Security & RBAC | No new endpoints; identity display and sign-out reuse the audited session flows from feature 007; no secrets involved | ✅ Pass |
| IV. AI Provider Independence | Not touched | ✅ N/A |
| V. API-First & Contract Consistency | Consumes the existing `/me` contract unchanged; UI contracts documented in `contracts/ui-shell.md` | ✅ Pass |
| VI. Observability by Default | No request-path changes; existing interceptors (request-id, error mapping) untouched | ✅ Pass |
| VII. Test-First & Regression Discipline | Component/guard/store specs required per user story (quickstart §1); sidebar-footer removal and fixture deletion covered by updated specs | ✅ Pass |
| VIII. Database Integrity | No schema changes | ✅ N/A |
| IX. Design System Discipline | Tokens exist (spec 003) → this feature builds components (user menu, breadcrumb, page container) → pages consume patterns; removes a UI-logic duplication (identity shown from fixture + real data in two places) | ✅ Pass |
| X. Performance & Efficiency | Static skeleton improves first paint; zero added queries; CSS-only transitions | ✅ Pass |

**Initial gate**: PASS — no violations, Complexity Tracking not required.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no deviations; the one boot-strategy decision (keep blocking initializer + static skeleton, research R1) trades a small amount of duplicated static markup for guaranteed zero entitlement flash, which is the stricter reading of Constitution II.

## Project Structure

### Documentation (this feature)

```text
specs/009-dashboard-shell/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── ui-shell.md      # Shell component/behavior contracts (nav surfaces, breadcrumb data, drawer, skeleton)
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
frontend/apps/dashboard/src/
├── index.html                                  # MODIFIED — pre-boot skeleton markup + theme-restoring inline script
└── app/
    ├── core/
    │   ├── identity/
    │   │   └── role-display.ts                 # NEW — role code → display name map + context-aware role label helper
    │   └── router/
    │       ├── app-paths.ts                    # MODIFIED if needed — platform destination constants
    │       └── breadcrumb.ts                   # NEW — route-data-driven crumb trail derivation (injectBreadcrumbs)
    ├── layout/
    │   ├── app-shell/
    │   │   ├── app-shell.component.ts          # MODIFIED — drawer mode, scrim, breadcrumb slot
    │   │   └── layout.store.ts                 # MODIFIED — mobile breakpoint + drawer open/close state
    │   ├── breadcrumb/
    │   │   └── breadcrumb.component.ts         # NEW — trail rendering, ancestor links
    │   ├── sidebar/
    │   │   └── sidebar.component.ts            # MODIFIED — footer user card removed, drawer-aware, close-on-navigate
    │   └── topbar/
    │       ├── topbar.component.ts             # MODIFIED — avatar menu + platform nav mounted; bare sign-out icon removed
    │       ├── user-menu.component.ts          # NEW — avatar trigger + identity dropdown + sign-out
    │       ├── platform-nav.component.ts       # NEW — permission-gated platform destinations dropdown
    │       └── tenant-switcher.component.ts    # UNCHANGED (already platform-only)
    ├── shared/
    │   ├── components/
    │   │   └── page-container/
    │   │       └── page-container.component.ts # NEW — max-width/padding wrapper for all pages
    │   └── fixtures/settings.fixtures.ts       # MODIFIED — SIDEBAR_USER fixture deleted
    └── features/
        ├── tenant/**                           # MODIFIED — pages adopt page-container + page-header pattern
        └── platform/**                         # MODIFIED — same adoption
```

**Structure Decision**: Existing Angular workspace layout (spec 002 layering) — shell chrome in `layout/`, reusable presentation in `shared/components/`, route/identity utilities in `core/`. No new top-level units; no backend directories touched.

## Complexity Tracking

No constitution violations — table intentionally empty.
