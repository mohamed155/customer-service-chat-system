# Data Model: Angular Frontend Foundation

**Date**: 2026-07-04 | **Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

No backend entities are created by this feature. The "data model" is the set of typed frontend structures the foundation defines.

## State

### AppUiState (global NgRx slice `appUi`)

| Field | Type | Default | Rules |
|---|---|---|---|
| `themeMode` | `ThemeMode = 'light' \| 'dark' \| 'system'` | `'system'` | Hydrated from localStorage key `app.themeMode` at store init; invalid/missing stored value → `'system'`. Persisted on every change (theme only). |
| `sidebarCollapsed` | `boolean` | `false` (expanded) | Never persisted; resets on reload. Defaults to `true` when viewport < breakpoint token at shell init. |

**Transitions** (actions, via `createActionGroup` source "App UI"):

- `themeModeChanged({ themeMode })` → sets `themeMode`; effect writes localStorage.
- `sidebarToggled()` → flips `sidebarCollapsed`.
- `sidebarCollapsedSet({ collapsed })` → sets explicit value (used by breakpoint rule).

**Selectors**: `selectThemeMode`, `selectSidebarCollapsed`, plus derived `selectResolvedTheme: 'light' | 'dark'` (combines `themeMode` with the current system preference signal).

### LayoutStore (feature-local SignalStore, provided in AppShell)

| Field | Type | Notes |
|---|---|---|
| `viewportWidth` | `number` | Updated from a resize listener (rxMethod / effect). |
| `isNarrow` | computed `boolean` | `viewportWidth < LAYOUT_COLLAPSE_BREAKPOINT` (exported TS const `1024`, mirroring the `--app-bp-lg` token). Crossing into narrow dispatches `sidebarCollapsedSet({ collapsed: true })`. |

Rule enforced: LayoutStore never stores a copy of `sidebarCollapsed` — it reads the global signal and only dispatches.

## HTTP / API models (`core/api`)

```ts
type ThemeMode = 'light' | 'dark' | 'system';

// NOTE: ApiResponse is a CLIENT-SIDE construct assembled by ApiService (body + X-Request-Id
// header). It is NOT a backend wire format — 2xx bodies are plain resources per the REST contract.
interface ApiResponse<T> {
  data: T;
  requestId?: string;          // from X-Request-Id when present
}

interface ApiErrorDetail {
  field?: string;
  code: string;
  message: string;
}

interface ApiError {
  code: string;                // backend error.code, or synthetic: 'network_error' | 'unknown_error'
  message: string;             // developer-facing; never rendered raw to users
  details?: ApiErrorDetail[];
  requestId?: string;          // backend error.request_id
  status: number;              // HTTP status; 0 for network failures
}

interface PaginatedResponse<T> {
  items: T[];
  nextCursor: string | null;   // backend next_cursor
  hasMore: boolean;            // backend has_more
}

interface ApiListQuery {
  limit?: number;              // backend caps at 100
  cursor?: string;
  sort?: string;
  order?: 'asc' | 'desc';
  q?: string;
}
```

**Mapping rules** (`mapHttpError`): backend envelope `{ error: { code, message, details, request_id } }` → `ApiError` (snake_case → camelCase); status 0 / ProgressEvent → `code: 'network_error'`; unparsable/missing body → `code: 'unknown_error'` with generic message. `userMessageFor(apiError)` maps codes/status ranges to safe user copy with a generic fallback.

## Configuration (`core/config`)

```ts
interface AppConfig {
  apiBaseUrl: string;          // e.g. '/api/v1' (dev), no secrets
  appName: string;
  environmentName: 'development' | 'production';
  enableNgRxDevtools: boolean;
}
```

Exposed via `APP_CONFIG` injection token whose value comes from `environments/environment*.ts` (Angular `fileReplacements`).

## Routing constants (`core/router`)

```ts
const APP_PATHS = {
  root: '',
  auth: { base: 'auth', loginPlaceholder: 'login-placeholder' },
  platform: { base: 'platform', overviewPlaceholder: 'overview-placeholder' },
  tenant: { base: 'tenant', overviewPlaceholder: 'overview-placeholder' },
  notFound: '**',
} as const;
```

## Design tokens (CSS custom properties, not TS)

Groups (see [contracts/frontend-foundation.md](./contracts/frontend-foundation.md) for the full list): layout sizes, spacing scale, radius, shadows, z-index, breakpoints, typography, theme colors (light/dark via `data-theme`).
