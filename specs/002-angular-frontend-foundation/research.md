# Research: Angular Frontend Foundation

**Date**: 2026-07-04 | **Feature**: [spec.md](./spec.md)

All versions below were verified against the live npm registry on 2026-07-04. The user-provided reference app `~/test-ng` (fresh `ng new` output) was inspected for current CLI conventions.

## R1. Angular version & workspace baseline

- **Decision**: Stay on the existing `frontend/` pnpm workspace, already at Angular **22.0.5** (`latest` on npm as of today). Update all `@angular/*` packages to `^22.0.5` uniformly. TypeScript `~6.0.3` (already present; matches Angular 22's supported range).
- **Rationale**: User decision: keep the existing scaffolding and update to latest Angular 22. The workspace already resolves 22.0.5, so "update" means aligning tooling/conventions rather than bumping the framework.
- **Alternatives considered**: Fresh single-app `ng new` project (rejected by user); staying on the old `@angular-devkit/build-angular` toolchain (rejected — see R2).

## R2. Build & test toolchain modernization (from `~/test-ng` reference)

- **Decision**: Migrate the `dashboard` project in `angular.json` to the modern builders and copy missing configs from `~/test-ng`:
  - `@angular/build:application` (replaces `@angular-devkit/build-angular:application`)
  - `@angular/build:dev-server`
  - `@angular/build:unit-test` with **Vitest 4 + jsdom** (replaces Karma/Jasmine)
  - Remove devDependencies: `@angular-devkit/build-angular`, `karma*`, `jasmine*`, `@types/jasmine`; add `@angular/build`, `vitest`, `jsdom`, `prettier`
  - Copy `.prettierrc` (printWidth 100, singleQuote, angular HTML parser) and `.editorconfig` from `~/test-ng`
  - Add production/development build configurations with budgets, `fileReplacements` for environments, `defaultConfiguration: production` (the existing angular.json lacks configurations entirely)
- **Rationale**: `~/test-ng` (user-supplied canonical reference) shows current `ng new` output: `@angular/build` + Vitest is the default unit-test story; Karma is legacy. Spec requires Prettier, which the workspace lacks.
- **Alternatives considered**: Keep Karma/Jasmine (rejected — legacy, and the workspace's Karma setup is incomplete anyway: no karma.conf, missing `main` test entry); Jest (rejected — not the Angular-CLI-blessed path in v22).

## R3. Zoneless change detection

- **Decision**: Bootstrap the dashboard app zoneless (`provideZonelessChangeDetection()`), remove `zone.js` from the dashboard's build/test polyfills (keep the dependency only if the untouched `widget` app still references it — its angular.json test config lists zone.js polyfills; do not break it).
- **Rationale**: `~/test-ng` ships without zone.js — zoneless is the Angular 22 default for new apps. Spec mandates signals + OnPush everywhere, which is exactly the zoneless-compatible pattern. NgRx (signals-based selectors via `store.selectSignal`) and Taiga UI 5 support zoneless.
- **Alternatives considered**: Keep zone.js (rejected — legacy pattern; spec says avoid legacy unless a dependency requires it; none does).

## R4. NgRx on Angular 22 (compatibility deviation)

- **Decision**: Install NgRx **21.1.1** (`@ngrx/store`, `@ngrx/effects`, `@ngrx/signals`, `@ngrx/store-devtools`) with a pnpm peer-dependency allowance:
  ```jsonc
  // frontend/package.json
  "pnpm": { "peerDependencyRules": { "allowedVersions": { "@ngrx/*>@angular/core": "22" } } }
  ```
  Use functional/standalone APIs exclusively: `provideStore`, `provideEffects`, `provideStoreDevtools` (dev only), `createFeature`, `createActionGroup`, `createReducer`, `signalStore`.
- **Rationale**: Verified on npm: NgRx `latest` = 21.1.1 with peer `@angular/core ^21.0.0`; no 22-targeting release or prerelease exists (`next` = 21.0.0-rc.0). NgRx's peer ranges follow a lockstep-with-Angular policy; the library uses only stable public Angular APIs, so running 21 on Angular 22 is technically sound. Recorded in plan Complexity Tracking; remove the allowance when NgRx 22 ships.
- **Alternatives considered**: Wait for NgRx 22 (rejected — blocks the whole foundation); Angular 21 downgrade (rejected — spec mandates Angular 22); state without NgRx (rejected — spec mandates NgRx).

## R5. Taiga UI installation & theming

- **Decision**: Install Taiga UI **5.13.0** using the official schematic — `ng add taiga-ui` (the "recommended Taiga UI installation command" per spec) run for the `dashboard` project; verify/adjust what it wires: Taiga styles in the dashboard's `angular.json` styles array, event-plugin providers in `app.config.ts`, `<tui-root>` wrapping the app template. Peer check passed: `@taiga-ui/core@5.13.0` requires `@angular/core >= 19` — Angular 22 is officially supported (no override needed).
  - Dark theme: set `[attr.tuiTheme]` on `<tui-root>` (per Taiga docs) driven by the resolved theme, and mirror `data-theme` on `<html>` for our own `--app-*` theme tokens. "System" mode resolves via `prefers-color-scheme` media query with a live listener.
  - Icons/components used in the foundation only where needed (`TuiButton`, `TuiIcon`, `TuiLoader` for the shared loading component) — no global icon-set import.
- **Rationale**: Spec requires the official setup flow and Taiga as primary UI library; Taiga's `tuiTheme` attribute is its documented dark-mode mechanism (verified via current Taiga docs), and pairing it with our token-level `data-theme` keeps app tokens and Taiga tokens switching together.
- **Alternatives considered**: Manual package-by-package install (rejected — spec says use the recommended installation command); Angular Material (rejected — user decision).

## R6. Linting

- **Decision**: Upgrade the flat ESLint config to **angular-eslint 22.0.0** (verified: supports `@angular/cli >= 22 < 23`, ESLint 10, typescript-eslint 8) with recommended TS + template rulesets, keeping the existing `@typescript-eslint/no-explicit-any: error`. Wire `pnpm lint` to cover dashboard sources; add a `format`/`format:check` script for Prettier.
- **Rationale**: Spec requires ESLint + Prettier + strict-template discipline; angular-eslint adds Angular-specific rules (template accessibility checks support the a11y requirements) that raw typescript-eslint lacks.
- **Alternatives considered**: Keep the current minimal typescript-eslint-only config (rejected — no Angular/template rules, no a11y linting).

## R7. State architecture for `appUi`

- **Decision**: One global feature slice `appUi` via `createFeature`:
  - State: `{ themeMode: 'light'|'dark'|'system', sidebarCollapsed: boolean }`; defaults `system` / `false` (per clarifications).
  - Actions via `createActionGroup` (`themeModeChanged`, `sidebarToggled`, `sidebarCollapsedSet`).
  - Theme persistence (theme only, per clarification): a small `AppUiEffects` class listens to `themeModeChanged` and writes localStorage; the initial state is hydrated by a factory that reads/validates localStorage (invalid/missing → `system`). Sidebar state intentionally not persisted.
  - Layout reads state with `store.selectSignal(...)` (zoneless-friendly), dispatches on toggle — no duplicated local state.
  - NgRx SignalStore: used for genuinely feature-local state only. The one real usage in this foundation: a small `LayoutStore` (signalStore) scoped to the shell for viewport/breakpoint tracking that drives the "narrow viewport ⇒ default collapsed" rule (feature-local, interacts with but does not duplicate the global `sidebarCollapsed` — it dispatches the global action when the breakpoint is crossed).
  - Devtools: `provideStoreDevtools` guarded by `environment.enableNgRxDevtools` (true only in development).
- **Rationale**: Satisfies FR-007..FR-011 and the Store-vs-SignalStore decision rule with a real (not decorative) SignalStore consumer; hydration-by-initial-state avoids a flash of wrong theme.
- **Alternatives considered**: Meta-reducer for persistence (rejected — effects are the documented side-effect home and the spec asks for an effects placeholder anyway); persisting via SignalStore (rejected — theme is global cross-feature state).

## R8. HTTP/API models aligned with backend contract

- **Decision**: Model shapes mirror `specs/001-ai-customer-service-platform/contracts/rest-api.md`:
  - `ApiError`: `{ code: string; message: string; details?: ApiErrorDetail[]; requestId?: string; status: number }` mapped from the backend error envelope `{ error: { code, message, details, request_id } }` and transport-level failures (status 0 network, malformed body → safe fallback).
  - `PaginatedResponse<T>`: `{ items: T[]; nextCursor: string | null; hasMore: boolean }` (cursor pagination per contract).
  - `ApiListQuery`: `{ limit?: number; cursor?: string; sort?: string; order?: 'asc'|'desc'; q?: string }`.
  - `ApiResponse<T>`: typed success wrapper carrying `data: T` plus optional `requestId` (captured from `X-Request-Id` when present).
  - Functional interceptors: `apiErrorInterceptor` (normalizes `HttpErrorResponse` → `ApiError`), `authTokenInterceptor` (registered no-op pass-through with a clearly named extension point; zero fake token logic).
  - `provideHttpClient(withInterceptors([...]), withFetch())`; base URL from environment via an `APP_CONFIG` injection token.
- **Rationale**: Constitution Principle V requires contract consistency; aligning now prevents model rework when real integration lands.
- **Alternatives considered**: Offset pagination models (rejected — backend contract is cursor-based); class-based interceptors (rejected — spec requires functional).

## R9. Routing & shell

- **Decision**: Root `app.routes.ts`: `''` → redirect `tenant/overview-placeholder`; `auth` / `platform` / `tenant` each `loadChildren` a per-area routes file; `**` → lazy not-found page. Platform + tenant route groups render inside the `AppShellComponent` (layout with sidebar/topbar/main); auth area renders a minimal centered layout without the dashboard chrome. One pass-through functional guard (`canMatchArea`-style named stub with tests) demonstrates the guard seam without faking auth. Typed route-path constants in `core/router` keep paths refactor-safe.
- **Rationale**: FR-002..FR-006 + clarified not-found behavior; separating shells now prepares platform/tenant contexts without business logic.
- **Alternatives considered**: Single flat routes file (rejected — spec requires per-area route files); component-input binding of route data (not needed yet).

## R10. Design tokens & themes

- **Decision**: `--app-*` CSS custom properties in `design-system/tokens/tokens.css` (spacing scale, radius, shadows, z-index scale, breakpoints as documented values, layout sizes incl. `--app-sidebar-width: 280px`, `--app-sidebar-collapsed-width: 72px`, `--app-topbar-height: 64px`, `--app-page-max-width: 1440px`, typography scale) and theme-dependent color tokens in `design-system/theme/themes.css` switching on `:root[data-theme='dark']` / `[data-theme='light']`, with system mode resolved in TS (media-query listener) rather than duplicating every token under `@media`. Color values take visual cues from the existing Helix palette (`libs/ui/src/styles/tokens.css`, extracted from `Helix Admin.html`) but are defined fresh under the `--app-*` namespace and kept compatible with Taiga's dark/light switching.
- **Rationale**: Constitution IX (tokens before components), spec token groups, and visual continuity with the approved Helix prototype without coupling the dashboard to the legacy `libs/ui` stylesheet.
- **Alternatives considered**: Importing `libs/ui` tokens directly (rejected — different naming scheme (`--bg-app`, `--panel*`), belongs to the retained-but-untouched Helix lib); pure `prefers-color-scheme` CSS switching (rejected — cannot express an explicit user override to light/dark).

## R11. Error handling & logging

- **Decision**: `ErrorHandler` implementation (`GlobalErrorHandler`) provided in `app.config.ts`, delegating to a `LoggerService` (leveled, structured console output in dev; single seam for future remote reporting). `mapHttpError(err) → ApiError` pure function (unit-tested against server-error, network-failure, malformed-body, unknown inputs) plus `userMessageFor(apiError)` returning safe human copy per error code/status — raw backend messages never surface directly.
- **Rationale**: FR-018/FR-019 + SC-008; pure-function mapper keeps the critical logic trivially testable.
- **Alternatives considered**: Toast-on-every-error via Taiga alerts (rejected — presentation policy belongs to future feature specs; foundation only maps and logs).

## R12. Loading pattern

- **Decision**: A shared `LoadingIndicatorComponent` wrapping Taiga's loader + a documented local-signal convention (`loading = signal(false)` per operation) in the frontend docs. No global loading slice.
- **Rationale**: Spec explicitly warns against loading over-engineering; the global-store route is documented as the escalation path only if a cross-feature indicator is ever required.
- **Alternatives considered**: Global `loading` NgRx slice keyed by action (rejected — over-engineering for a foundation with no real requests yet).

## Resolved clarifications carried from spec

| Topic | Resolution |
|---|---|
| Unknown routes | Minimal not-found page with link back to default route |
| Default theme | `system` |
| Persistence | Theme only, localStorage; sidebar resets |
| Narrow viewport | Sidebar defaults to collapsed below breakpoint, still toggleable |

No NEEDS CLARIFICATION items remain.
