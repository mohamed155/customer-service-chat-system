# Contract: Frontend Foundation Interfaces

**Feature**: 002-angular-frontend-foundation | **Date**: 2026-07-04

These are the interfaces the foundation exposes to future feature specs (its "consumers"). Changing any of them after this feature ships is a breaking change for downstream specs and must be called out.

## 1. Route contract

| Path | Behavior | Chunk |
|---|---|---|
| `/` | Redirect → `/tenant/overview-placeholder` | eager (redirect only) |
| `/auth` | Auth area; renders minimal centered layout (no dashboard chrome). `/auth` (empty child) redirects → `login-placeholder` | lazy `auth` |
| `/auth/login-placeholder` | Placeholder login page | lazy `auth` |
| `/platform` | Platform area inside dashboard shell; empty child redirects → `overview-placeholder` | lazy `platform` |
| `/platform/overview-placeholder` | Placeholder platform overview page | lazy `platform` |
| `/tenant` | Tenant area inside dashboard shell; empty child redirects → `overview-placeholder` | lazy `tenant` |
| `/tenant/overview-placeholder` | Placeholder tenant overview page | lazy `tenant` |
| `*` (any unknown) | Minimal not-found page with a link to `/tenant/overview-placeholder` | lazy `not-found` |

Guard seam: functional pass-through guard (always `true`, no auth simulation) attached to the platform and tenant area routes, named so the auth spec can replace its body. Route paths are exported as `APP_PATHS` constants from `core/router` — features must use the constants, never string literals.

## 2. Global state contract (`appUi` slice)

```ts
// Feature key: 'appUi'
interface AppUiState {
  themeMode: 'light' | 'dark' | 'system'; // default 'system'
  sidebarCollapsed: boolean;              // default false
}

// Actions (source: 'App UI')
themeModeChanged({ themeMode: ThemeMode })
sidebarToggled()
sidebarCollapsedSet({ collapsed: boolean })

// Selectors
selectThemeMode(): Signal<ThemeMode>        // via store.selectSignal
selectSidebarCollapsed(): Signal<boolean>
selectResolvedTheme(): Signal<'light' | 'dark'>  // 'system' resolved against OS preference
```

Persistence: `themeMode` ↔ localStorage key `app.themeMode` (validated on read; invalid → `'system'`). Nothing else is persisted.

State placement rules (binding on future specs):

| State kind | Mechanism |
|---|---|
| Global cross-feature (session, tenant context, theme, notifications) | NgRx Store |
| Feature-local interactive (inbox filters, editor state) | NgRx SignalStore |
| Component-only temporary UI | Angular `signal()` |

The same piece of state must never live in two mechanisms.

## 3. HTTP contract (models per [data-model.md](../data-model.md))

- All API calls go through `ApiService` (`core/api`) which prefixes `AppConfig.apiBaseUrl`.
- Interceptor chain (order): `authTokenInterceptor` (no-op placeholder) → `apiErrorInterceptor` (normalizes every failure to `ApiError`).
- Consumers catch typed `ApiError` — never `HttpErrorResponse` — and render user copy only via `userMessageFor()`.
- Shapes mirror the backend REST contract (`specs/001-ai-customer-service-platform/contracts/rest-api.md`): error envelope, cursor pagination (`items`/`nextCursor`/`hasMore`), `X-Request-Id` propagation.

## 4. Environment contract

```ts
// environments/environment.ts (production defaults)
{ apiBaseUrl: '/api/v1', appName: 'AI Customer Service Platform',
  environmentName: 'production', enableNgRxDevtools: false }

// environments/environment.development.ts (via fileReplacements)
{ apiBaseUrl: 'http://localhost:8080/api/v1', appName: 'AI Customer Service Platform',
  environmentName: 'development', enableNgRxDevtools: true }
```

Injected via `APP_CONFIG` token; components/services must inject the token, never import environment files directly. No secrets ever.

## 5. Design token contract (`--app-*` namespace)

| Group | Tokens (representative) |
|---|---|
| Layout | `--app-sidebar-width: 280px`, `--app-sidebar-collapsed-width: 72px`, `--app-topbar-height: 64px`, `--app-page-max-width: 1440px` |
| Spacing | `--app-space-1..8` (4px scale) |
| Radius | `--app-radius-sm: 6px`, `--app-radius-md: 10px`, `--app-radius-lg: 16px` |
| Shadows | `--app-shadow-sm/md/lg` |
| Z-index | `--app-z-sidebar`, `--app-z-topbar`, `--app-z-overlay`, `--app-z-toast` |
| Breakpoints | `--app-bp-sm/md/lg/xl` (documented values; media queries use the literal values, TS reads them from a mirrored const). `--app-bp-lg: 1024px` is the designated sidebar-collapse breakpoint, mirrored as `LAYOUT_COLLAPSE_BREAKPOINT` in TS |
| Typography | `--app-font-family`, `--app-text-xs..xl`, `--app-font-weight-*` |
| Theme colors | `--app-color-bg`, `--app-color-surface`, `--app-color-border`, `--app-color-text`, `--app-color-text-muted`, `--app-color-accent`, semantic `--app-color-success/warning/danger` — values differ under `:root[data-theme='dark']` |

Theme switching: TS resolves `themeMode` → sets `data-theme="light|dark"` on `<html>` **and** `[attr.tuiTheme]` on `<tui-root>`, so app tokens and Taiga tokens flip together. Components use tokens or Taiga variables — no hardcoded app-specific values.

## 6. Quality gates contract (commands)

Run from `frontend/`:

| Command | Must |
|---|---|
| `pnpm ng build dashboard` | Succeed; produce separate lazy chunks per area |
| `pnpm ng test dashboard` | All Vitest suites pass |
| `pnpm lint` | Zero errors (angular-eslint, TS + templates) |
| `pnpm format:check` | Zero diffs (Prettier) |
