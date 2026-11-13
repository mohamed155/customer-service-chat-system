# Frontend Agent Context

## Where code lives

- The dashboard's code is all under `apps/dashboard/src/app`: `core/`, `shared/`, `layout/` and
  `features/` (see the layers below). There are no libraries for features: the seven placeholder
  libs (`data-access`, `feature-*`, `realtime`) were empty and were removed (MOH-63).
- `libs/ui` is the only library left: Helix components that nothing imports, not built and not
  tested by `ng test` (its README says so). Do not add to it; shared dashboard code goes in
  `shared/` or `core/`.
- `apps/widget` is its own app (see below) and imports nothing from `libs/`.

## Frontend rules (from spec 002)

- Layers inside `apps/dashboard/src/app/`: `core/` (singletons, no feature deps), `shared/` (reusable, no business logic), `layout/`, `features/{auth,platform,tenant}` (lazy route areas), `design-system/` (`--app-*` tokens, light/dark via `data-theme` + Taiga `tuiTheme`).
- State: global cross-feature → NgRx Store (`appUi` slice: themeMode/sidebarCollapsed); feature-local → NgRx SignalStore; component-temporary → `signal()`. Never duplicate state across mechanisms. Theme persists to localStorage; sidebar does not.
- HTTP: typed `ApiResponse<T>`/`ApiError`/`PaginatedResponse<T>`/`ApiListQuery` aligned with `specs/001-ai-customer-service-platform/contracts/rest-api.md` (cursor pagination, error envelope, `X-Request-Id`). Functional interceptors only. No fake auth logic.
- **Module boundaries** (`local/feature-boundaries`, `tools/eslint-rules/`): a feature (`features/tenant/<area>`, `features/platform/<area>`, `features/auth`, ...) never imports another feature; what two features need moves to `shared/` or `core/` (an API service two features call lives in `core/api/`). Presentational code (`shared/`, `core/ui/`) never imports a store, API service or facade. The area route files (`tenant.routes.ts`) may import their own area's features.
- **Request ID** (MOH-64): `requestIdInterceptor` puts `X-Request-ID: req_<uuid>` on every API call (the form the backend keeps, ADR-012). A mapped `ApiError.requestId` is the body's `request_id`, else the server's `X-Request-ID`, else the ID the call went out with (so a network failure has one too). Show it with `<app-error-reference>` or `<app-inline-alert [requestId]="error.requestId">`; a store that keeps only the message string cannot, so keep the `ApiError` (or its `requestId`) beside it.
- **Timeout and retry** (MOH-166, `core/http/http-resilience*`): an API call that gets no answer for 30 s fails with `code: 'timeout'`; a GET/HEAD is retried (at most twice, capped exponential backoff with full jitter) after a dropped connection, 502, 503 or 504, and after a 429 that carries `Retry-After`, never after another 4xx and never a write. A timeout is not retried. An upload or other long call opts out with `noTimeout()` as the `context` of the request (`api.post(path, body, noTimeout())`); streaming goes through `fetch`, not `HttpClient`, so it is not covered.
- **Stale lazy chunks** (MOH-167, `core/version/`): after a deploy, a page opened before it asks for lazy chunks that are gone. The router's navigation-error handler (`recoverFromChunkLoadError`, in `app.config.ts`) loads the target URL again, once; the guard is in `sessionStorage` (`app.chunk-reload`, 60 s), so a second failure shows the update notice (`<app-update-banner>` in the app root) instead of looping. `AppUpdateService` also compares the hashed file names in `index.html` every 5 minutes (and when the tab returns to the foreground) with the ones the page loaded, and offers a reload. This depends on the web server: `infra/nginx/spa.conf` answers a missing `.js`/`.css` with 404, never `index.html`, and `e2e/support/static-server.mjs` does the same. Do not give the dashboard a server that falls back to `index.html` for files.
- **Browser error reports** (MOH-168, `core/errors/client-error-reporter.ts`): `GlobalErrorHandler` (which Angular feeds with `error` and `unhandledrejection` events) and the chunk-load recovery hand every unhandled error to `ClientErrorReporter`, which sends it to `POST /api/v1/client-errors`: scrubbed, deduplicated, batched every 5 s (10 reports a request), at most 20 a minute and 100 a page session, paused for a minute after a `401`, `403` or `429`. It uses `fetch` with `keepalive` and the `X-Requested-With` header, never `HttpClient` (the interceptors would retry it and report its own failure), and only for a signed-in user. **What may leave the browser** is fixed in `core/errors/scrub.ts`: a message and stack run through `scrubText` (URL queries and fragments, tokens, JWTs, e-mail addresses, values of password-like keys, UUIDs and long path segments are removed), an `ApiError` is described by its code and never by the backend's message, an `HttpErrorResponse` by its status, any other thrown value by its type. Never add a form value, a request or response body, a cookie or a storage value to a report. Report the route as a pattern (`tenant/customers/:id`), never the URL. The backend log line (target `frontend`) and the alerts are in `docs/runbooks/operations.md`, "Frontend errors". The production build writes **hidden** source maps; `infra/Dockerfile.frontend` moves them out of the served image (nginx also refuses `.map`) and the release job publishes them as a private package; read a stack with `node tools/symbolicate.mjs <maps-dir> <stack-file>`.
- **Generated API types** (MOH-169, ADR-012 gap A6): code in `core/` and `layout/` types its API calls with `Schemas['…']` from `core/api/generated/api.ts` (`type Schemas = components['schemas']`), never a hand-written wire type: `pnpm lint:api-types` (CI) fails `api.get<MyType>(…)` there. The contract types roles, permissions, status and plan as plain strings; where the app needs its narrower unions, map the wire object in a `…FromWire` function in `tenant-api.models.ts` (`meFromWire`, `tenantSummaryFromWire`), the one place with the type assertions. A wire type that has a generated twin is an alias of it (`SignupRequest`, `Availability`). Regenerate after a backend change with `pnpm gen:api`. A file that cannot follow this yet goes in the check's `EXEMPT` list with the issue that removes it.
- **Realtime** (MOH-65, `core/realtime/realtime.service.ts`): the tenant event stream reconnects after a **full-jitter** wait (`reconnectDelay`: random between 0 and `min(30 s, 1 s * 2^attempt)`), the count starts over after a successful connection, and `disconnect()` cancels a waiting reconnect. `/tenant/events` is an in-process broadcast with no event log and no ids to resume from, so the server ignores `Last-Event-ID`: events sent while the stream was down are lost, and a screen that must be right after a gap has to reload its data. The app and its tests are zoneless and nothing may use `NgZone`. `zone.js` is a **dev** dependency only (no runtime dependency, no polyfill, nothing in the bundle): Angular's unit-test builder looks for it and, when it finds it, imports `zone.js/testing` only if a `Zone` exists, which it never does here; with `zone.js` missing from the install the builder still tries to resolve it and every spec fails to load (found in CI on PR #92).
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

