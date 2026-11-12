# Frontend Agent Context

## Frontend rules (from spec 002)

- Layers inside `apps/dashboard/src/app/`: `core/` (singletons, no feature deps), `shared/` (reusable, no business logic), `layout/`, `features/{auth,platform,tenant}` (lazy route areas), `design-system/` (`--app-*` tokens, light/dark via `data-theme` + Taiga `tuiTheme`).
- State: global cross-feature → NgRx Store (`appUi` slice: themeMode/sidebarCollapsed); feature-local → NgRx SignalStore; component-temporary → `signal()`. Never duplicate state across mechanisms. Theme persists to localStorage; sidebar does not.
- HTTP: typed `ApiResponse<T>`/`ApiError`/`PaginatedResponse<T>`/`ApiListQuery` aligned with `specs/001-ai-customer-service-platform/contracts/rest-api.md` (cursor pagination, error envelope, `X-Request-Id`). Functional interceptors only. No fake auth logic.
- **Module boundaries** (`local/feature-boundaries`, `tools/eslint-rules/`): a feature (`features/tenant/<area>`, `features/platform/<area>`, `features/auth`, ...) never imports another feature; what two features need moves to `shared/` or `core/` (an API service two features call lives in `core/api/`). Presentational code (`shared/`, `core/ui/`) never imports a store, API service or facade. The area route files (`tenant.routes.ts`) may import their own area's features.
- **Request ID** (MOH-64): `requestIdInterceptor` puts `X-Request-ID: req_<uuid>` on every API call (the form the backend keeps, ADR-012). A mapped `ApiError.requestId` is the body's `request_id`, else the server's `X-Request-ID`, else the ID the call went out with (so a network failure has one too). Show it with `<app-error-reference>` or `<app-inline-alert [requestId]="error.requestId">`; a store that keeps only the message string cannot, so keep the `ApiError` (or its `requestId`) beside it.
- Route paths come from `APP_PATHS` constants in `core/router` — no string literals in features.
- `apps/widget` is a standalone Angular application for the embeddable website chat widget:
  - Standalone components, OnPush, signals
  - No imports from `libs/*`, Taiga UI, or NgRx
  - Own `--wgt-*` CSS tokens
  - 97 KB initial budget (configured in angular.json)
  - Loader build script: `pnpm build:widget-loader`
  - Loader-vs-app split: loader owns launcher + iframe lifecycle, Angular app owns chat UI
