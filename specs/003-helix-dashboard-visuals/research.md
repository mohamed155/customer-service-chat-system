# Research: Helix Admin Dashboard Visual System

**Feature**: 003-helix-dashboard-visuals | **Date**: 2026-07-06

All Technical Context unknowns resolved. Each decision below records what was chosen, why, and what was rejected.

## R1. Reference fidelity source

- **Decision**: Treat the CSS custom properties extracted from `Helix Admin.html` as the authoritative palette. Verification performed: the reference defines `--bg-app/--panel/--panel-2/--panel-3/--sidebar/--border/--border-strong/--text/--text-2/--text-3/--accent(-strong/-ink/-soft)/--green/--amber/--red(+ -soft)/--shadow(-lg)` in light and dark blocks, and every value matches the token table in the feature spec input exactly (e.g., light `--bg-app:#f5f6f8`, dark `--accent:#38bdf8`).
- **Rationale**: Confirms the spec's token values need no reconciliation against the reference; implementation can code straight from the spec table.
- **Alternatives considered**: Screenshot-based color picking (unnecessary — variables are explicit); copying the reference stylesheet (prohibited by FR-032).

## R2. Design token architecture

- **Decision**: Keep the existing two-file token system with the `--app-*` prefix: rewrite `design-system/tokens/tokens.css` (theme-independent: layout, spacing, radius, shadows*, z-index, typography scale, transitions, breakpoints) and `design-system/theme/themes.css` (light/dark palettes under `:root[data-theme='light']` / `:root[data-theme='dark']`, plus `color-scheme`). Shadows move to themes.css because Helix uses different shadow values per theme. Naming maps reference → app tokens: `--bg-app`→`--app-bg`, `--panel`→`--app-panel`, `--text-2`→`--app-text-2`, `--accent-soft`→`--app-accent-soft`, etc. Old token names that no longer exist (`--app-color-bg`, `--app-color-surface`, `--app-sidebar-width`, `--app-page-max-width`, …) are renamed/replaced across all consumers in the same change — no alias layer.
- **Rationale**: Spec permits "equivalent structure if the current architecture already has a token system" (it does, from spec 002); two files with clear theme-dependent/independent split beats seven micro-files for a codebase this size; a hard rename with full-codebase sweep avoids a permanent compatibility shim. CLAUDE.md mandates the `--app-*` prefix.
- **Alternatives considered**: Seven-file split per the feature input (more files, no benefit at current scale — revisit if the token count grows); keeping old names as aliases (creates two names for one value, violates single-source-of-truth).

## R3. Geist font packaging (from Clarification Q2)

- **Decision**: Add `@fontsource/geist-sans` and `@fontsource/geist-mono` as dependencies. Import weights 400/500/600/700 of geist-sans (and 400/500 of geist-mono) in `styles.css` via `@import '@fontsource/geist-sans/400.css';` etc. Set `--app-font-family: 'Geist Sans', system-ui, -apple-system, 'Segoe UI', sans-serif;` and `--app-font-mono: 'Geist Mono', ui-monospace, 'Cascadia Mono', monospace;`. Geist Mono is used by the prompt-editor shell and masked API key field.
- **Rationale**: Fontsource ships the SIL-OFL-licensed Geist as versioned, self-hosted woff2 with per-weight CSS — no runtime CDN (clarification requirement), builder handles asset hashing. The alternative `geist` npm package is Next.js-oriented (exports `next/font` loaders, not plain CSS).
- **Alternatives considered**: Vercel's `geist` package (framework-coupled); manual woff2 vendoring in `public/` (unversioned, manual `@font-face` upkeep); system fonts only (rejected in clarification).

## R4. Icons

- **Decision**: Use the already-installed `@taiga-ui/icons` set exclusively, via `<tui-icon icon="@tui.…" />` inside project components (icon-button, nav-item, badges, metric cards). Concrete mapping (Lucide names): overview `@tui.layout-dashboard`, conversations `@tui.messages-square`, customers `@tui.users`, AI agent `@tui.bot`, knowledge base `@tui.book-open`, integrations `@tui.plug`, analytics `@tui.chart-line`, settings `@tui.settings`, search `@tui.search`, theme `@tui.sun`/`@tui.moon`/`@tui.monitor`, notifications `@tui.bell`, new `@tui.plus`, logout `@tui.log-out`, alert `@tui.triangle-alert`, dismiss `@tui.x`.
- **Rationale**: Zero new dependencies; Taiga's icon pipeline is already configured and tree-shaken per icon; Lucide's style matches the Helix reference's clean line-icon look.
- **Alternatives considered**: Inline custom SVGs (more maintenance, no visual gain); adding `lucide-angular` (duplicates what Taiga icons already provide).

