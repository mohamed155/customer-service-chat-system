# Frontend workspace

This pnpm workspace contains the `dashboard` and `widget` Angular applications. Run commands from this directory. The dashboard is an Angular 22, standalone, zoneless application; the widget remains an independent application.

## Dashboard architecture

Dashboard source lives in `apps/dashboard/src/app`:

- `core`: singleton configuration, global state, routing seams, HTTP, errors, and logging. It never imports features or layout.
- `shared`: feature-neutral reusable presentation and utilities. It never imports features, layout, or core state.
- `layout`: shell composition and viewport behavior.
- `features`: lazy route areas. Features may consume core, shared, and layout components.
- `design-system`: application tokens and theme definitions.

ESLint enforces the core/shared dependency direction. Keep feature code inside its route area and do not create empty placeholder directories.

## State placement

| State scope               | Mechanism          | Example                              |
| ------------------------- | ------------------ | ------------------------------------ |
| Cross-feature/global      | NgRx Store         | theme, session, tenant context       |
| Feature-local interactive | NgRx SignalStore   | shell viewport, future inbox filters |
| Component-only temporary  | Angular `signal()` | one request's loading flag           |

Never copy the same state into multiple mechanisms. `appUi` owns theme and sidebar collapse. `LayoutStore` owns only viewport width and dispatches global sidebar actions.

## UI and theming

Taiga UI is the primary component library. Compose its components directly; do not start a custom component library. Add a wrapper only after a stable repeated product pattern exists.

Application values use `--app-*` tokens from `design-system`. Do not hardcode app-specific color, spacing, radius, or layout values in components. Theme selection updates both `data-theme` on `<html>` and `tuiTheme` on `<tui-root>` so application and Taiga tokens stay aligned.

## HTTP, errors, and loading

Use `ApiService` for backend calls. It prefixes `APP_CONFIG.apiBaseUrl`, captures `X-Request-Id`, and exposes typed models. Interceptors run in this order: the no-op authentication extension point, then error normalization. Feature code handles `ApiError` and renders only `userMessageFor(error)` output, never raw backend messages.

Loading state stays local to the operation: `readonly loading = signal(false)`. Render `LoadingIndicatorComponent` while active. Do not add a global loading slice.

## Testing

Tests run with Vitest through Angular's unit-test builder. Unit-test reducers, selectors, effects, mappers, and guards. Use Angular component tests for DOM behavior and RouterTestingHarness for route integration. Tests must assert behavior rather than snapshots alone.

## Commands

```bash
pnpm start
pnpm ng build dashboard
pnpm ng build widget
pnpm ng test dashboard
pnpm ng test widget
pnpm lint
pnpm format:check
```
