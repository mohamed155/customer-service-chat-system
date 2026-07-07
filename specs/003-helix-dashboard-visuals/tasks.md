# Tasks: Helix Admin Dashboard Visual System

**Input**: Design documents from `/specs/003-helix-dashboard-visuals/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ui-contract.md, quickstart.md

**Tests**: Included — the spec explicitly requires behavior tests (SC-006, FR-031/FR-032) and constitution Principle VII mandates test coverage for shipped functionality.

**Organization**: Tasks grouped by user story. Path prefix `app/` = `frontend/apps/dashboard/src/app/`. All components: standalone, OnPush, token-consuming (`--app-*`), no business logic. Contracts in [contracts/ui-contract.md](./contracts/ui-contract.md); fixture shapes in [data-model.md](./data-model.md); decisions R1–R13 in [research.md](./research.md).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 (shell/tokens/themes), US2 (Overview), US3 (Conversations), US4 (workspace pages), US5 (Auth)

---

## Phase 1: Setup

**Purpose**: New dependencies and font wiring (research.md R3)

- [x] T001 Add `@fontsource/geist-sans` and `@fontsource/geist-mono` dependencies in `frontend/package.json` (run `pnpm add @fontsource/geist-sans @fontsource/geist-mono` in `frontend/`)
- [x] T002 Import Geist weights (sans 400/500/600/700, mono 400/500) via `@import` and keep global resets in `frontend/apps/dashboard/src/styles.css`; add `body { overflow: hidden; }` per shell scroll model (R11)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Token layer, routes/titles, fixtures, and reusable UI components that every story consumes (constitution IX: tokens → components → pages)

**⚠️ CRITICAL**: No user story phase can begin until this phase is complete

### Tokens & theming (contract §2)

- [x] T003 [P] Rewrite theme-independent tokens (layout 248/68/60/1320px, page padding, radius xs–xl, spacing, font family/mono + 11–24px scale, transitions, z-index, breakpoints) in `app/design-system/tokens/tokens.css`
- [x] T004 [P] Rewrite light/dark palettes (`--app-bg/panel/panel-2/panel-3/sidebar/border/border-strong/text/text-2/text-3/accent(+strong,ink,soft)/green,amber,red(+soft)/shadow(+lg)`, `color-scheme`, per-theme shadows, one-time Taiga custom-property bridge) in `app/design-system/theme/themes.css` — values exactly per spec/contract (verified against `Helix Admin.html`, R1)
- [x] T005 Hard-rename sweep: replace all removed token names (`--app-color-*`, `--app-sidebar-width`, `--app-page-max-width`, `--app-text-md`, `--app-font-family` value, …) in every existing consumer — `app/layout/app-shell/app-shell.component.ts`, `app/layout/sidebar/sidebar.component.ts`, `app/layout/topbar/topbar.component.ts`, `app/layout/page-header/page-header.component.ts`, `app/features/not-found/not-found.component.ts`, `app/shared/components/loading-indicator/loading-indicator.component.ts`, `frontend/apps/dashboard/src/styles.css`; build must compile with zero references to old names (depends on T003, T004)

### Routes, titles, fixtures

- [x] T006 Update `APP_PATHS` in `app/core/router/app-paths.ts`: add tenant `overview/conversations/customers/ai-agent/knowledge-base/integrations/analytics/settings` and auth `login/signup/forgot-password/verify-email`; remove `tenant.overviewPlaceholder` and `auth.loginPlaceholder`; keep `platform.overviewPlaceholder`
- [x] T007 Create typed `PAGE_TITLES` map (contract §1 incl. platform generic title, Overview date-prefixed subtitle helper) and route-data reader utility in `app/core/router/page-title.ts` (depends on T006)
- [x] T008 [P] Create all fixture types from data-model.md (unions + ConversationFixture, MessageFixture, CustomerFixture, MetricFixture, TrendSeriesFixture, ChannelBreakdownFixture, TopArticleFixture, KnowledgeArticleFixture, IntegrationFixture, settings fixtures, SidebarUserFixture, AlertFixture) in `app/shared/fixtures/fixture.models.ts`
- [x] T009 [P] Create conversation fixtures (≥9, all channel×status combos, all three sentiments, ≥3 messages each incl. AI + system events, aiConfidence/citations on AI messages) in `app/shared/fixtures/conversation.fixtures.ts` (depends on T008)
- [x] T010 [P] Create customer fixtures (every conversation customerId resolvable, recentActivity entries) in `app/shared/fixtures/customer.fixtures.ts` (depends on T008)
- [x] T011 [P] Create analytics fixtures (5 overview metrics, trend series, channel breakdown summing to 100, analytics metrics/charts, top articles) in `app/shared/fixtures/analytics.fixtures.ts` (depends on T008)
- [x] T012 [P] Create knowledge fixtures (all statuses + source types, categories, indexed flags) in `app/shared/fixtures/knowledge.fixtures.ts` (depends on T008)
- [x] T013 [P] Create the 8 integration fixtures (Website Widget, WhatsApp, Telegram, Shopify, HubSpot, Zendesk, Slack, Stripe; mixed statuses) in `app/shared/fixtures/integration.fixtures.ts` (depends on T008)
- [x] T014 [P] Create settings + sidebar-user + overview-alert fixtures (profile, 5-role team, usage, invoices, masked API key, sessions) in `app/shared/fixtures/settings.fixtures.ts` (depends on T008)

### Route scaffolding (pages as minimal stubs so all nav works from US1 onward)

- [x] T015 Create `PageContainerComponent` (max-width 1320px, page padding, centered) in `app/layout/page-container/page-container.component.ts` (depends on T003)
- [x] T016 Create 8 stub tenant page components (empty `<app-page-container>` shells) under `app/features/tenant/{overview,conversations,customers,ai-agent,knowledge-base,integrations,analytics,settings}/` and rewrite `app/features/tenant/tenant.routes.ts` (lazy `loadComponent`, `data.pageTitle` + `title` from PAGE_TITLES, default redirect → overview); delete `app/features/tenant/overview-placeholder/` (depends on T006, T007, T015)
- [x] T017 Create 4 stub auth page components under `app/features/auth/{login,signup,forgot-password,verify-email}/` and rewrite `app/features/auth/auth.routes.ts` (default redirect → login); delete `app/features/auth/login-placeholder/` (depends on T006, T007)
- [x] T018 Update root routes: `/` → `/tenant/overview`, platform route gets `data.pageTitle` (generic "Platform"), in `app/app.routes.ts`; update route tests in `app/app.routes.spec.ts` (depends on T016, T017)

### Reusable UI components (contract §4 — no business logic, fixtures-typed inputs)

- [x] T019 [P] Create `SparklineComponent` (inline SVG polyline from `points`, viewBox-normalized, token stroke, `aria-hidden`) in `app/shared/components/sparkline/sparkline.component.ts`
- [x] T020 [P] Create `DashboardCardComponent` (panel bg, border, radius-lg, shadow, header/body/footer slots) in `app/shared/components/dashboard-card/dashboard-card.component.ts`
- [x] T021 [P] Create `StatusBadgeComponent`, `ChannelBadgeComponent`, `SentimentBadgeComponent` (soft-color pill styles per tone) in `app/shared/components/status-badge/`, `app/shared/components/channel-badge/`, `app/shared/components/sentiment-badge/`
- [x] T022 [P] Create `AvatarComponent` (initials, sm/md/lg) in `app/shared/components/avatar/avatar.component.ts`
- [x] T023 [P] Create `SearchInputComponent` (Taiga textfield wrapped, ⌘K hint slot, model signal) and `IconButtonComponent` (38×38, bordered, required `label` → aria-label) in `app/shared/components/search-input/` and `app/shared/components/icon-button/`
- [x] T024 [P] Create `EmptyStateComponent` and `LoadingStateComponent` (absorb `loading-indicator`: migrate its usages, delete `app/shared/components/loading-indicator/`) in `app/shared/components/empty-state/` and `app/shared/components/loading-state/`
- [x] T025 [P] Create `SectionHeaderComponent` and `ToolbarComponent` (start/end slots) in `app/shared/components/section-header/` and `app/shared/components/toolbar/`
- [x] T026 [P] Create `DataTableComponent` (Helix panel table wrapper: rounded container, compact rows, muted metadata) in `app/shared/components/data-table/data-table.component.ts`
- [x] T027 Create `MetricCardComponent` (icon badge, label, value, delta badge with direction/positivity coloring, sparkline) in `app/shared/components/metric-card/metric-card.component.ts` (depends on T019, T020)

**Checkpoint**: Tokens live, all 12 routes resolve to stubs, fixtures typed, component kit ready — user stories can start (in parallel if staffed)

---

## Phase 3: User Story 1 — Helix Visual Shell: Tokens, Sidebar, Topbar, Themes (Priority: P1) 🎯 MVP

**Goal**: The running app presents the Helix shell — 248/68px grouped sidebar with brand/badge/footer, 60px topbar with route titles + cycling theme toggle, correct light/dark palettes, no body scroll.

**Independent Test**: quickstart.md scenario 1 — visit `/`, verify dimensions, collapse, theme cycle with persistence, nav groups + active state + `aria-current`, title mapping on all routes, visual match vs `Helix Admin.html`.

### Implementation

- [x] T028 [US1] Rewrite shell to fixed-viewport grid (`height: 100dvh`, `auto 1fr` columns, sidebar/main own overflow, fixed topbar row, content in PageContainer) in `app/layout/app-shell/app-shell.component.ts` (R11)
- [x] T029 [P] [US1] Create `SidebarNavGroupComponent` (uppercase 10px/0.07em label, hides when collapsed) and `SidebarNavItemComponent` (36px height, 9px radius, icon + label, accent-soft active via `routerLinkActive`, `aria-current="page"`, optional red count badge, `aria-label` when collapsed) in `app/layout/sidebar/sidebar-nav-group.component.ts` and `app/layout/sidebar/sidebar-nav-item.component.ts`
- [x] T030 [US1] Rewrite sidebar as `<aside>`: brand block (30px gradient logo square, "Helix / Support AI"), Workspace/AI/Insights/Settings groups from APP_PATHS, Conversations badge, footer user card from SidebarUserFixture with logout icon-button, width transition `--app-transition-base`, collapsed icon-only mode in `app/layout/sidebar/sidebar.component.ts` (depends on T029)
- [x] T031 [US1] Rewrite topbar as `<header>`: route-driven title (16px/600) + subtitle (11.5px muted) via page-title reader, sidebar toggle, 260px SearchInput with ⌘K hint (visual-only), theme-cycle IconButton (light→dark→system, sun/moon/monitor icon, mode-announcing aria-label), notifications IconButton (visual-only), primary accent "New" button (visual-only, 38px) in `app/layout/topbar/topbar.component.ts` (depends on T007, T023; R5, R6)
- [x] T032 [P] [US1] Restyle not-found page with Helix tokens and link back to `/tenant/overview` in `app/features/not-found/not-found.component.ts`

### Tests

- [x] T033 [P] [US1] Sidebar behavior tests (renders 4 groups + 8 items, collapse hides labels + adds aria-labels, Conversations badge shown, active item marked) in `app/layout/sidebar/sidebar.component.spec.ts`
- [x] T034 [P] [US1] Topbar behavior tests (title/subtitle per route data; theme button dispatches `themeModeChanged` with next mode for each of light/dark/system; New/search/bell have no handlers) in `app/layout/topbar/topbar.component.spec.ts`
- [x] T035 [US1] Update shell tests (landmarks `aside`/`header`/`main`, grid/scroll classes, collapsed input flows to sidebar) in `app/layout/app-shell/app-shell.component.spec.ts`

**Checkpoint**: US1 fully functional — MVP demoable with stub pages

---

## Phase 4: User Story 2 — Overview Cockpit Page (Priority: P2)

**Goal**: Overview renders alert banner, 5 metric cards, trend chart, channel breakdown, and activity preview from fixtures in both themes.

**Independent Test**: quickstart.md scenario 2 — all five sections render; alert dismisses (session-only); zero network requests; tablet wrap intact.

### Implementation

- [x] T036 [P] [US2] Create `EscalationBannerComponent` (amber alert: icon, title, description, dismiss button emitting output) in `app/shared/components/ai/escalation-banner/escalation-banner.component.ts`
- [x] T037 [P] [US2] Create trend chart block (3 SVG path series from TrendSeriesFixture + HTML legend, pure geometry functions) and channel donut block (stroke-dasharray segments + labeled list, Website/WhatsApp/Telegram/Mobile SDK) in `app/features/tenant/overview/overview-trend-chart.component.ts` and `app/features/tenant/overview/overview-channel-breakdown.component.ts` (R8)
- [x] T038 [US2] Compose Overview page: dismissible alert (in-memory `signal()`), 5-metric-card grid, trend chart card, breakdown card, activity/inbox preview card (recent conversations from fixtures: avatar, name, channel/status badges, snippet, time) in `app/features/tenant/overview/overview.component.ts` (depends on T036, T037; fixtures T009–T011)

### Tests

- [x] T039 [US2] Overview behavior tests (exactly 5 metric cards render from fixtures; alert visible initially, removed on dismiss; trend/breakdown/activity sections present) in `app/features/tenant/overview/overview.component.spec.ts`

**Checkpoint**: US1 + US2 work — landing page is the Helix cockpit

---

## Phase 5: User Story 3 — Conversations Inbox Experience (Priority: P3)

**Goal**: Three-pane inbox over fixtures with SignalStore-driven selection/filtering, full badge variants, thread with AI visuals, customer panel.

**Independent Test**: quickstart.md scenario 3 — select conversations, thread + customer panel update; filter narrows list; responsive stacking below lg/md breakpoints.

### Implementation

- [x] T040 [P] [US3] Create `AiConfidenceBadgeComponent` and `KnowledgeCitationComponent` in `app/shared/components/ai/ai-confidence-badge/` and `app/shared/components/ai/knowledge-citation/`
- [x] T041 [P] [US3] Create `AiSuggestionCardComponent` (suggestion text + projected actions) and `AiThinkingIndicatorComponent` in `app/shared/components/ai/ai-suggestion-card/` and `app/shared/components/ai/ai-thinking-indicator/`
- [x] T042 [P] [US3] Create `ConversationsStore` SignalStore (`selectedId`, `statusFilter`; computed `filteredConversations`/`selectedConversation`/`selectedCustomer`; default first conversation; selection follows filter per data-model) in `app/features/tenant/conversations/conversations.store.ts` (R9)
- [x] T043 [P] [US3] Create inbox list component (avatar, name, channel/status/sentiment badges, snippet, relative time, unread dot, selected state, filter toolbar) in `app/features/tenant/conversations/inbox-list.component.ts`
- [x] T044 [P] [US3] Create thread component (customer/AI/human bubbles, AI confidence + citations, system event rows, reply composer, AiSuggestionCard, takeover/hand-back button — all visual) in `app/features/tenant/conversations/conversation-thread.component.ts` (depends on T040, T041)
- [x] T045 [P] [US3] Create customer panel component (profile, email, tier, orders, since, sentiment, recent activity) in `app/features/tenant/conversations/customer-panel.component.ts`
- [x] T046 [US3] Compose Conversations page: three-pane grid bound to ConversationsStore, customer panel hides below `--app-bp-lg`, list/thread stack below `--app-bp-md` in `app/features/tenant/conversations/conversations.component.ts` (depends on T042–T045)

### Tests

- [x] T047 [P] [US3] ConversationsStore unit tests (default selection, select(), setFilter() narrows + moves hidden selection) in `app/features/tenant/conversations/conversations.store.spec.ts`
- [x] T048 [US3] Conversations page behavior tests (clicking list item updates thread + customer panel; filter updates rendered list) in `app/features/tenant/conversations/conversations.component.spec.ts`

**Checkpoint**: Core product screen complete and independently testable

---

## Phase 6: User Story 4 — Workspace Pages: AI Agent, Customers, Knowledge Base, Integrations, Analytics, Settings (Priority: P4)

**Goal**: Remaining six tenant pages visually complete from fixtures, tabs on SignalStores.

**Independent Test**: quickstart.md scenario 4 — every required section renders per page in both themes; AI Agent/Settings tabs switch content; knowledge-base empty state on zero matches.

### Implementation

- [x] T049 [P] [US4] Create `PromptEditorShellComponent` (mono font, line-gutter visual, model signal), `AgentPreviewCardComponent` (fixture transcript), `AiToolTimelineComponent` (steps list) in `app/shared/components/ai/prompt-editor-shell/`, `app/shared/components/ai/agent-preview-card/`, `app/shared/components/ai/ai-tool-timeline/`
- [x] T050 [US4] Create `AiAgentStore` (activeTab) + AI Agent page: Behavior (profile card, name, tone/language/response-length Taiga selects), Prompt (editor shell), Escalation (triggers, allowed/blocked topic chip lists), Testing (agent preview + tool timeline) tabs via TuiTabs in `app/features/tenant/ai-agent/ai-agent.store.ts` and `app/features/tenant/ai-agent/ai-agent.component.ts` (depends on T049)
- [x] T051 [P] [US4] Create Customers page: Toolbar (search `signal()` + filters) + DataTable (avatar, name, email, channel badge, tier badge, last interaction, interactions, CSAT, spend/orders) in `app/features/tenant/customers/customers.component.ts`
- [x] T052 [P] [US4] Create Knowledge Base page: "New article" header action (visual), search + category filter (`signal()`s), article cards (status/source badges, updated, re-index indicator), EmptyState on zero matches in `app/features/tenant/knowledge-base/knowledge-base.component.ts`
- [x] T053 [P] [US4] Create Integrations page: responsive grid of 8 cards (icon, name, description, status badge, action button per status) in `app/features/tenant/integrations/integrations.component.ts`
- [x] T054 [P] [US4] Create Analytics page: date-range + channel selects (`signal()`s), metric cards, 4 SVG chart cards (volume/AI resolution/CSAT/handoff — reuse sparkline/chart primitives), top-articles DataTable in `app/features/tenant/analytics/analytics.component.ts`
- [x] T055 [US4] Create `SettingsStore` (activeTab) + Settings page: General (workspace profile, theme preference segmented control dispatching `themeModeChanged`, notification toggles), Team (member list with role badges), Billing (usage bars, invoices), API Keys (masked field in mono), Security (2FA TuiSwitch, sessions list) in `app/features/tenant/settings/settings.store.ts` and `app/features/tenant/settings/settings.component.ts`

### Tests

- [x] T056 [P] [US4] Tab store tests (AiAgentStore + SettingsStore initial tab + setTab) in `app/features/tenant/ai-agent/ai-agent.store.spec.ts` and `app/features/tenant/settings/settings.store.spec.ts`
- [x] T057 [US4] Page behavior tests: AI Agent tab switch changes visible section; knowledge-base unmatched search shows EmptyState; Settings theme control dispatches action in `app/features/tenant/ai-agent/ai-agent.component.spec.ts`, `app/features/tenant/knowledge-base/knowledge-base.component.spec.ts`, `app/features/tenant/settings/settings.component.spec.ts`

**Checkpoint**: Full dashboard surface complete

---

## Phase 7: User Story 5 — Auth Screens (Priority: P5)

**Goal**: Login, signup, forgot-password, verify-email as centered Helix-quality cards, both themes, zero auth logic.

**Independent Test**: quickstart.md scenario 5 — all four routes render branded centered cards; OTP boxes on verify-email; dark mode correct; submit does nothing.

### Implementation

- [x] T058 [US5] Create shared `AuthCardComponent` (soft-bg centered layout, gradient logo, product name, projected form slot, footer links slot) in `app/features/auth/auth-card/auth-card.component.ts`
- [x] T059 [P] [US5] Build Login (email/password Taiga fields, accent submit, "forgot password"/"sign up" links) and Signup (name/email/password, terms line) pages in `app/features/auth/login/login.component.ts` and `app/features/auth/signup/signup.component.ts` (depends on T058)
- [x] T060 [P] [US5] Build Forgot-password (email + submit + back link) and Verify-email (6 OTP boxes with per-box focus `signal()` handling, resend link) pages in `app/features/auth/forgot-password/forgot-password.component.ts` and `app/features/auth/verify-email/verify-email.component.ts` (depends on T058)

### Tests

- [x] T061 [US5] Auth behavior tests (login renders card, labeled fields, no submit handler side effects; verify-email renders 6 OTP inputs) in `app/features/auth/login/login.component.spec.ts` and `app/features/auth/verify-email/verify-email.component.spec.ts`

**Checkpoint**: All five stories independently functional

---

## Phase 8: Polish & Cross-Cutting Concerns

- [x] T062 [P] Fixture integrity tests (every conversation customerId resolves; channel breakdown sums to 100; all channel/status/sentiment/article-status/source variants present) in `app/shared/fixtures/fixtures.spec.ts`
- [x] T063 [P] Responsive audit at 768/1024/1280px: card grids wrap, topbar search shrinks/hides below md, conversations panes stack, auth centered — fix in affected component styles (FR-030)
- [x] T064 [P] Accessibility audit: one `aside`/`header`/`main` per dashboard route, all actions real buttons/links, visible focus in both themes, input labels, collapsed-nav aria-labels (FR-031, SC-005)
- [x] T065 Side-by-side visual comparison against `Helix Admin.html` (light + dark, all pages); refine token/spacing deviations found (SC-001; spec allows minor value refinements)
- [x] T066 Run quality gates in `frontend/`: `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` — fix all failures (SC-006)
- [x] T067 Execute full quickstart.md validation (scenarios 1–6, incl. zero-network check per SC-007) and record results in `specs/003-helix-dashboard-visuals/quickstart.md` notes or PR description

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: none — start immediately
- **Phase 2 (Foundational)**: T002→(T003,T004)→T005; T006→T007→(T016,T017)→T018; T008→T009–T014; T015 before T016; T019/T020 before T027. Blocks all stories.
- **Phases 3–7 (US1–US5)**: each depends only on Phase 2 — stories are mutually independent and can run in parallel
- **Phase 8 (Polish)**: after all desired stories

### Within stories

- US1: T029 → T030; T028/T031/T032 independent of each other; tests T033–T035 after their components
- US2: T036, T037 → T038 → T039
- US3: T040–T045 parallel → T046 → T048; T047 after T042
- US4: T049 → T050; T051–T054 parallel; T055 independent; tests last
- US5: T058 → T059, T060 → T061

### Parallel opportunities

- Phase 2: T003+T004 together; T008 then T009–T014 (6 files) together; T019–T026 (8 component tasks) together
- After Phase 2, all five stories can proceed in parallel (different feature folders)
- US4 offers the widest fan-out: T051, T052, T053, T054 are four independent pages

## Parallel Example: Foundational component kit

```text
# After T003–T005, launch together:
Task T019: SparklineComponent in app/shared/components/sparkline/
Task T020: DashboardCardComponent in app/shared/components/dashboard-card/
Task T021: badge components in app/shared/components/{status,channel,sentiment}-badge/
Task T022: AvatarComponent in app/shared/components/avatar/
Task T023: SearchInput + IconButton in app/shared/components/{search-input,icon-button}/
Task T024: EmptyState + LoadingState in app/shared/components/{empty-state,loading-state}/
Task T025: SectionHeader + Toolbar in app/shared/components/{section-header,toolbar}/
Task T026: DataTableComponent in app/shared/components/data-table/
```

## Implementation Strategy

**MVP first (US1 only)**: Phases 1–2, then Phase 3. Stop and validate: the app boots into the Helix shell with working nav, collapse, theme cycle, and correct titles over stub pages — demoable proof of the visual system.

**Incremental delivery**: +US2 (landing cockpit) → +US3 (core inbox) → +US4 (full surface) → +US5 (auth) — each checkpoint independently testable per its quickstart scenario, then Phase 8 polish and gates.

**Suggested single-developer order**: straight T001→T067; commit per task or logical group; run `pnpm ng test dashboard` at every checkpoint.
