# AI Customer Service Platform — Agent Context

## Active Technologies

- **Backend** (`backend/`): Rust (edition 2024), Axum, Tokio, SQLx, PostgreSQL, Redis, pgvector. Cargo workspace under `backend/crates/`.
- **Frontend** (`frontend/`): Angular 22 (standalone, signals, zoneless, OnPush), TypeScript ~6.0, pnpm workspace (`apps/dashboard`, `apps/widget`, `libs/*`). NgRx 21 (Store/Effects/Signals — peer-dep allowance on Angular 22), Taiga UI 5 as primary UI library, `@fontsource/geist-sans` + `@fontsource/geist-mono` (self-hosted fonts), Vitest via `@angular/build:unit-test`, angular-eslint + Prettier.
- **Specs**: Spec-Kit workflow in `specs/`; constitution in `.specify/memory/constitution.md`.

## Frontend rules (from spec 002)

- Layers inside `apps/dashboard/src/app/`: `core/` (singletons, no feature deps), `shared/` (reusable, no business logic), `layout/`, `features/{auth,platform,tenant}` (lazy route areas), `design-system/` (`--app-*` tokens, light/dark via `data-theme` + Taiga `tuiTheme`).
- State: global cross-feature → NgRx Store (`appUi` slice: themeMode/sidebarCollapsed); feature-local → NgRx SignalStore; component-temporary → `signal()`. Never duplicate state across mechanisms. Theme persists to localStorage; sidebar does not.
- HTTP: typed `ApiResponse<T>`/`ApiError`/`PaginatedResponse<T>`/`ApiListQuery` aligned with `specs/001-ai-customer-service-platform/contracts/rest-api.md` (cursor pagination, error envelope, `X-Request-Id`). Functional interceptors only. No fake auth logic.
- Route paths come from `APP_PATHS` constants in `core/router` — no string literals in features.
- `apps/widget` and `libs/*` (Helix hx- components) are prior scaffolding — do not modify or use for the dashboard; Taiga UI only.
- Quality gates (run in `frontend/`): `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` — all must pass.

## Frontend rules (from spec 003)

- Helix visual system: `--app-*` tokens rewritten to the Helix palette/layout (sidebar 248/68px, topbar 60px, content max 1320px); light/dark values live in `design-system/theme/themes.css`, theme-independent values in `design-system/tokens/tokens.css`. Old `--app-color-*` names are gone. Reference design: `Helix Admin.html` at repo root (compare only — never copy markup/assets/fonts from it).
- Theme toggle cycles light → dark → system. Topbar search / notifications / "New" are purely visual (no handlers) until later specs.
- Page data comes from typed fixtures in `shared/fixtures/` (no mock HTTP services, no network calls). Charts are hand-built inline SVG — no chart library.
- Taiga UI components are wrapped inside project components in `shared/components/` and `layout/` — no raw Taiga styling in feature pages.

## Recent Changes

- 003-helix-dashboard-visuals: Helix Admin visual system (tokens, shell, 8 tenant pages, 4 auth screens, fixtures, SignalStores) — see `specs/003-helix-dashboard-visuals/plan.md`.
- 002-angular-frontend-foundation: frontend foundation plan (Angular 22 modernization, Taiga UI, NgRx, layered structure) — see `specs/002-angular-frontend-foundation/plan.md`.
- 001-ai-customer-service-platform: platform-wide spec/plan/tasks; backend scaffolding and original frontend workspace.