- **CSP**: the dashboard document is served with `script-src 'self'` (`infra/Caddyfile`, enforced). No inline `<script>`, no inline event handlers (`onclick=`, `onload=`), no `eval`; scripts that must run before bootstrap go in a file (see `theme-init.js`). Production builds keep `optimization.styles.inlineCritical` off, because Angular's critical-CSS swap is an inline script. `'unsafe-inline'` is allowed on `style-src` only. `e2e/csp-enforcement.spec.ts` fails on any violation, and the mocked E2E suite runs under the policy.
- Quality gates (run in `frontend/`): `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm ng build widget`, `pnpm build:widget-loader`, `pnpm lint`, `pnpm format:check` — all must pass.
- **Coverage** (MOH-66): `pnpm test:coverage` runs both unit-test suites with coverage and fails when a number falls below `tools/coverage-baseline.json` (all four metrics for each app, plus lines for the dashboard's `core/` and for its stores; the target for those two is 80% of lines). The baseline only goes up: after adding tests, `pnpm test:coverage --update` raises it. CI uploads the HTML and lcov report as the `frontend-coverage` artifact.

## Frontend rules (from spec 003)

- Helix visual system: `--app-*` tokens rewritten to the Helix palette/layout (sidebar 248/68px, topbar 60px, content max 1320px); light/dark values live in `design-system/theme/themes.css`, theme-independent values in `design-system/tokens/tokens.css`. Old `--app-color-*` names are gone. Reference design: `Helix Admin.html` at repo root (compare only — never copy markup/assets/fonts from it).
- Theme toggle cycles light → dark → system. Topbar search and "New" are purely visual (no handlers) until later specs.
- Page data comes from typed fixtures in `shared/fixtures/` (no mock HTTP services, no network calls). Charts are hand-built inline SVG — no chart library.
- Taiga UI components are wrapped inside project components in `shared/components/` and `layout/` — no raw Taiga styling in feature pages.

## Switching workspace (MOH-61, ADR-003 section 2)

- The workspace switch happens in place, so nothing on screen may keep the old workspace's data. **Every store that holds workspace data ends with `withTenantScope({ initialState, reload })`** (`core/state/tenant-scope.ts`): it resets the store in the same call as the switch and reloads for the new workspace. `pnpm lint:tenant-scope` fails a store under `features/tenant/` or in root without it.
- State a component holds itself (an open dialog, a picked row) uses `onTenantChange()` from the same file. Do not watch `selectActiveTenant` in an `effect()` for this: it runs a render late.
- `/tenant/` requests of the old workspace are cancelled on a switch, and the realtime stream reconnects. Specs that need the active workspace to change use `provideTenantStore()` and `tenantStore()` (`core/state/testing/tenant-store.testing.ts`), not a fake `Store`.

## Notifications

- Shared components: `shared/components/notification-bell/` (bell icon with badge, `count` input, `toggle` output), `shared/components/notification-list/` (scrollable list with relative timestamps, mark-read, load-more, empty/loading states; `items`/`loading`/`hasMore` inputs, `itemClick`/`markRead`/`loadMore` outputs).
- Core store: `core/notifications/notifications.store.ts` — NgRx SignalStore (`items`, `unreadCount`, `loading`, `nextCursor`, `hasMore`) with `loadFirstPage`, `loadMore`, `markRead`, `markAllRead`, `refreshUnreadCount`, `setUnreadCount` methods. Provided in root.
- SSE live updates: `core/realtime/notifications.service.ts` — subscribes to `notification.created` / `notification.cleared` SSE events, updates store via `setUnreadCount`.
- **Badge rule**: the badge is SET from `unreadCount` (absolute), never incremented. SSE events carry the authoritative count from the server.
- Wire types and mapper in `core/api/tenant-api.models.ts` (`NotificationWire`, `NotificationEntry`, `notificationFromWire`, `notificationListFromWire`).
- Fixtures in `shared/fixtures/notification.fixtures.ts`.
- Tenant feature page: `features/tenant/notifications/notifications-page.component.ts`.

## Audit Logs

- Shared components: `shared/components/audit-log-table/` (table with Time/Actor/Action/Target/Tenant columns, clickable rows, empty/loading states), `shared/components/audit-detail-drawer/` (drawer-right dialog with definition list and pretty-printed JSON metadata).
- Tenant feature: `features/tenant/audit-logs/` — API service, SignalStore, component with category/date/actor filters.
- Platform feature: `features/platform/audit-logs/` — same layout plus tenant filter and `showTenantColumn="true"`.
- Wire types and mapper in `core/api/tenant-api.models.ts` (`AuditEntryWire`, `AuditListWire`, `auditListFromWire`).
- Fixtures in `shared/fixtures/audit.fixtures.ts`.

## Integrations

- Tenant feature: `features/tenant/integrations/` — `integrations-api.service.ts` (typed HTTP), `integrations.store.ts` (catalog list SignalStore), `integration-detail.store.ts` (detail store with `load` / `connect` / `updateConfig` / `disconnect` / `loadMoreEvents` rxMethods), `integration-detail.component.ts` (read-only detail + connect/update form + event log with "Load more").
- Wire types and mappers in `core/api/tenant-api.models.ts` (`IntegrationListItemWire`, `IntegrationDetailWire`, `IntegrationEventWire` + camelCase models + `integrationListFromWire` / `integrationDetailFromWire` / `integrationEventListFromWire`).
- Shared utilities: `shared/utils/clipboard.ts` (`copyToClipboard`, used by the webhook-URL copy button) and `shared/utils/relative-time.ts` (`relativeTime`, used by the event log).
- Routes: `APP_PATHS.tenant.integrations` (list) and `APP_PATHS.tenant.integrationDetail` (`'integrations/:slug'`, detail) — both guarded by `integrations.view` via `PAGE_PERMISSIONS`.
- Permissions: `integrations.view` (list, detail, events) and `integrations.manage` (connect, update, disconnect) — Owner/Admin/Manager manage, Viewer read-only, Agent denied.

## User-facing text (MOH-41, ADR-016)

- New user-facing text goes in a message catalog, not in a template. The widget uses `I18n` (`apps/widget/src/i18n`: `i18n.t('chat.close')`, English in `messages.ts`, other locales are `public/i18n/<locale>.json` loaded with `?lang=`). The dashboard moves to Transloco, area by area (ADR-016 section 2).
- `pnpm lint:i18n` fails a template that gains hard-coded text (text nodes and literal `placeholder`, `title`, `aria-label`, `alt` values). The number allowed per file is in `tools/i18n-baseline.json` and may only go down: after moving text out of a file, run `pnpm lint:i18n --update`.
- Show an API error by its `code` (a catalog key), never the backend's `message`.

## Two-step verification (MOH-34, spec 034)

- The password step can end in `mfa-required` (a `202` and a challenge cookie, no session): `AuthService.login` returns it and `/auth/mfa` completes the sign-in. A session that lacks a second factor on a deployment that requires one gets `403 mfa_enrolment_required`; `apiErrorInterceptor` sends the user to `/mfa-setup`.
- A sensitive action answers `401 mfa_step_up_required`. `stepUpInterceptor` (outside `apiErrorInterceptor`, so it sees the mapped error) opens the code prompt (`StepUpService` and the dialog in the app root) and sends the original request once more. Code that can trigger it needs no special handling.
- Codes and recovery codes are never stored in the app; recovery codes are shown once and cleared from memory afterwards. New API code uses the generated types.
