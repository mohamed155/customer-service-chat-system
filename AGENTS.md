# AI Customer Service Platform — Agent Context

## Active Technologies

- **Backend** (`backend/`): Rust (edition 2024), Axum, Tokio, SQLx, PostgreSQL, Redis, pgvector. Cargo workspace under `backend/crates/`.
- **Frontend** (`frontend/`): Angular 22 (standalone, signals, zoneless, OnPush), TypeScript ~6.0, pnpm workspace (`apps/dashboard`, `apps/widget`, `libs/*`). NgRx 21 (Store/Effects/Signals — peer-dep allowance on Angular 22), Taiga UI 5 as primary UI library, Vitest via `@angular/build:unit-test`, angular-eslint + Prettier.
- **Specs**: Spec-Kit workflow in `specs/`; constitution in `.specify/memory/constitution.md`.

## Frontend rules (from spec 002)

- Layers inside `apps/dashboard/src/app/`: `core/` (singletons, no feature deps), `shared/` (reusable, no business logic), `layout/`, `features/{auth,platform,tenant}` (lazy route areas), `design-system/` (`--app-*` tokens, light/dark via `data-theme` + Taiga `tuiTheme`).
- State: global cross-feature → NgRx Store (`appUi` slice: themeMode/sidebarCollapsed); feature-local → NgRx SignalStore; component-temporary → `signal()`. Never duplicate state across mechanisms. Theme persists to localStorage; sidebar does not.
- HTTP: typed `ApiResponse<T>`/`ApiError`/`PaginatedResponse<T>`/`ApiListQuery` aligned with `specs/001-ai-customer-service-platform/contracts/rest-api.md` (cursor pagination, error envelope, `X-Request-Id`). Functional interceptors only. No fake auth logic.
- Route paths come from `APP_PATHS` constants in `core/router` — no string literals in features.
- `apps/widget` and `libs/*` (Helix hx- components) are prior scaffolding — do not modify or use for the dashboard; Taiga UI only.
- Quality gates (run in `frontend/`): `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm test:e2e`, `pnpm lint`, `pnpm format:check` — all must pass.

## Recent Changes

- 006-multi-tenancy-foundation: tenant isolation (identity/tenancy modules, tenant-context middleware, X-Tenant-ID contract, 4 endpoints, 25 integration tests, audit) + frontend (NgRx tenantContext feature, interceptors, CurrentUserService, TenantContextService, tenant switcher, area guards, dev identity header). See `specs/006-multi-tenancy-foundation/plan.md`.
- 002-angular-frontend-foundation: frontend foundation plan (Angular 22 modernization, Taiga UI, NgRx, layered structure) — see `specs/002-angular-frontend-foundation/plan.md`.
- 001-ai-customer-service-platform: platform-wide spec/plan/tasks; backend scaffolding and original frontend workspace.

<!-- SPECKIT START -->
For additional context about technologies to be used, project structure,
shell commands, and other important information, read the current plan
at specs/017-ai-agent-config/plan.md
<!-- SPECKIT END -->
