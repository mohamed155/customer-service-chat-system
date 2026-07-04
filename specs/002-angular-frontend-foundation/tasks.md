# Tasks: Angular Frontend Foundation

**Input**: Design documents from `/specs/002-angular-frontend-foundation/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/frontend-foundation.md, quickstart.md

**Tests**: REQUIRED by the spec (FR-024, Testing Requirements). Test tasks are included per story; no snapshot-only tests.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story. All paths are relative to the repo root. All `pnpm`/`ng` commands run from `frontend/`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US4)

## Path Conventions

Existing pnpm workspace `frontend/` (kept per plan). This feature works only in `frontend/` root configs and `frontend/apps/dashboard/`. `frontend/apps/widget/` and `frontend/libs/*` are NOT modified except the mechanical builder rename in T003. Dashboard app code root: `frontend/apps/dashboard/src/`.

---

## Phase 1: Setup (Workspace Modernization)

**Purpose**: Bring the existing workspace to current Angular 22 conventions (per research.md R1–R6) and install the required libraries.

- [X] T001 Update `frontend/package.json`: bump all `@angular/*` to `^22.0.5`; remove `@angular-devkit/build-angular`, `karma`, `karma-chrome-launcher`, `karma-coverage`, `karma-jasmine`, `karma-jasmine-html-reporter`, `jasmine-core`, `@types/jasmine`; add devDeps `@angular/build@^22.0.5`, `vitest@^4`, `jsdom`, `prettier@^3`; add `pnpm.peerDependencyRules.allowedVersions` entry allowing `@ngrx/*` peers on `@angular/core` 22; add scripts `start` (`ng serve dashboard`), `test` (`ng test dashboard`), `format` (`prettier --write .`), `format:check` (`prettier --check .`); run `pnpm install`
- [X] T002 [P] Add `frontend/.prettierrc` (printWidth 100, singleQuote, angular parser for HTML — copied from `~/test-ng/.prettierrc`) and `frontend/.editorconfig` (copied from `~/test-ng/.editorconfig`); add a `.prettierignore` covering `dist/`, `.angular/`, `pnpm-lock.yaml`
- [X] T003 Migrate `frontend/angular.json` to modern builders: both projects `@angular-devkit/build-angular:*` → `@angular/build:*` (application/dev-server); **both** `test` targets → `@angular/build:unit-test` (vitest runner, jsdom) — dashboard without zone polyfills, widget keeping its `zone.js`/`zone.js/testing` polyfills (its Jasmine-style `describe/it/expect` spec runs unchanged under Vitest globals; widget must keep building AND testing after the Karma removal in T001); add dashboard `production`/`development` build configurations with budgets, `outputHashing`, `fileReplacements` (`environment.ts` ← `environment.development.ts` in development), `defaultConfiguration`; dashboard `styles` → `apps/dashboard/src/styles.css`. Create minimal stub files so the checkpoint build passes: empty `apps/dashboard/src/styles.css` and placeholder `apps/dashboard/src/environments/environment.ts` + `environment.development.ts` (filled properly in T007/T009); widget config otherwise unchanged
- [X] T004 Replace `frontend/eslint.config.mjs` with angular-eslint 22 flat config: install `angular-eslint@^22`, `typescript-eslint@^8`, `eslint@^10` (align existing); enable recommended TS + template rulesets incl. template accessibility rules; keep `@typescript-eslint/no-explicit-any: error`; scope to `apps/**` and `libs/**`; add layer-boundary enforcement for the dashboard app via `no-restricted-imports` patterns (files in `core/**` and `shared/**` may not import from `features/**` or `layout/**`; `shared/**` may not import `core/**` state) so FR-022's dependency direction is machine-enforced, not review-only
- [X] T005 [P] Install NgRx 21.1.1: `pnpm add @ngrx/store@21.1.1 @ngrx/effects@21.1.1 @ngrx/signals@21.1.1` and `pnpm add -D @ngrx/store-devtools@21.1.1`; verify install succeeds with the peer allowance from T001
- [X] T006 Install Taiga UI 5.13 via the official schematic: `pnpm ng add taiga-ui --project dashboard` (accept its changes: Taiga styles in `frontend/angular.json` dashboard styles, event-plugin providers, `<tui-root>` in the app template); verify `pnpm ng build dashboard` succeeds

**Checkpoint**: `pnpm ng build dashboard`, `pnpm ng build widget`, and `pnpm ng test widget` all succeed on the new toolchain; `pnpm lint` and `pnpm format:check` run (fix any fallout — including any Jasmine-type references in widget's `tsconfig.spec.json` now that `@types/jasmine` is removed).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Structure, configuration, tokens, and bootstrap that every user story depends on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T007 [P] Complete environment config (stubs created in T003): `frontend/apps/dashboard/src/environments/environment.ts` (production values) and `environment.development.ts` (dev values) per contracts §4; create `frontend/apps/dashboard/src/app/core/config/app-config.ts` with `AppConfig` interface + `APP_CONFIG` injection token provided from the environment file
- [X] T008 [P] Create design tokens in `frontend/apps/dashboard/src/app/design-system/tokens/tokens.css`: `--app-*` custom properties for layout sizes (sidebar 280px/72px, topbar 64px, page max 1440px), spacing scale, radius (6/10/16px), shadows, z-index scale, breakpoints (`--app-bp-lg: 1024px` is the designated sidebar-collapse breakpoint), typography — per contracts §5
- [X] T009 [P] Create theme stylesheet in `frontend/apps/dashboard/src/app/design-system/theme/themes.css`: theme color tokens (`--app-color-*`) with values under `:root[data-theme='light']` and `:root[data-theme='dark']` (color values informed by the Helix palette in `frontend/libs/ui/src/styles/tokens.css`, defined fresh under `--app-*`); fill `frontend/apps/dashboard/src/styles.css` (stubbed in T003) importing tokens.css + themes.css and setting base body styles from tokens
- [X] T010 [P] Create `frontend/apps/dashboard/src/app/core/logging/logger.service.ts`: injectable singleton `LoggerService` with leveled (`debug/info/warn/error`) structured console output including timestamp and context tag
- [X] T011 [P] Create `frontend/apps/dashboard/src/app/core/router/app-paths.ts`: exported `APP_PATHS` const (auth/platform/tenant bases + placeholder child paths) per data-model.md
- [X] T012 Rebuild dashboard bootstrap for the layered structure: `main.ts` (zoneless standalone bootstrap via `provideZonelessChangeDetection`, no zone.js), `app/app.config.ts` (`provideRouter` with the routes file, `provideBrowserGlobalErrorListeners`, `provideHttpClient(withFetch())`, Taiga providers from T006), `app/app.component.ts` (OnPush, `<tui-root>` wrapping `<router-outlet/>`), `app/app.routes.ts` (temporary empty routes array — real routes in US1), `index.html` (lang, title, viewport); create the layer folders `core/ layout/ shared/ features/ design-system/` as they gain their first real files (no empty placeholder dirs)

**Checkpoint**: App builds and serves a blank `<tui-root>` shell with tokens/themes loaded — foundation ready; user stories can begin.

---

## Phase 3: User Story 1 — Working Dashboard Application Shell (Priority: P1) 🎯 MVP

**Goal**: Runnable dashboard with sidebar/topbar/content shell and lazy auth/platform/tenant areas; `/` redirects to tenant overview; unknown routes hit a not-found page.

**Independent Test**: `pnpm ng serve dashboard`; visit `/`, `/auth/login-placeholder`, `/platform/overview-placeholder`, `/tenant/overview-placeholder`, `/nope` — each renders correctly per quickstart.md; route/boot/shell tests pass.

### Implementation for User Story 1

- [X] T013 [P] [US1] Create auth area: `frontend/apps/dashboard/src/app/features/auth/auth.routes.ts` (empty-path redirect → `login-placeholder`) and `features/auth/login-placeholder/login-placeholder.component.ts` (standalone, OnPush, minimal centered layout without dashboard chrome, semantic `<main>`)
- [X] T014 [US1] (after T018) Create platform area: `frontend/apps/dashboard/src/app/features/platform/platform.routes.ts` (empty-path redirect → `overview-placeholder`) and `features/platform/overview-placeholder/platform-overview-placeholder.component.ts` (standalone, OnPush, uses PageHeader from T018)
- [X] T015 [US1] (after T018) Create tenant area: `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` (empty-path redirect → `overview-placeholder`) and `features/tenant/overview-placeholder/tenant-overview-placeholder.component.ts` (standalone, OnPush, uses PageHeader from T018)
- [X] T016 [P] [US1] Create not-found page: `frontend/apps/dashboard/src/app/features/not-found/not-found.component.ts` (standalone, OnPush, message + `routerLink` back to `/tenant/overview-placeholder` using `APP_PATHS`)
- [X] T017 [P] [US1] Create pass-through guard with test: `frontend/apps/dashboard/src/app/core/router/area-access.guard.ts` (functional `CanMatchFn`, returns `true`, clearly named seam for the future auth spec — no fake auth) and `area-access.guard.spec.ts` asserting it passes
- [X] T018 [P] [US1] Create page header component: `frontend/apps/dashboard/src/app/layout/page-header/page-header.component.ts` (standalone, OnPush, `<header>` with title input + content projection slot for future actions, styled with `--app-*` tokens)
- [X] T019 [P] [US1] Create sidebar component: `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` (standalone, OnPush, semantic `<nav aria-label>`, placeholder nav links to platform/tenant overview via `APP_PATHS` + `routerLinkActive`, Taiga icons/buttons, width from `--app-sidebar-width`; accepts `collapsed` input rendering the 72px icon-only variant)
- [X] T020 [P] [US1] Create topbar component: `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts` (standalone, OnPush, semantic `<header>`, app name from `APP_CONFIG`, placeholder region for future user menu/notifications; real `<button>` elements only — wired to state in US2)
- [X] T021 [US1] Create app shell: `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.ts` (standalone, OnPush) composing sidebar + topbar + `<main>` with `<router-outlet/>`; responsive CSS grid using layout tokens; visible focus states; keyboard-reachable landmarks (nav/header/main)
- [X] T022 [US1] Wire `frontend/apps/dashboard/src/app/app.routes.ts`: `''` redirect → `tenant/overview-placeholder`; `auth` → `loadChildren` auth.routes (no shell); `platform` and `tenant` → child routes under `AppShellComponent` with `canMatch: [areaAccessGuard]`, each `loadChildren` their routes file; `**` → `loadComponent` not-found; all paths from `APP_PATHS`
- [X] T023 [P] [US1] Boot + route tests in `frontend/apps/dashboard/src/app/app.spec.ts` and `app.routes.spec.ts`: app bootstraps (TestBed renders AppComponent); RouterTestingHarness resolves `/` → tenant overview redirect, each placeholder route renders its component, unknown route renders not-found (covers spec SC-002)
- [X] T024 [P] [US1] Shell component test in `frontend/apps/dashboard/src/app/layout/app-shell/app-shell.component.spec.ts`: renders nav/header/main landmarks and the routed content outlet

**Checkpoint**: US1 fully functional — quickstart "Run the app" section passes end-to-end. MVP deliverable.

---

## Phase 4: User Story 2 — Theme and Sidebar Preferences (Priority: P2)

**Goal**: Global `appUi` NgRx slice drives theme (light/dark/system, persisted) and sidebar collapse (not persisted); layout reacts; narrow viewports default to collapsed via a real SignalStore.

**Independent Test**: Toggle sidebar and switch theme in the topbar — layout and colors respond, DevTools shows actions (dev only), dark mode survives reload, sidebar resets; reducer/selector/integration tests pass (quickstart "Validate state & theming").

### Implementation for User Story 2

- [X] T025 [P] [US2] Create `appUi` slice in `frontend/apps/dashboard/src/app/core/state/app-ui.feature.ts`: `ThemeMode` type, `AppUiState`, `appUiActions` via `createActionGroup` (`themeModeChanged`, `sidebarToggled`, `sidebarCollapsedSet`), `createFeature` reducer with initial-state factory hydrating `themeMode` from localStorage key `app.themeMode` (invalid/missing → `'system'`, `sidebarCollapsed: false`), selectors `selectThemeMode`/`selectSidebarCollapsed` per data-model.md
- [X] T026 [P] [US2] Create system-theme resolution in `frontend/apps/dashboard/src/app/core/state/system-theme.ts`: signal wrapping `matchMedia('(prefers-color-scheme: dark)')` with change listener, and `selectResolvedTheme` composition (`'system'` → OS preference) — SSR-safe guards
- [X] T027 [US2] Create `frontend/apps/dashboard/src/app/core/state/app-ui.effects.ts`: non-dispatching effect persisting `themeMode` to localStorage on `themeModeChanged` (theme only — sidebar never persisted)
- [X] T028 [US2] Register the store in `frontend/apps/dashboard/src/app/app.config.ts`: `provideStore({ [appUiFeature.name]: appUiFeature.reducer })`, `provideEffects(AppUiEffects)`, `provideStoreDevtools(...)` included only when `APP_CONFIG.enableNgRxDevtools` (development)
- [X] T029 [US2] Apply resolved theme in `frontend/apps/dashboard/src/app/app.component.ts`: effect syncing resolved theme → `data-theme` attribute on `<html>` and `[attr.tuiTheme]` on `<tui-root>` so `--app-*` and Taiga tokens flip together (live-updates when OS preference changes in `system` mode)
- [X] T030 [US2] Connect layout to the store: `app-shell.component.ts` reads `selectSidebarCollapsed` via `store.selectSignal` and passes to sidebar (no duplicated local state); `topbar.component.ts` gets working controls — sidebar toggle `<button>` dispatching `sidebarToggled` (aria-label, aria-expanded) and a Taiga-based theme-mode switcher dispatching `themeModeChanged`
- [X] T031 [US2] Create the real SignalStore: `frontend/apps/dashboard/src/app/layout/app-shell/layout.store.ts` — `signalStore` (provided in AppShell) tracking `viewportWidth` from a resize listener with computed `isNarrow` (uses exported `LAYOUT_COLLAPSE_BREAKPOINT = 1024` const mirroring `--app-bp-lg`); on crossing into narrow, dispatches `sidebarCollapsedSet({ collapsed: true })`; never copies global state (contracts §2 rule)
- [X] T032 [P] [US2] State unit tests in `frontend/apps/dashboard/src/app/core/state/app-ui.feature.spec.ts` and `app-ui.effects.spec.ts`: theme action updates state; sidebar toggle flips; `sidebarCollapsedSet` sets; hydration falls back to `'system'` on invalid stored value; selectors return expected slices; persistence effect writes localStorage (covers spec SC-003 state behavior)
- [X] T033 [US2] Integration tests: extend `app-shell.component.spec.ts` (toggle click dispatches `sidebarToggled`; collapsed state renders narrow sidebar) and `app.spec.ts` or new `theme.spec.ts` (theme change sets `data-theme`/`tuiTheme`; simulated `matchMedia` change event updates `data-theme` while `themeMode === 'system'` — live OS-preference edge case); LayoutStore test for narrow-viewport auto-collapse in `layout.store.spec.ts`

**Checkpoint**: US1 + US2 work independently — quickstart "Validate state & theming" passes.

---

## Phase 5: User Story 3 — API Communication Foundation (Priority: P3)

**Goal**: Typed API models aligned with the backend contract, functional interceptors, tested error mapping with user-safe messages, global error handler, and a local loading pattern.

**Independent Test**: `pnpm ng test dashboard` — mapper suite covers server error/network failure/malformed body/unknown (SC-008); interceptor normalizes failures to `ApiError`; `ApiService` uses `APP_CONFIG.apiBaseUrl` (quickstart "Validate error foundation").

### Implementation for User Story 3

- [X] T034 [P] [US3] Create API models in `frontend/apps/dashboard/src/app/core/api/api.models.ts`: `ApiResponse<T>`, `ApiError`, `ApiErrorDetail`, `PaginatedResponse<T>` (`items`/`nextCursor`/`hasMore`), `ApiListQuery` — exactly per data-model.md (mirrors `specs/001-ai-customer-service-platform/contracts/rest-api.md`)
- [X] T035 [P] [US3] Create error mapping in `frontend/apps/dashboard/src/app/core/errors/http-error.mapper.ts`: pure `mapHttpError(err: unknown): ApiError` (backend envelope snake_case → camelCase incl. `request_id` → `requestId`; status 0/ProgressEvent → `network_error`; unparsable → `unknown_error`) and `userMessageFor(error: ApiError): string` mapping codes/status ranges to safe user copy with generic fallback — never returns raw backend messages
- [X] T036 [US3] Create functional interceptors in `frontend/apps/dashboard/src/app/core/http/auth-token.interceptor.ts` (registered no-op pass-through with named extension point, zero fake token logic) and `core/http/api-error.interceptor.ts` (catches errors, rethrows normalized `ApiError` via `mapHttpError`); register both in `app.config.ts` `withInterceptors([authTokenInterceptor, apiErrorInterceptor])`
- [X] T037 [US3] Create base API service in `frontend/apps/dashboard/src/app/core/api/api.service.ts`: injectable singleton wrapping `HttpClient` with `get/post/patch/delete` prefixing `APP_CONFIG.apiBaseUrl`, list helper serializing `ApiListQuery` params, capturing `X-Request-Id` into `ApiResponse.requestId` when present
- [X] T038 [US3] Create global error handler in `frontend/apps/dashboard/src/app/core/errors/global-error-handler.ts`: `ErrorHandler` implementation delegating to `LoggerService` (readable structured output, `ApiError`-aware); provide via `{ provide: ErrorHandler, useClass: GlobalErrorHandler }` in `app.config.ts`
- [X] T039 [US3] Create loading pattern: `frontend/apps/dashboard/src/app/shared/components/loading-indicator/loading-indicator.component.ts` (standalone, OnPush, wraps Taiga loader, size input, accessible busy semantics) — documents the local `signal(false)` convention in its docblock; no global loading slice
- [X] T040 [P] [US3] API foundation tests: `core/errors/http-error.mapper.spec.ts` (all four failure shapes → typed `ApiError`; `userMessageFor` safe copy + fallback — SC-008), `core/http/api-error.interceptor.spec.ts` (HttpTestingController: failed request surfaces normalized `ApiError`; auth placeholder passes through untouched), `core/api/api.service.spec.ts` (base URL from `APP_CONFIG`, list-query serialization)

**Checkpoint**: All three foundations (shell, state, HTTP) complete and independently tested.

---

## Phase 6: User Story 4 — Consistent Foundations for New Contributors (Priority: P4)

**Goal**: Architecture documentation matching the implementation; all quality gates green.

**Independent Test**: `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` all exit 0; a new developer can place code correctly using only the docs (SC-006).

### Implementation for User Story 4

- [X] T041 [P] [US4] Write `frontend/README.md` (frontend architecture documentation): workspace layout + dashboard layer boundaries with dependency rules (core/shared never depend on features), state placement rules incl. Store vs SignalStore vs signal decision table (contracts §2), Taiga UI usage rules (primary library, no custom component library, wrap/compose later), theme/token rules (`--app-*`, `data-theme` + `tuiTheme`, no hardcoded values), HTTP/error conventions (typed `ApiError`, `userMessageFor`), loading pattern, testing conventions (Vitest, what gets unit vs component tests), quality-gate commands
- [X] T042 [US4] Run and fix all quality gates from `frontend/`: `pnpm format` then `pnpm format:check`, `pnpm lint`, `pnpm ng test dashboard`, `pnpm ng build dashboard` — zero errors/failures; remove any dead placeholder files introduced during implementation; verify no TODO comments without a linked spec/task (FR-023)

**Checkpoint**: All user stories complete; codebase ready for the next frontend spec.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final verification against success criteria and spec compliance.

- [X] T043 [P] Verify lazy loading (SC-005): `pnpm ng build dashboard` and confirm separate chunks for auth/platform/tenant/not-found in `frontend/dist/dashboard/browser`; confirm initial bundle excludes other areas (Network tab per quickstart)
- [X] T044 [P] Accessibility pass (FR-027): keyboard-walk the shell (tab order, visible focus, Enter/Space on toggles), confirm landmarks (nav/header/main), no clickable divs, contrast via theme tokens in both themes
- [X] T045 Execute `specs/002-angular-frontend-foundation/quickstart.md` top to bottom (serve, routes, state/theming incl. reload persistence + narrow viewport, devtools off in production build, gates) and fix any gaps
- [X] T046 Sync docs with reality: update `frontend/README.md` and `CLAUDE.md` if implementation diverged during T042–T045 (FR-026: documentation must match actual implementation); confirm no business features slipped in (FR-025 / SC-007 exclusion list)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies. T001 → T003/T004/T005 (deps must exist before config referencing them); T002 anytime; T006 after T003 (needs working build)
- **Foundational (Phase 2)**: After Phase 1. T007–T011 all [P]; T012 after T006 + T007 (Taiga providers, config token)
- **User Stories (Phase 3+)**: All require Phase 2 complete
- **US1 (Phase 3)**: T013–T020 mostly [P] (T014/T015 need T018 for PageHeader); T021 after T019/T020; T022 after T013–T017 + T021; T023/T024 after T022
- **US2 (Phase 4)**: Independent of US3. T025/T026 [P] → T027 → T028; T029 after T026/T028; T030 after T028 (touches US1's shell/topbar files — sequence after US1); T031 after T030; T032 after T025–T027; T033 after T030/T031
- **US3 (Phase 5)**: Independent of US2 (can run in parallel with Phase 4 — different files except `app.config.ts` registration in T028/T036/T038, coordinate those edits). T034/T035 [P] → T036 → T037/T038; T039 anytime after Phase 2; T040 after T036/T037
- **US4 (Phase 6)**: After US1–US3
- **Polish (Phase 7)**: After Phase 6

### User Story Dependencies

- **US1 (P1)**: Only Foundational — standalone MVP
- **US2 (P2)**: Builds on US1's layout components (wires them to state) — starts after US1
- **US3 (P3)**: Independent of US1/US2 except shared `app.config.ts` lines — parallelizable with US2
- **US4 (P4)**: Documents everything — last

### Parallel Opportunities

- Phase 2: T007, T008, T009, T010, T011 simultaneously (5 different files)
- US1: T013–T020 largely simultaneously (8 independent component/route files)
- US2 + US3 can proceed in parallel by two developers (coordinate `app.config.ts`)
- Test tasks marked [P] within each story run alongside each other

## Parallel Example: User Story 1

```bash
# After Phase 2, launch the independent area/component tasks together:
Task: T013 auth routes + login placeholder
Task: T016 not-found page
Task: T017 pass-through guard + spec
Task: T018 page-header component
Task: T019 sidebar component
Task: T020 topbar component
# Then (need T018's PageHeader): T014 platform area + T015 tenant area in parallel
# Then sequentially: T021 shell → T022 routes wiring → T023/T024 tests
```

## Implementation Strategy

### MVP First (US1 only)

1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (US1)
2. **STOP and VALIDATE**: quickstart "Run the app" — navigable dashboard shell is a demonstrable MVP
3. Then US2 (state/theming), US3 (HTTP) — optionally in parallel — then US4 (docs/gates), Polish

### Incremental Delivery

Each checkpoint is a working, testable increment: toolchain green → blank themed shell → navigable dashboard (MVP) → stateful preferences → API-ready foundation → documented + gated.

## Notes

- The spec's suggested FE-001…FE-016 task list maps onto these phases (FE-001/002→Phase 1, FE-003→Phase 2, FE-004/005→US1, FE-006→T008/T009, FE-007/008/009/010→US2, FE-011/012/013/014→US3, FE-015 distributed per story, FE-016→US4)
- Never modify `frontend/apps/widget/` or `frontend/libs/*` beyond T003's mechanical builder rename
- Commit after each task or logical group; keep `pnpm lint`/`format:check` green as you go (FR-023)
