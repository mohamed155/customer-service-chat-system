# UI Contract: Helix Admin Dashboard Visual System

**Feature**: 003-helix-dashboard-visuals | **Date**: 2026-07-06

The externally observable surface this feature exposes: routes, title mapping, design-token names, component APIs, and state contracts. Implementation must not deviate without updating this contract.

## 1. Route contract

All paths sourced from `core/router/app-paths.ts` (`APP_PATHS`). No string literals in features.

| Route | Lazy page | Shell | Title / Subtitle |
|-------|-----------|-------|------------------|
| `/` | redirect → `/tenant/overview` | — | — |
| `/tenant/overview` | tenant/overview | Helix shell | Overview / *{formatted date}* · Your support cockpit |
| `/tenant/conversations` | tenant/conversations | Helix shell | Conversations / Shared inbox · 6 open, 2 escalated |
| `/tenant/customers` | tenant/customers | Helix shell | Customers / Customer profiles and conversation history |
| `/tenant/ai-agent` | tenant/ai-agent | Helix shell | AI Agent / Configure how your assistant behaves |
| `/tenant/knowledge-base` | tenant/knowledge-base | Helix shell | Knowledge Base / Train your AI with trusted company knowledge |
| `/tenant/integrations` | tenant/integrations | Helix shell | Integrations / Connect channels and business systems |
| `/tenant/analytics` | tenant/analytics | Helix shell | Analytics / Trends across every channel |
| `/tenant/settings` | tenant/settings | Helix shell | Settings / Workspace preferences and security |
| `/platform/overview-placeholder` | existing placeholder (unchanged content) | Helix shell | Platform / Platform administration |
| `/auth/login` | auth/login | auth layout (no shell) | — |
| `/auth/signup` | auth/signup | auth layout | — |
| `/auth/forgot-password` | auth/forgot-password | auth layout | — |
| `/auth/verify-email` | auth/verify-email | auth layout | — |
| `**` | not-found (restyled) | — | — |

Title/subtitle delivered via route `data.pageTitle` from the typed `PAGE_TITLES` map (`core/router/page-title.ts`); browser tab title via route `title`. Removed: `/tenant/overview-placeholder`, `/auth/login-placeholder` (and their `APP_PATHS` entries).

## 2. Design token contract (`--app-*`)

`tokens.css` (theme-independent) MUST define at minimum:

```text
Layout:      --app-sidebar-expanded-width: 248px; --app-sidebar-collapsed-width: 68px;
             --app-topbar-height: 60px; --app-content-max-width: 1320px;
             --app-page-padding-x: 28px; --app-page-padding-y: 24px;
Radius:      --app-radius-xs: 6px; --app-radius-sm: 8px; --app-radius-md: 10px;
             --app-radius-lg: 13px; --app-radius-xl: 16px;
Spacing:     --app-space-1..8 (4/8/12/16/20/24/28/32px, retained from spec 002)
Type:        --app-font-family ('Geist Sans' + system fallback); --app-font-mono ('Geist Mono' + fallback)
             --app-font-xs: 11px; --app-font-sm: 12.5px; --app-font-md: 13.5px; --app-font-base: 14px;
             --app-font-lg: 16px; --app-font-xl: 20px; --app-font-2xl: 24px
Motion:      --app-transition-fast: 120ms ease; --app-transition-base: 200ms cubic-bezier(0.4, 0, 0.2, 1)
Z-index:     --app-z-sidebar/topbar/overlay/toast (retained)
Breakpoints: --app-bp-sm/md/lg/xl (retained: 640/768/1024/1280px)
```

`themes.css` MUST define per theme (`:root[data-theme='light']`, `:root[data-theme='dark']`) exactly the palette from the spec (verified identical to `Helix Admin.html`):

```text
--app-bg, --app-panel, --app-panel-2, --app-panel-3, --app-sidebar,
--app-border, --app-border-strong, --app-text, --app-text-2, --app-text-3,
--app-accent, --app-accent-strong, --app-accent-ink, --app-accent-soft,
--app-green, --app-green-soft, --app-amber, --app-amber-soft, --app-red, --app-red-soft,
--app-shadow, --app-shadow-lg, color-scheme
```

Old `--app-color-*`, `--app-sidebar-width`, `--app-page-max-width` names are **removed** (hard rename; no aliases). Components MUST consume tokens — no repeated hardcoded colors/dimensions.

## 3. Layout component contract

