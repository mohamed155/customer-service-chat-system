# Frontend Agent Context

## Frontend rules (from spec 002)

- Layers inside `apps/dashboard/src/app/`: `core/` (singletons, no feature deps), `shared/` (reusable, no business logic), `layout/`, `features/{auth,platform,tenant}` (lazy route areas), `design-system/` (`--app-*` tokens, light/dark via `data-theme` + Taiga `tuiTheme`).
- State: global cross-feature → NgRx Store (`appUi` slice: themeMode/sidebarCollapsed); feature-local → NgRx SignalStore; component-temporary → `signal()`. Never duplicate state across mechanisms. Theme persists to localStorage; sidebar does not.
- HTTP: typed `ApiResponse<T>`/`ApiError`/`PaginatedResponse<T>`/`ApiListQuery` aligned with `specs/001-ai-customer-service-platform/contracts/rest-api.md` (cursor pagination, error envelope, `X-Request-Id`). Functional interceptors only. No fake auth logic.
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

## Frontend rules (from spec 003)

- Helix visual system: `--app-*` tokens rewritten to the Helix palette/layout (sidebar 248/68px, topbar 60px, content max 1320px); light/dark values live in `design-system/theme/themes.css`, theme-independent values in `design-system/tokens/tokens.css`. Old `--app-color-*` names are gone. Reference design: `Helix Admin.html` at repo root (compare only — never copy markup/assets/fonts from it).
- Theme toggle cycles light → dark → system. Topbar search and "New" are purely visual (no handlers) until later specs.
- Page data comes from typed fixtures in `shared/fixtures/` (no mock HTTP services, no network calls). Charts are hand-built inline SVG — no chart library.
- Taiga UI components are wrapped inside project components in `shared/components/` and `layout/` — no raw Taiga styling in feature pages.

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

## Two-step verification (MOH-34, spec 034)

- The password step can end in `mfa-required` (a `202` and a challenge cookie, no session): `AuthService.login` returns it and `/auth/mfa` completes the sign-in. A session that lacks a second factor on a deployment that requires one gets `403 mfa_enrolment_required`; `apiErrorInterceptor` sends the user to `/mfa-setup`.
- A sensitive action answers `401 mfa_step_up_required`. `stepUpInterceptor` (outside `apiErrorInterceptor`, so it sees the mapped error) opens the code prompt (`StepUpService` and the dialog in the app root) and sends the original request once more. Code that can trigger it needs no special handling.
- Codes and recovery codes are never stored in the app; recovery codes are shown once and cleared from memory afterwards. New API code uses the generated types.
