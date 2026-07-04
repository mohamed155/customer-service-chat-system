# Quickstart: Angular Frontend Foundation

**Feature**: 002-angular-frontend-foundation | Validates the foundation end-to-end once implemented.

## Prerequisites

- Node.js ≥ 22 (verified working: v25.9.0)
- pnpm 10 (`corepack enable` or `npm i -g pnpm`)
- No backend required — this feature makes no real API calls.

## Setup

```bash
cd frontend
pnpm install
```

## Run the app (SC-001)

```bash
pnpm ng serve dashboard
# open http://localhost:4200
```

Expected:

1. `/` redirects to `/tenant/overview-placeholder`, rendered inside the dashboard shell (sidebar + topbar + main content, semantic landmarks).
2. `/platform/overview-placeholder` renders in the same shell; `/auth/login-placeholder` renders in the minimal auth layout without dashboard chrome.
3. Any unknown URL (e.g. `/nope`) shows the not-found page with a working link back to the tenant overview.

## Validate state & theming (SC-003)

1. Click the sidebar toggle in the topbar → sidebar animates between 280px and 72px widths. With Redux DevTools open (dev build), observe the `[App UI] sidebarToggled` action and state change.
2. Switch theme via the topbar control: light → dark → system. The page colors flip (`data-theme` on `<html>`, `tuiTheme` on `<tui-root>`).
3. Set dark, reload the page → dark restored (localStorage `app.themeMode`); sidebar returns to expanded.
4. Narrow the window below the breakpoint (1024px) → sidebar defaults to collapsed, still toggleable.
5. In a production build (`pnpm ng serve dashboard --configuration production` or served `dist/`), Redux DevTools shows no store instance (devtools disabled).

## Validate lazy loading (SC-005)

```bash
pnpm ng build dashboard
ls dist/dashboard/browser   # expect separate chunk files for auth / platform / tenant / not-found
```

Or in DevTools Network tab: visiting `/tenant/...` must not fetch the auth/platform chunks.

## Run quality gates (SC-004)

```bash
pnpm ng test dashboard     # Vitest: boot, routes, shell, appUi reducer/selectors, error mapper, guard
pnpm lint                  # angular-eslint: zero errors
pnpm format:check          # Prettier: zero diffs
```

All three must exit 0.

## Validate error foundation (SC-008)

Covered by unit tests (no manual step): `mapHttpError` suite feeds server-error envelope, network failure, malformed body, and unknown inputs; each yields a typed `ApiError` and `userMessageFor()` returns safe copy. Confirm via the test output listing the mapper suite.

## References

- Interfaces & route map: [contracts/frontend-foundation.md](./contracts/frontend-foundation.md)
- State/model shapes: [data-model.md](./data-model.md)
- Decisions & versions: [research.md](./research.md)