| Component | Selector | API (signals-based) |
|-----------|----------|---------------------|
| AppShell | `app-shell` | none — reads `appUi` store; grid `auto 1fr`, `height: 100dvh`, no body scroll |
| Sidebar | `app-sidebar` | `collapsed = input<boolean>()`; emits nothing (nav via routerLink, logout visual-only); renders `<aside>` |
| SidebarNavGroup | `app-sidebar-nav-group` | `label = input<string>()`, `collapsed = input<boolean>()`; content-projects nav items; hides label when collapsed |
| SidebarNavItem | `app-sidebar-nav-item` | `icon`, `label`, `link` (from APP_PATHS), `collapsed`, optional `badgeCount = input<number>()`; `routerLinkActive` + `aria-current="page"`; `aria-label` when collapsed |
| Topbar | `app-topbar` | none — reads route title data + `appUi`; renders `<header>`; contains sidebar toggle, title/subtitle, search (visual), theme cycle button, notifications (visual), New (visual) |
| PageContainer | `app-page-container` | wraps page content: max-width 1320px, page padding, centered |
| PageHeader | `app-page-header` | `title`, optional `subtitle` inputs + projected actions slot (in-page section headers) |

## 4. Shared UI component contract (all standalone, OnPush, token-consuming, business-logic-free)

| Component | Key inputs |
|-----------|-----------|
| `app-dashboard-card` | `padding?: 'md' \| 'none'`; projected header/body/footer slots |
| `app-metric-card` | `metric: MetricFixture` (icon, label, value, delta, trend → sparkline) |
| `app-status-badge` | `status: ConversationStatus \| ArticleStatus \| IntegrationStatus \| string`, `tone: 'green'\|'amber'\|'red'\|'accent'\|'neutral'` |
| `app-channel-badge` | `channel: Channel` |
| `app-sentiment-badge` | `sentiment: Sentiment` |
| `app-avatar` | `initials: string`, `size?: 'sm'\|'md'\|'lg'` |
| `app-search-input` | `placeholder`, `value` model signal, optional `shortcutHint` ("⌘K") |
| `app-icon-button` | `icon`, `label` (required — becomes aria-label), `active?` |
| `app-empty-state` | `icon`, `title`, `description`; projected action slot |
| `app-loading-state` | `label?` (absorbs spec-002 loading-indicator) |
| `app-section-header` | `title`, `subtitle?`; projected actions |
| `app-toolbar` | layout wrapper: projected start/end slots |
| `app-data-table` | projected `<table>` content; provides Helix panel/rounded/border/row styles |
| `app-sparkline` | `points: readonly number[]`, `colorToken` |

AI-specific (visual-only, under `shared/components/ai/`): `app-ai-confidence-badge` (`confidence: number`), `app-ai-suggestion-card` (`suggestion: string`; projected actions), `app-ai-thinking-indicator`, `app-ai-tool-timeline` (`steps: readonly {label, detail?}[]`), `app-knowledge-citation` (`titles: readonly string[]`), `app-escalation-banner` (`title`, `description`, dismissible output), `app-agent-preview-card` (fixture transcript), `app-prompt-editor-shell` (`value` model, mono font, line-count gutter visual).

## 5. State contract

- **Global (`appUi`, existing)**: `themeMode` cycles light→dark→system via `appUiActions.themeModeChanged`; `sidebarCollapsed` via `sidebarToggled`/`sidebarCollapsedSet`. No new global slices. No duplication of this state anywhere.
- **SignalStores**: `ConversationsStore`, `AiAgentStore`, `SettingsStore` per data-model.md (component-provided; not root).
- **Persistence**: theme mode only (existing mechanism). Nothing else touches localStorage.

## 6. Interaction & accessibility contract

- Theme toggle: cycles modes, icon reflects current mode (sun/moon/monitor), `aria-label` states current mode.
- Search box, notifications, "New": render + hover/focus states only; no handlers beyond the input's own text entry.
- Sidebar: collapse animates width via `--app-transition-base`; collapsed items expose `aria-label`; Conversations badge shows fixture count.
- Landmarks: `aside` (sidebar), `header` (topbar), `main` (content, focusable for skip). All actions are `<button>`/`<a>`. Focus visible on every interactive element in both themes.
- Overview alert dismiss removes banner for the session (in-memory signal).
- Conversation select/filter, AI Agent tabs, Settings tabs update via their stores; no page reloads; no network requests anywhere (SC-007).

## 7. Quality gates (unchanged commands, run in `frontend/`)

`pnpm ng build dashboard` · `pnpm ng test dashboard` · `pnpm lint` · `pnpm format:check` — all must pass.