## R5. Route titles & subtitles

- **Decision**: Define a typed `PAGE_TITLES` map in `core/router/page-title.ts` keyed by route path constant, with `{ title, subtitle }` per the spec mapping. Each tenant route carries `data: { pageTitle: … }`; the topbar reads the deepest activated route's data via a small `toSignal`-based helper on `Router` events (or `ActivatedRoute` traversal) and renders title/subtitle. Also set the browser tab title through Angular's built-in `TitleStrategy` (`title:` route property) for accessibility. The `/platform` placeholder route gets a generic `{ title: 'Platform', subtitle: 'Platform administration' }` per clarification Q4. The Overview subtitle prepends the formatted current date ("Tuesday, June 20 · Your support cockpit" pattern) computed at render time.
- **Rationale**: Route `data` keeps titles co-located with route definitions and testable without component coupling; the central map keeps strings out of feature code per the APP_PATHS discipline.
- **Alternatives considered**: NgRx `appUi` slice for title metadata (spec allows it, but router data is derived state — storing it would duplicate the router as source of truth); per-page `<app-page-header>` inputs only (loses topbar integration required by FR-011/FR-012).

## R6. Theme toggle cycling (from Clarification Q1)

- **Decision**: Topbar toggle dispatches `themeModeChanged` with the next mode in the fixed cycle light → dark → system → light, starting from current `themeMode`. Icon per mode: `@tui.sun` (light), `@tui.moon` (dark), `@tui.monitor` (system); `aria-label` announces current mode and next action. Existing `selectResolvedTheme` + `data-theme` effect and localStorage persistence (spec 002) are reused untouched. The Settings → General theme preference renders the same three modes as an explicit segmented control dispatching the same action.
- **Rationale**: Matches clarification; single action keeps one source of truth; no new state shape.
- **Alternatives considered**: Resolved in clarification session (flip-only and menu variants rejected by user).

## R7. Taiga UI wrapping strategy

