# Quickstart: Helix Admin Dashboard Visual System

**Feature**: 003-helix-dashboard-visuals | **Date**: 2026-07-06

Validation guide proving the feature works end-to-end. Contracts: [contracts/ui-contract.md](./contracts/ui-contract.md) · Fixture shapes: [data-model.md](./data-model.md).

## Prerequisites

- Node 20+, pnpm 10 (`corepack enable`)
- Repo cloned; reference file `Helix Admin.html` present at repo root (visual comparison only — never imported by the app)

## Setup

```bash
cd frontend
pnpm install         # brings in @fontsource/geist-sans / geist-mono
```

## Run

```bash
pnpm start           # ng serve dashboard → http://localhost:4200
```

## Validation scenarios

### 1. Shell & theming (Spec US1 / FR-001…FR-012)

1. Open `http://localhost:4200/` → redirected to `/tenant/overview` inside the Helix shell.
2. Verify with devtools: sidebar `248px` wide (collapse → `68px`, animated), topbar `60px`, content max-width `1320px`, **no body scrollbar** — only the main area scrolls.
3. Sidebar shows: gradient logo square + "Helix / Support AI", groups Workspace / AI / Insights / Settings, red count badge on Conversations, footer user card with logout icon.
4. Click the theme toggle repeatedly → cycles light → dark → system (icon: sun → moon → monitor); all surfaces switch palette. Reload → mode restored (localStorage `app.themeMode`).
5. Collapse the sidebar → icon-only items; hover/focus an item and confirm an accessible label (inspect `aria-label`); active item has accent-soft background and `aria-current="page"`.
6. Navigate through all 8 tenant routes → topbar title/subtitle match the mapping in the UI contract; Overview subtitle starts with today's formatted date.
7. Search box, bell, and "New" render with hover/focus states but do nothing when activated.
8. Compare side-by-side with `Helix Admin.html` opened in a browser (light and dark): background, panel, accent, border colors visually match (SC-001).

### 2. Overview page (US2 / FR-019)

- Amber alert banner (icon, title, description) renders; dismiss removes it; it stays gone while navigating away and back (same session), returns after full reload.
- Five metric cards with icon badge, value, delta badge, sparkline. Trend chart card shows 3 SVG series + legend; channel breakdown shows Website/WhatsApp/Telegram/Mobile SDK; activity panel lists recent conversations with badges.
- Network tab: **zero** XHR/fetch requests (SC-007).

### 3. Conversations page (US3 / FR-020)

- Three panes: inbox list / thread / customer sidebar. List shows all channel, status, and sentiment badge variants.
- Click another conversation → thread + customer panel update instantly (no reload).
- Status filter → list narrows; filtering out the selection moves it to the first visible item.
- Thread contains customer/AI/human bubbles, a system event row, AI suggestion card, reply composer, takeover control.
- Narrow the window below the lg breakpoint → customer panel hides/stacks; below md → list/thread stack; nothing overlaps (FR-030).

### 4. Remaining pages (US4 / FR-021…FR-026)

- `/tenant/ai-agent`: Behavior/Prompt/Escalation/Testing tabs switch content (SignalStore); prompt editor shell renders in mono font; allowed/blocked topics, escalation triggers, agent preview all present.
- `/tenant/customers`: toolbar + panel table with avatar, tier, CSAT, spend columns and compact rows.
- `/tenant/knowledge-base`: cards with status + source badges and re-index indicator; type a search matching nothing → shared empty state appears.
- `/tenant/integrations`: 8 cards, each with status badge and action button, in a wrapping grid.
- `/tenant/analytics`: date-range + channel filters, metric cards, 4 SVG charts, top-articles table.
- `/tenant/settings`: 5 tabs; General shows theme preference control (dispatches the same appUi action as the topbar toggle); toggles and usage bars respond visually; API key shows masked value only.
- `/platform/overview-placeholder`: old placeholder content, but rendered inside the Helix shell with "Platform" topbar title.

### 5. Auth screens (US5 / FR-027)

- `/auth/login`, `/auth/signup`, `/auth/forgot-password`, `/auth/verify-email`: centered card, logo + product name, labeled fields, accent primary button, secondary links; verify-email shows OTP boxes. Toggle dark mode → all four remain correct. Submitting does nothing (no requests, no navigation side effects beyond visual links).

### 6. Accessibility sweep (FR-031, SC-005)

- Keyboard-only: Tab reaches sidebar items (collapsed included), topbar controls, list items, tabs, toggles; focus ring visible in both themes.
- Landmarks present: exactly one `aside`, `header`, `main` on dashboard routes.

## Automated gates (all must pass)

```bash
cd frontend
pnpm ng build dashboard    # compiles; lazy chunk per page
pnpm ng test dashboard     # vitest: shell/state/page behavior suites incl. new visual-behavior tests
pnpm lint
pnpm format:check
```

Expected new/updated test coverage (behavior, not snapshots): sidebar renders groups + collapse state; topbar title per route + theme cycle dispatches `themeModeChanged` with the next mode; overview renders 5 metric cards + alert dismiss; ConversationsStore selection/filter logic; AiAgent/Settings tab stores; auth login renders; fixture referential integrity (conversation → customer); channel breakdown sums to 100.

## Validation notes

- 2026-07-07 implementation pass: source-level validation succeeded with `tsc -p apps/dashboard/tsconfig.app.json --noEmit`, `tsc -p apps/dashboard/tsconfig.spec.json --noEmit`, direct local ESLint, and direct local Prettier check.
- Exact pnpm gates were attempted from `frontend/`, but `pnpm ng build dashboard`, `pnpm ng test dashboard --watch=false`, `pnpm lint`, and `pnpm format:check` all failed before script execution with `fetch failed` under restricted network. Direct Angular CLI build/test entrypoints reached Angular, then failed at esbuild startup with `spawn EPERM`.
