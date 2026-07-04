# Implementation Plan: Angular Frontend Foundation

**Branch**: `002-angular-frontend-foundation` | **Date**: 2026-07-04 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-angular-frontend-foundation/spec.md`

## Summary

Build the real frontend foundation for the dashboard application inside the existing `frontend/` pnpm workspace (kept per user decision): modernize the workspace tooling to current Angular 22 conventions (new `@angular/build` builder, Vitest unit testing, zoneless change detection, Prettier, angular-eslint — mirroring the reference app in `~/test-ng`), install Taiga UI 5 as the primary UI library and NgRx (Store/Effects/Signals), and implement the layered `core / shared / layout / features / design-system` structure with lazy `auth` / `platform` / `tenant` route areas, a dashboard shell (sidebar, topbar, content outlet), `--app-*` design tokens with light/dark/system theming, a global `appUi` NgRx slice (theme persisted to localStorage), typed HTTP/API models aligned with the backend REST contract from spec 001, error-handling and loading foundations, and tests for all of it.

## Technical Context

**Language/Version**: TypeScript ~6.0.3 on Angular 22.0.5 (standalone components, signals, zoneless change detection, built-in control flow)

**Primary Dependencies**: `@angular/*` 22.0.5, `@angular/build` 22.0.5 (replaces `@angular-devkit/build-angular`), NgRx 21.1.1 (`@ngrx/store`, `@ngrx/effects`, `@ngrx/signals`, `@ngrx/store-devtools` — peer-dependency allowance for Angular 22, see Complexity Tracking), Taiga UI 5.13.0 (`taiga-ui` meta-package via `ng add`), RxJS 7.8

**Storage**: Browser localStorage (theme preference only); no backend persistence in scope

**Testing**: Vitest 4 via `@angular/build:unit-test` builder with jsdom (replaces Karma/Jasmine, per `~/test-ng` reference); Angular TestBed for component tests

**Target Platform**: Evergreen desktop/mobile browsers; dashboard app served by Angular dev server locally

**Project Type**: Web application — existing pnpm workspace `frontend/` with `apps/dashboard` (this feature) and `apps/widget` + `libs/*` (retained untouched, prior scaffolding)

**Performance Goals**: Lazy route chunks per area (auth/platform/tenant); OnPush + zoneless throughout; theme/sidebar toggles render instantly (single change-detection pass); initial bundle excludes other areas' page code

**Constraints**: No business features (spec Non-Goals); no fake auth/token logic; no custom component library; no secrets in environment files; strict TS + strict templates; ESLint/Prettier clean

**Scale/Scope**: 1 app (dashboard), 3 lazy route areas + not-found page, 1 global NgRx slice, ~4 layout components, HTTP/error/loading foundations, ~10 test suites

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Enterprise Modular Monolith | ✅ PASS | Frontend layering enforces the same discipline: `core`/`shared` never depend on `features`; each route area is an isolated lazy boundary extractable later |
| II | Multi-Tenant Isolation | ✅ PASS | No tenant data handled; platform/tenant route split prepares contexts without frontend-only enforcement claims |
| III | Zero-Trust Security & RBAC | ✅ PASS | No auth implemented, no fake auth simulated; auth-header interceptor is a registered no-op awaiting the auth spec; no secrets in env files |
| IV | AI Provider Independence | ✅ N/A | No AI functionality in scope |
| V | API-First & Contract Consistency | ✅ PASS | `ApiError`/`PaginatedResponse`/`ApiListQuery` models mirror `specs/001-.../contracts/rest-api.md` (error envelope, cursor pagination, `X-Request-Id`) |
| VI | Observability by Default | ✅ PASS | Error model carries `requestId`; logging utility gives structured dev-readable output; foundation ready for tracing headers |
| VII | Test-First & Regression Discipline | ⚠️ DEVIATION | Reducer/selector/mapper/guard unit tests + shell/route component tests required by spec FR-024; E2E category deferred until real user flows exist — justified in Complexity Tracking |
| VIII | Database Integrity | ✅ N/A | No database access from frontend |
| IX | Design System Discipline | ⚠️ DEVIATION | Tokens-before-components honored (`--app-*` tokens first). But Taiga UI replaces "Angular Material or the project's own component library" and the spec-001 Helix `libs/ui` direction — user-confirmed decision, justified in Complexity Tracking |
| X | Performance & Efficiency | ✅ PASS | Zoneless + OnPush, lazy areas, signals/async-pipe only (no manual subscriptions), no global icon-set imports |

**Post-Phase-1 re-check**: No new violations introduced by the design artifacts. Gate passes with the one justified deviation below.

## Project Structure

### Documentation (this feature)

```text
specs/002-angular-frontend-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── frontend-foundation.md   # Route map, state contract, HTTP models contract
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
frontend/                          # existing pnpm workspace (kept)
├── package.json                   # updated: NgRx, Taiga UI, vitest, prettier, angular-eslint; pnpm peer rules
├── pnpm-lock.yaml
├── angular.json                   # dashboard migrated to @angular/build builders + vitest unit-test
├── tsconfig.base.json             # strict (already) — kept as workspace base
├── eslint.config.mjs              # upgraded to angular-eslint flat config
├── .prettierrc                    # copied/adapted from ~/test-ng
├── .editorconfig                  # copied/adapted from ~/test-ng
├── apps/
│   ├── widget/                    # untouched (prior scaffolding, out of scope)
│   └── dashboard/
│       ├── tsconfig.app.json
│       ├── tsconfig.spec.json
│       ├── public/                # static assets (favicon)
│       └── src/
│           ├── index.html
│           ├── main.ts            # zoneless standalone bootstrap
│           ├── styles.css         # imports tokens.css + themes.css
│           ├── environments/
│           │   ├── environment.ts              # production
│           │   └── environment.development.ts  # dev (fileReplacements)
│           └── app/
│               ├── app.component.ts        # <tui-root> + router outlet
│               ├── app.config.ts           # providers: router, http, store, effects, devtools, taiga, error handler
│               ├── app.routes.ts           # root redirect, area lazy-loads, not-found
│               ├── core/
│               │   ├── api/                 # api models + base api service
│               │   ├── config/              # environment access (injection token)
│               │   ├── errors/              # global error handler, error mapper, user-message util
│               │   ├── http/                # functional interceptors (error, auth-placeholder, request-id ready)
│               │   ├── logging/             # logger service
│               │   ├── router/              # typed route path constants, pass-through guard
│               │   └── state/               # store setup + appUi slice (actions/reducer/selectors/effects)
│               ├── layout/
│               │   ├── app-shell/           # grid shell, reads appUi from store
│               │   ├── sidebar/             # collapsible nav placeholder (Taiga buttons/icons)
│               │   ├── topbar/              # toggle buttons: sidebar, theme
│               │   └── page-header/         # reusable page title header
│               ├── shared/
│               │   ├── components/          # loading indicator (Taiga-based)
│               │   └── utils/               # type helpers as needed
│               ├── features/
│               │   ├── auth/                # auth.routes.ts + login-placeholder page
│               │   ├── platform/            # platform.routes.ts + overview-placeholder page
│               │   ├── tenant/              # tenant.routes.ts + overview-placeholder page
│               │   └── not-found/           # minimal not-found page
│               └── design-system/
│                   ├── tokens/tokens.css    # --app-* tokens (layout, spacing, radius, type, z-index, breakpoints, shadows)
│                   └── theme/themes.css     # light/dark values + data-theme + prefers-color-scheme
└── libs/                          # untouched (hx- UI lib + stubs from spec 001; see Complexity Tracking)
```

**Structure Decision**: Keep the existing `frontend/` pnpm workspace and its `apps/ + libs/` layout (user decision). The spec-002 `core/shared/layout/features/design-system` separation is implemented **inside `apps/dashboard/src/app/`**, which satisfies the spec's allowance that "the exact structure may be adjusted if CLI conventions require it, but the separation of concerns must remain". `apps/widget` and `libs/*` are not modified by this feature. Empty scaffold folders (`shared/directives`, `shared/pipes`) are created only when a real file lands in them (spec forbids unused placeholder files).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Taiga UI instead of constitution's "Angular Material or project's own component library" (Principle IX / stack section) | Explicit user decision in spec-002 input: Taiga UI is the primary UI library; building on the spec-001 Helix custom library now would violate spec-002's "no custom component library yet" non-goal | Angular Material rejected by user; continuing Helix `libs/ui` rejected because spec-002 forbids custom component library work in the foundation. Follow-up: amend constitution stack section to name Taiga UI |
| NgRx 21.1.1 on Angular 22 via pnpm peer-dependency allowance (`@ngrx/* → @angular/core 22`) | No NgRx release currently declares Angular 22 peer support (latest = 21.1.1 with `^21.0.0`); spec mandates both Angular 22 and NgRx | Downgrading to Angular 21 rejected (spec requires Angular 22); waiting for NgRx 22 rejected (blocks the foundation). NgRx 21 uses only stable Angular APIs (DI, signals); lockstep peer ranges are policy, not technical incompatibility. Follow-up: drop the allowance when NgRx 22 ships |
| Pre-existing `libs/ui` (Helix hx- components) retained although the dashboard won't use it (tension with spec-002 "no unused placeholder files") | User instructed to keep the existing scaffolding; it belongs to spec-001 scope (widget app / future reconciliation) | Deleting rejected by user decision. The rule is applied to all **new** code in `apps/dashboard`; a follow-up spec must reconcile Helix vs Taiga for the widget |
| No end-to-end test category in this feature (Principle VII lists E2E as a required coverage category) | This foundation ships no business flows to exercise end-to-end; the repo has no E2E tooling yet, and spec-002 explicitly defers E2E until tooling exists | Adding Playwright now rejected — it would bootstrap E2E infrastructure solely to click placeholder pages, contradicting spec-002's "no over-engineering" scope. E2E arrives with spec-001's planned Playwright suites once real user flows exist; unit/integration/component categories are fully covered here |