- **Decision**: Taiga components are used only inside `shared/components/*`, `layout/*`, and feature form controls — never styled ad hoc in page templates. Concrete usage: `TuiButton` (primary/New/auth buttons via project button styles), `TuiIcon`, `TuiTextfield`+`TuiInput*` (search input, auth forms, settings fields), `TuiSelect` (tone/language/date-range/category selects), `TuiTabs` (AI Agent + Settings tabs), `TuiSwitch` (2FA, notification toggles), `TuiBadge` only if it accepts Helix tokens cleanly — otherwise the project `status-badge` renders its own markup (spec: "do not fight Taiga for visual parity"). Helix look is applied through component-scoped CSS consuming `--app-*` tokens; where Taiga's own CSS custom properties exist (e.g., `--tui-background-*`), they are bridged once in `themes.css`, not per component.
- **Rationale**: Satisfies FR-016 (wrap, don't scatter) and keeps a single place to adjust Taiga/Helix bridging.
- **Alternatives considered**: Building all controls from scratch (wasteful; Taiga is mandated by CLAUDE.md); using Taiga appearances/themes API alone for full parity (insufficient control over exact Helix values).

## R8. Charts & sparklines without a library

- **Decision**: Two tiny presentational components: `sparkline` (inline SVG polyline, viewBox-normalized from a `number[]` input, stroke = token color) and per-page chart blocks built from the same primitives — Overview trend card renders three `<path>` series with an HTML legend; channel breakdown renders an SVG donut via stroke-dasharray circle segments; Analytics reuses sparkline/area/bar variants. All geometry computed in pure functions from fixture arrays; sized via viewBox + CSS (responsive), `aria-hidden` with adjacent text alternatives.
- **Rationale**: FR-019/FR-025 mandate no chart library; static fixture data needs no scales/interactivity; pure-function geometry is unit-testable.
- **Alternatives considered**: `ngx-charts`/`chart.js` (prohibited); CSS-only bars (insufficient for line/donut visuals).

## R9. Feature-local state (SignalStores)

- **Decision**: Three component-provided SignalStores: `ConversationsStore` (state: `selectedId`, `statusFilter`; computed: `filteredConversations`, `selectedConversation`, `selectedCustomer` over fixture input; default selection = first conversation), `AiAgentStore` (`activeTab: 'behavior' | 'prompt' | 'escalation' | 'testing'`), `SettingsStore` (`activeTab: 'general' | 'team' | 'billing' | 'api-keys' | 'security'`). Analytics date-range/channel filter and knowledge-base filters use plain component `signal()`s (component-temporary per CLAUDE.md state rules) unless they grow cross-component consumers within the page — knowledge-base filter feeds the empty-state computed, so it may justify a store at implementation time; default is `signal()`.
- **Rationale**: Meets FR-018 and acceptance criterion 16 (SignalStore for at least conversations + tabs) while honoring the three-tier state rule (global NgRx / SignalStore / signal) without over-formalizing trivial page state.
- **Alternatives considered**: Global NgRx for page state (violates FR-018); one shared "pageUi" store (couples unrelated features).

## R10. Fixture architecture

- **Decision**: `shared/fixtures/fixture.models.ts` declares the readonly types from data-model.md; six sibling `*.fixtures.ts` files export `const` arrays/objects (e.g., `CONVERSATION_FIXTURES: readonly ConversationFixture[]`). Pages import fixtures directly as initial SignalStore/`signal()` values — no fake HTTP services, no artificial delays, no injection tokens pretending to be APIs. Field names align with spec-001 REST contract vocabulary (`id`, `tenantId` omitted as visual-only, `createdAt` ISO strings) so future wiring replaces the import with a real service call per page.
- **Rationale**: FR-029 requires clear separation and easy replacement; a direct typed import is the most honest "visual-only" boundary — inventing mock services would simulate infrastructure this spec explicitly excludes.
- **Alternatives considered**: Mock services implementing future API interfaces (speculative contracts before the backend endpoints exist invite rework); JSON files + HTTP loading (adds runtime fetch for no benefit and breaks "no network activity" SC-007).

## R11. Shell scroll model

- **Decision**: Shell becomes a fixed-viewport grid: `.shell { height: 100dvh; display: grid; grid-template-columns: auto 1fr; overflow: hidden; }`; sidebar owns `overflow-y: auto`; the main area is a column flex with fixed topbar and `main { overflow-y: auto; }`; `body { overflow: hidden; }`. Conversations page uses an internal three-column grid (`minmax` list / flexible thread / fixed customer panel) with each pane scrolling independently; below the `--app-bp-lg` breakpoint the customer panel hides behind a toggle and below `--app-bp-md` list/thread stack.
- **Rationale**: Spec shell rules (no body scroll, independent overflow); grid + dvh is the simplest robust approach and plays well with the width transition.
- **Alternatives considered**: Current min-height flow layout (allows body scroll — violates spec); position:fixed sidebar (complicates width transition and stacking).

## R12. Platform area re-hosting (from Clarification Q4)

- **Decision**: `/platform` keeps `platform-overview-placeholder.component` and its route file; it already renders inside `AppShellComponent` in `app.routes.ts`, so it inherits the new shell automatically. Only additions: generic page-title data (R5) and removing the old placeholder path constant only for tenant/auth (platform's stays). The platform area gets no sidebar nav entry (sidebar groups are tenant-scoped per spec); it remains reachable by URL.
- **Rationale**: Clarification chose "wrap in new shell"; zero-cost since the route structure already nests it under the shell.
- **Alternatives considered**: Resolved in clarification (untouched / removal rejected by user).

## R13. Legacy component disposition

- **Decision**: Delete `tenant/overview-placeholder` and `auth/login-placeholder` (superseded by real pages). Absorb `shared/components/loading-indicator` into the new `loading-state` component (keep selector-compatible export or update the few usages). Keep `not-found` but restyle it with Helix tokens (it references `--app-color-*` names that disappear in R2's rename sweep). `area-access.guard` stays as-is (pass-through).
- **Rationale**: R2's hard token rename forces touching every consumer anyway; leaving dead placeholders violates FR-013.
- **Alternatives considered**: Keeping placeholders alongside real pages (dead routes, confusing).