## Trusted Types and HTML (MOH-170, architecture §13.2)

- The dashboard document is served with `require-trusted-types-for 'script'; trusted-types angular angular#unsafe-bypass` (`infra/Caddyfile`, kept equal to `config::DEFAULT_CSP_POLICY` by a backend test). The browser blocks any string written to an HTML or script sink (`innerHTML`, `insertAdjacentHTML`, `iframe.srcdoc`, `script.textContent`, ...) unless Angular's own policy made it. Do not create other policies: the list has no other name. Render with templates and `textContent`.
- `pnpm lint` runs `html-safety/unsafe-html` (`tools/eslint-rules/unsafe-html.mjs`). It flags `bypassSecurityTrust*()`, assignments to `innerHTML` / `outerHTML` / `srcdoc`, `insertAdjacentHTML()`, `document.write()` and `[innerHTML]` / `[outerHTML]` / `[srcdoc]` in dashboard templates. It cannot tell a script element's `text` / `textContent` / `src` from the same names on other elements; the browser blocks those and the e2e guard fails a spec that hits one. A real need is allowed with a disable comment that says why the value is safe (`// eslint-disable-next-line html-safety/unsafe-html -- reason`, in a template `<!-- eslint-disable-next-line html-safety/unsafe-html-template -- reason -->`). Specs and the widget are not checked.
- Knowledge article bodies: **the server sanitises**. `validate::sanitize_body` (`ammonia::clean`) runs on every create and update in `backend/crates/modules/knowledge/src/routes.rs`, so the stored body is already clean (documents have no body). The article page binds `[innerHTML]` and Angular sanitises it a second time. Neither side is a reason to use `bypassSecurityTrustHtml`.
- Every mocked e2e spec imports `test` from `e2e/support/test.ts`, which fails the test on any CSP or Trusted Types violation (the static server sends the production policy). Specs that provoke a violation on purpose use `@playwright/test` directly, as `csp-enforcement.spec.ts` does. Add a route to that spec when you add one to the router.
- `provideTaiga({ fontScaling: false })`: Taiga's text-scale watcher writes a `<meta>` tag into a hidden iframe with `insertAdjacentHTML`. Trusted Types blocks that, and a policy cannot cover it (the iframe is its own context), which stopped the whole app from starting. The cost: the dashboard no longer follows the browser's preferred text scale (iOS Dynamic Type, Chrome on Android). Browser zoom and `rem` sizes are unaffected. Turn it back on only with a Taiga release that builds the tag without a string sink.
- The widget loader (`apps/widget/loader`) sets its button icon with `innerHTML` on the host page. A host page that enforces Trusted Types would block it; this is outside the dashboard policy and is not changed here.
