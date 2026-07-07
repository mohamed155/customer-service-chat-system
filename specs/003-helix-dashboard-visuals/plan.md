# Implementation Plan: Helix Admin Dashboard Visual System

**Branch**: `003-helix-dashboard-visuals` | **Date**: 2026-07-06 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-helix-dashboard-visuals/spec.md`

## Summary

Transform the spec-002 dashboard foundation into the full Helix Admin visual system: replace the current placeholder tokens/theme values with the Helix palette and layout constants (verified against `Helix Admin.html` — the reference's CSS variables match the spec values exactly), self-host the Geist font via Fontsource, rebuild the shell (248/68px sidebar with grouped nav + user footer, 60px topbar with route-driven title/subtitle, search, cycling theme toggle, notifications, "New" button), create the reusable UI and AI-specific visual components as standalone, token-consuming wrappers (Taiga UI inside project components), and compose nine visual pages (Overview, Conversations, Customers, AI Agent, Knowledge Base, Integrations, Analytics, Settings, plus four auth screens) from typed fixture data. Global theme/sidebar state stays in the existing `appUi` NgRx slice; page-local UI state (conversation selection/filters, tabs, date range) uses NgRx SignalStores. No backend calls, no chart library, no persistence beyond the existing theme-mode storage.

## Technical Context

**Language/Version**: TypeScript ~6.0.3 on Angular 22.0.5 (standalone components, signals, zoneless, OnPush, built-in control flow)

**Primary Dependencies**: Existing — `@angular/*` 22.0.5, NgRx 21.1.1 (Store/Effects/Signals), Taiga UI 5.13.0 (`core`, `kit`, `icons`, `styles`), RxJS 7.8. New — `@fontsource/geist-sans` (+ `@fontsource/geist-mono` for the prompt editor/API key fields), openly licensed (SIL OFL), self-hosted via npm; no other new runtime dependencies (charts are hand-built inline SVG)

**Storage**: Browser localStorage (theme preference only — unchanged from spec 002); all page data from static TypeScript fixtures

**Testing**: Vitest 4 via `@angular/build:unit-test` + jsdom; Angular TestBed with `provideStore`/SignalStore instances; behavior-focused component tests (no snapshot-only tests)

**Target Platform**: Evergreen desktop/laptop/tablet browsers (≥768px usable); served by Angular dev server locally

**Project Type**: Web application — `frontend/` pnpm workspace, all work inside `apps/dashboard`; `apps/widget` and `libs/*` untouched (prior scaffolding, per CLAUDE.md)

**Performance Goals**: All interactions (theme cycle, sidebar collapse, conversation select, tab switch, filters) complete in a single zoneless change-detection pass with no network activity; lazy route chunks per page; inline SVG charts render without runtime chart computation

**Constraints**: No backend/business logic, no real auth, no chart library, no persistence beyond theme mode, no copying markup/assets/fonts out of `Helix Admin.html`, no raw Taiga styling scattered in pages, route paths only via `APP_PATHS`, quality gates (`pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`) all green

**Scale/Scope**: 12 routes (8 tenant pages + 4 auth screens), ~7 layout components, ~13 shared UI components, ~8 AI-specific visual components, 6 fixture files, 1 global NgRx slice (reused), ≥3 SignalStores, token layer rewrite, ~15 new/updated test suites

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Enterprise Modular Monolith | ✅ PASS | Work stays inside the `core/shared/layout/features/design-system` layering; each page remains an isolated lazy boundary; fixtures live in `shared/fixtures` behind mock-named providers so backend integration swaps data sources, not pages |
| II | Multi-Tenant Isolation | ✅ PASS | Visual-only; no tenant data handled; no isolation claims made in frontend |
| III | Zero-Trust Security & RBAC | ✅ PASS | No auth logic added (auth screens are visual; submit does nothing); no secrets; masked API key is a fixture string |
| IV | AI Provider Independence | ✅ N/A | AI components are visual-only (badges, suggestion cards, timelines); no LLM access |
| V | API-First & Contract Consistency | ✅ PASS | No API surface added; fixture types kept structurally compatible with spec-001 REST contract naming so later wiring is a data-source swap |
| VI | Observability by Default | ✅ N/A | No requests issued; existing foundations untouched |
| VII | Test-First & Regression Discipline | ⚠️ DEVIATION | Unit/component behavior tests required for shell, state, and each page's key interaction; E2E still deferred (no real flows exist yet) — carried over from spec 002, justified in Complexity Tracking |
| VIII | Database Integrity | ✅ N/A | No database access |
| IX | Design System Discipline | ✅ PASS | Order enforced by phases: Helix tokens first (T003–T005), then reusable components (T019–T027), then pages (T028+); no page-specific one-off styling of shared concerns; Taiga wrapped in project components per R7 |
| X | Performance & Efficiency | ✅ PASS | Zoneless + OnPush, signals throughout, lazy pages, static SVG charts, no new heavy dependencies |

**Post-Phase-1 re-check**: Design artifacts introduce no new violations. The single carried-over Test deviation remains justified below. Gate passes.

## Project Structure

### Documentation (this feature)

```text
specs/003-helix-dashboard-visuals/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output (fixture models + UI state)
├── quickstart.md        # Phase 1 output (validation guide)
├── contracts/
│   └── ui-contract.md   # Routes/titles, token contract, component APIs, state contract
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
frontend/apps/dashboard/src/
├── styles.css                          # updated: imports token files + fontsource Geist
└── app/
    ├── app.routes.ts                   # updated: tenant/auth real routes, platform kept in shell
    ├── core/
    │   ├── router/
    │   │   ├── app-paths.ts            # updated: 8 tenant + 4 auth path constants
    │   │   └── page-title.ts           # NEW: route data → {title, subtitle} mapping + resolver helper
    │   └── state/
    │       └── app-ui.feature.ts       # reused as-is (themeMode, sidebarCollapsed)
    ├── design-system/
    │   ├── tokens/
    │   │   └── tokens.css              # rewritten: Helix layout/spacing/radius/shadow/z/typography/transition tokens
    │   └── theme/
    │       └── themes.css              # moved+rewritten: Helix light/dark palettes on [data-theme]
    ├── layout/
    │   ├── app-shell/                  # updated: fixed-viewport shell, independent scroll areas
    │   ├── sidebar/
    │   │   ├── sidebar.component.*     # rewritten: brand, grouped nav, badge, footer, collapse
    │   │   ├── sidebar-nav-group.component.ts   # NEW
    │   │   └── sidebar-nav-item.component.ts    # NEW
    │   ├── topbar/                     # rewritten: title/subtitle, search, theme cycle, bell, New
    │   ├── page-container/             # NEW: max-width + page padding wrapper
    │   └── page-header/                # updated to Helix style (in-page section headers)
    ├── shared/
    │   ├── components/
    │   │   ├── dashboard-card/         # NEW  ├── metric-card/       # NEW
    │   │   ├── status-badge/           # NEW  ├── channel-badge/     # NEW
    │   │   ├── sentiment-badge/        # NEW  ├── avatar/            # NEW
    │   │   ├── search-input/           # NEW  ├── icon-button/       # NEW
    │   │   ├── empty-state/            # NEW  ├── loading-state/     # NEW (absorbs loading-indicator)
    │   │   ├── section-header/         # NEW  ├── toolbar/           # NEW
    │   │   ├── data-table/             # NEW  ├── sparkline/         # NEW (inline SVG)
    │   │   └── ai/
    │   │       ├── ai-confidence-badge/  ai-suggestion-card/  ai-thinking-indicator/
    │   │       ├── ai-tool-timeline/     knowledge-citation/  escalation-banner/
    │   │       └── agent-preview-card/   prompt-editor-shell/          # all NEW, visual-only
    │   └── fixtures/
    │       ├── conversation.fixtures.ts  customer.fixtures.ts   analytics.fixtures.ts
    │       ├── knowledge.fixtures.ts     integration.fixtures.ts settings.fixtures.ts
    │       └── fixture.models.ts       # NEW: shared fixture types (data-model.md)
    └── features/
        ├── auth/
        │   ├── auth.routes.ts          # updated: login/signup/forgot-password/verify-email
        │   ├── auth-card/              # NEW: shared centered auth layout component
        │   ├── login/ signup/ forgot-password/ verify-email/   # NEW pages
        ├── platform/                   # placeholder content kept; route moved under Helix shell title
        └── tenant/
            ├── tenant.routes.ts        # updated: 8 pages, lazy loadComponent each
            ├── overview/               # NEW: alert banner, 5 metric cards, trend chart, donut, activity
            ├── conversations/          # NEW: inbox-list / thread / customer-sidebar + conversations.store.ts (SignalStore)
            ├── customers/              # NEW: toolbar + data table
            ├── ai-agent/               # NEW: tabs (Behavior/Prompt/Escalation/Testing) + ai-agent.store.ts
            ├── knowledge-base/         # NEW: cards + filters + empty state
            ├── integrations/           # NEW: card grid
            ├── analytics/              # NEW: filters + metric cards + SVG charts + table
            └── settings/               # NEW: tabs (General/Team/Billing/API Keys/Security) + settings.store.ts
```

**Structure Decision**: Extend the existing spec-002 layered structure in place — no new apps or libs. The token layer keeps the established `--app-*` prefix and two-file layout (`design-system/tokens/tokens.css` for theme-independent values, `design-system/theme/themes.css` for the light/dark palettes), rewritten with Helix values, rather than the seven-file split suggested in the feature input (the spec explicitly allows "equivalent structure if the current frontend architecture already has a token system"). Old placeholder components (`tenant/overview-placeholder`, `auth/login-placeholder`) are deleted; `platform/overview-placeholder` survives unchanged but renders under the new shell.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Principle VII: E2E test category still absent (unit/component tests only) | This spec ships visual composition over fixtures; there is still no real user flow (no auth, no data mutation) for an E2E suite to protect | Adding Playwright now would test static fixtures through a browser at high maintenance cost with no regression value; E2E lands with the first real flow (next spec: real authentication UI integration) |
