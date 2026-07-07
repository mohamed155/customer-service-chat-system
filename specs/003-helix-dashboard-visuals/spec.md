# Feature Specification: Helix Admin Dashboard Visual System

**Feature Branch**: `003-helix-dashboard-visuals`

**Created**: 2026-07-06

**Status**: Draft

**Input**: User description: "Frontend Spec 02 — Helix Admin Dashboard Visual System. Make the Angular dashboard application visually match the provided Helix Admin template (`Helix Admin.html` at the repository root). Transform the current frontend foundation into a polished enterprise SaaS dashboard following the same visual language, spacing, layout, typography, colors, navigation style, page structure, and interaction patterns as the Helix Admin design — design tokens, app shell, sidebar, topbar, light/dark themes, reusable components, and static visual implementations of the Overview, Conversations, AI Agent, Customers, Knowledge Base, Integrations, Analytics, Settings, and Auth screens. No real backend business logic; mock fixture data only, clearly separated from real services. The product should feel like an Enterprise Operating System for AI Customer Service (Intercom / Linear / Stripe Dashboard quality), not a simple chatbot admin panel."

## Clarifications

### Session 2026-07-06

- Q: How should the topbar theme toggle button behave, given themeMode supports light / dark / system? → A: Each click cycles light → dark → system; the button's icon indicates the current mode.
- Q: How should the dashboard obtain its display font (reference design uses Geist)? → A: Self-host the openly licensed Geist font as a project dependency, with system-font fallback; no runtime font CDN and nothing copied from the reference HTML.
- Q: What should the topbar search box (⌘K), notifications button, and "New" button do when activated in this spec? → A: Purely visual — they render with hover/focus states but perform no action; behavior arrives in later specs.
- Q: What should happen to the /platform area (platform-admin placeholder from spec 002) in this spec? → A: Keep its placeholder content but render the /platform route inside the new Helix shell (with a generic topbar title) so it doesn't look broken if visited; full platform-dashboard visuals remain a future spec.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Helix Visual Shell: Tokens, Sidebar, Topbar, Themes (Priority: P1)

A stakeholder opens the dashboard and sees a polished enterprise SaaS shell that matches the Helix Admin reference: a compact collapsible left sidebar with grouped navigation, branding, and a user footer; a topbar with route-driven page title and subtitle, search box, theme toggle, notifications button, and a primary "New" action; a soft-gray content background with panel-based cards; and complete light and dark themes driven by a shared design-token layer. Collapsing the sidebar and switching themes updates the whole interface consistently.

**Why this priority**: Every page in this spec (and every future feature) renders inside this shell and inherits its tokens. Without the token layer, shell dimensions, and theme behavior matching the reference, no page can "look like Helix." This story alone turns the foundation shell into the real product shell.

**Independent Test**: Start the application, land on the overview route, and verify: sidebar expanded width 248px / collapsed 68px with icon-only mode; topbar height 60px with title, subtitle, search, theme toggle, notifications, and "New" button; light and dark themes both render with the specified palette; sidebar collapse and theme mode are held in the central store.

**Acceptance Scenarios**:

1. **Given** the application is running, **When** a user visits the root URL, **Then** they are redirected to the tenant overview page rendered inside the Helix-style shell (sidebar + topbar + scrollable content area) with no page-level body scroll.
2. **Given** the shell is displayed, **When** the user activates the sidebar collapse control, **Then** the sidebar animates from 248px to 68px, shows icon-only navigation with accessible labels, and the collapsed state is reflected in the central store.
3. **Given** the application is in light mode, **When** the user activates the theme toggle, **Then** the mode advances to dark (cycling light → dark → system on subsequent clicks), the entire interface (shell, cards, badges, text, borders, shadows) switches palette using shared tokens, the toggle's icon reflects the current mode, and the choice persists across reloads.
4. **Given** the sidebar is expanded, **Then** it shows the brand block (gradient logo square, product name, "Support AI" subtitle), grouped navigation (Workspace: Overview, Conversations, Customers · AI: AI Agent, Knowledge Base, Integrations · Insights: Analytics · Settings), a red count badge on Conversations, and a footer user card with avatar, name, role, and logout control.
5. **Given** the user navigates between dashboard routes, **Then** the active navigation item is highlighted with the accent-soft background and accent-strong text, carries `aria-current="page"`, and the topbar title/subtitle update per the route's title mapping.
6. **Given** any dashboard screen in either theme, **Then** text, borders, and interactive states meet contrast expectations and all interactive elements are real buttons/links with visible keyboard focus states.

---

### User Story 2 - Overview Cockpit Page (Priority: P2)

A support operations lead opens the Overview page and sees a dense, information-rich cockpit: a dismissible amber alert banner about elevated AI provider latency, five metric cards (total conversations, resolved by AI, escalation rate, average response time, satisfaction) each with icon, value, delta badge, and sparkline, a large conversation-trend chart card with legend, a channel breakdown card (Website, WhatsApp, Telegram, Mobile SDK), and a recent activity/inbox preview panel.

**Why this priority**: The Overview page is the landing route and the single strongest proof that the visual system delivers the "Helix Admin" quality bar. It exercises nearly every reusable component (cards, metric cards, badges, charts, activity list) that the remaining pages reuse.

**Independent Test**: Navigate to the overview route and verify the alert banner, five metric cards, trend chart card, channel breakdown card, and activity panel all render from fixture data in both themes, and the alert can be dismissed.

**Acceptance Scenarios**:

1. **Given** the Overview page loads, **Then** an amber alert banner renders with icon, short title, description, and a dismiss button, and dismissing it removes it from view for the session.
2. **Given** the Overview page loads, **Then** exactly five metric cards render, each with an icon badge, label, main value, delta badge (positive/negative styling), and a small sparkline visual, laid out per the reference density.
3. **Given** the Overview page loads, **Then** a large chart card shows conversation, AI-resolved, and escalation trend lines as a lightweight vector graphic with a legend, and a breakdown card shows the four channels with proportional visuals.
4. **Given** the Overview page loads, **Then** a side panel lists recent conversations/escalations with avatar, name, channel badge, status badge, snippet, and time.
5. **Given** the viewport narrows to tablet width, **Then** the card grid wraps without broken or overlapping layout.

---

### User Story 3 - Conversations Inbox Experience (Priority: P3)

A support agent opens the Conversations page and sees a realistic three-pane shared inbox: a filterable conversation list (avatar, name, channel, status, sentiment, snippet, time), a message thread showing customer, AI, and human messages plus system events with a reply composer, AI suggestion card, and takeover/hand-back control, and a customer detail sidebar (profile, email, tier, orders, customer-since, sentiment, recent activity). Selecting a conversation updates the thread and customer panel; changing a filter updates the list.

**Why this priority**: Conversations is the core daily-use screen of the product and the most complex composition in the reference design. It also proves the feature-local state pattern (selected conversation, filters) end to end.

**Independent Test**: Navigate to the conversations route, select different conversations from the list, and verify the thread and customer sidebar update; apply a status filter and verify the list updates — all from fixture data, held in feature-local state.

**Acceptance Scenarios**:

1. **Given** the Conversations page loads, **Then** the inbox list renders fixture conversations spanning all three channels (web, whatsapp, telegram), all three statuses (open, escalated, closed), and all three sentiments (positive, neutral, angry), each with the corresponding badges.
2. **Given** a conversation is selected, **When** the user clicks a different conversation, **Then** the thread pane renders that conversation's messages (customer, AI, human, system-event entries) and the customer sidebar shows that customer's profile, and the selection is held in feature-local page state.
3. **Given** the thread pane is visible, **Then** it includes a reply composer, an AI suggestion card, and a visible takeover / hand-back-to-AI control (visual only).
4. **Given** the user changes the status filter, **Then** the list shows only matching conversations.
5. **Given** the viewport narrows to tablet width, **Then** the customer sidebar collapses or stacks so the list and thread remain usable.

---

### User Story 4 - Workspace Pages: AI Agent, Customers, Knowledge Base, Integrations, Analytics, Settings (Priority: P4)

A workspace admin browses the remaining dashboard pages and finds each one visually complete and consistent with the shell: AI Agent configuration (profile card, tone/language/response-length controls, system prompt editor, allowed/blocked topics, escalation triggers, test-assistant preview, organized in Behavior/Prompt/Escalation/Testing tabs), Customers (search/filter toolbar plus a dense customer table with avatars, tiers, CSAT, and activity fields), Knowledge Base (article cards with status and source-type badges, search, category filter, "New article" action, re-index indicator, and an empty state), Integrations (a card grid of eight integrations with status badges and action buttons), Analytics (date-range and channel filters, metric cards, four chart cards, top-articles table), and Settings (General/Team/Billing/API Keys/Security tabs with realistic sections: workspace profile, theme preference, notifications, team list, usage bars, invoices, masked API key, 2FA toggle, active sessions).

**Why this priority**: These pages complete the product surface so the dashboard reads as a full enterprise operating system rather than a two-page demo, but they reuse the components proven by P1–P3.

**Independent Test**: Navigate to each of the six routes and verify every required section renders from fixture data in both themes; switch AI Agent and Settings tabs and verify content changes with tab state held in feature-local state.

**Acceptance Scenarios**:

1. **Given** the AI Agent page loads, **Then** it shows the agent profile card, tone selector, language mode, response length, prompt editor shell, allowed/blocked topic lists, escalation triggers, and a test-assistant preview, organized in tabs whose switching updates visible content.
2. **Given** the Customers page loads, **Then** a panel-style table lists fixture customers with avatar, name, email, channel, tier, last interaction, interaction count, CSAT, and spend/orders fields, behind a search/filter toolbar.
3. **Given** the Knowledge Base page loads, **Then** article cards render with status badges (Draft/Published/Archived), source-type badges (Article/FAQ/PDF/URL), last-updated metadata, and a re-index indicator; a "New article" action and search/category filters are present; and filtering to zero results shows the empty-state component.
4. **Given** the Integrations page loads, **Then** a grid of eight integration cards (Website Widget, WhatsApp, Telegram, Shopify, HubSpot, Zendesk, Slack, Stripe) renders, each with icon, name, description, status badge (Connected / Not connected / Coming soon), and an action button.
5. **Given** the Analytics page loads, **Then** date-range and channel filter controls, metric cards, four chart visuals (volume, AI resolution, CSAT, handoff rate), and a top-knowledge-articles table all render.
6. **Given** the Settings page loads, **Then** five tabs render and each tab shows its required sections; toggles and usage bars are visually interactive without persisting anything.

---

### User Story 5 - Auth Screens (Priority: P5)

A visitor reaches the authentication screens (login, signup, forgot password, verify email) and sees the same visual quality as the dashboard: a centered card on a soft background with the product logo and name, clean labeled form fields, a primary accent button, secondary text links, and OTP input boxes on the verification screen — all working in both light and dark themes, with no real authentication behavior.

**Why this priority**: Auth screens are the product's first impression and prepare the ground for the next spec (real authentication UI integration), but they have no dependencies on the dashboard pages.

**Independent Test**: Visit each of the four auth routes and verify the centered card layout, branding, fields, buttons, links, and OTP boxes render correctly in both themes with no authentication logic.

**Acceptance Scenarios**:

1. **Given** a user visits `/auth/login`, `/auth/signup`, `/auth/forgot-password`, or `/auth/verify-email`, **Then** each renders a centered auth card with logo, product name, appropriately labeled fields, a primary accent button, and secondary links, consistent with the dashboard's visual language.
2. **Given** the verify-email screen, **Then** it renders a row of OTP input boxes styled to match the design.
3. **Given** any auth screen in dark mode, **Then** the card, fields, and buttons use the dark palette with sufficient contrast.
4. **Given** any auth screen, **When** the user submits the form, **Then** no real authentication occurs (visual behavior only).

---

### Edge Cases

- What happens when the sidebar is collapsed and a navigation item is focused via keyboard? Each icon-only item must expose an accessible name (label) and a visible focus state.
- What happens when the OS appearance preference changes while theme mode is "system"? The interface follows the new preference, including all Helix tokens.
- What happens when a filter (knowledge base, conversations, customers) matches zero fixture items? The shared empty-state component renders instead of a blank region.
- What happens when the Overview alert is dismissed and the page is revisited within the same session? The alert stays dismissed for the session; it may reappear on a fresh session (no persistence requirement).
- What happens on a tablet-width viewport on the Conversations page? Panels stack or the customer sidebar hides; the list and thread remain usable and nothing overlaps.
- What happens when a conversation has no selection yet (initial load)? The thread pane shows a sensible default (first conversation selected or an empty/placeholder state), never a broken pane.
- What happens when the self-hosted Geist font fails to load? The interface falls back to the system font stack without layout breakage; no font files are copied out of the reference HTML.
- What happens when content exceeds the content max width (1320px) on very wide screens? Content stays centered at max width; the background fills the remainder.

## Requirements *(mandatory)*

### Functional Requirements

#### Design Tokens & Theming

- **FR-001**: The system MUST provide a dedicated design-token layer (colors, typography scale, spacing, radii, shadows, layout dimensions, z-index, transitions) as the single source of truth for the Helix visual language; components MUST consume tokens rather than repeating hardcoded values.
- **FR-002**: The token layer MUST define complete light and dark palettes matching the Helix Admin reference values provided in the feature input (soft-gray background, white/panel surfaces, sky-blue accent with soft/strong/ink variants, green/amber/red semantic colors with soft variants, subtle borders, and two shadow levels), applied via the existing `data-theme` mechanism.
- **FR-003**: The token layer MUST define the Helix layout constants: sidebar expanded width 248px, collapsed width 68px, topbar height 60px, content max width 1320px, page padding, the radius scale (6–16px), and fast/base transition curves.
- **FR-004**: Typography MUST use the Geist display font, self-hosted as an openly licensed project dependency (no runtime font CDN), with a system-font fallback stack; a compact size scale (≈11–24px) suitable for dashboard density; and tight letter spacing for headers. Font files MUST NOT be extracted or copied from the reference HTML.

#### App Shell

- **FR-005**: The authenticated shell MUST render as sidebar + main area (topbar + scrollable page content), fill the viewport with no body-level scroll, let the sidebar and content own their own overflow, and constrain page content to the content max width.
- **FR-006**: The shell MUST use semantic landmarks: `aside` for the sidebar, `header` for the topbar, `main` for page content.

#### Sidebar

- **FR-007**: The sidebar MUST match the reference layout: expanded 248px / collapsed 68px with animated width transition, sidebar background token, right border, and icon-only mode when collapsed with accessible labels on every item.
- **FR-008**: The sidebar MUST show a brand block (30×30px gradient logo square with 9px radius, product name, "Support AI"-style subtitle) and grouped navigation — Workspace (Overview, Conversations, Customers), AI (AI Agent, Knowledge Base, Integrations), Insights (Analytics), and Settings — with uppercase ~10px section labels, ~36px item height, ~9px item radius, accent-soft active background, accent-strong active text/icon, muted inactive text, and a red count badge on Conversations.
- **FR-009**: The active navigation item MUST derive from the current route and carry `aria-current="page"`.
- **FR-010**: The sidebar footer MUST show a static user card (avatar circle, name, role/company subtitle, logout control) whose text hides in collapsed mode.

#### Topbar

- **FR-011**: The topbar MUST be 60px tall on the panel background with a bottom border and contain: route-driven page title (16px/600) and subtitle (≈11.5px, muted), a 260px search box (panel-2 background, border, 10px radius, ≈38px height, placeholder "Search conversations, customers…", ⌘K hint), 38×38px bordered icon buttons (theme toggle, notifications) with hover states, and a 38px-tall primary accent "New" button. The theme toggle MUST cycle light → dark → system on successive activations, with its icon indicating the current mode. The search box, notifications button, and "New" button are purely visual in this spec: they render hover/focus states but trigger no action (no command palette, dropdown, or creation flow).
- **FR-012**: Page title and subtitle MUST update per route using the defined mapping (Overview/"Your support cockpit", Conversations/"Shared inbox · 6 open, 2 escalated", Customers, AI Agent, Knowledge Base, Integrations, Analytics, Settings — as listed in the feature input).

#### Routes

- **FR-013**: The system MUST expose the dashboard routes `/tenant/{overview, conversations, customers, ai-agent, knowledge-base, integrations, analytics, settings}` and auth routes `/auth/{login, signup, forgot-password, verify-email}`, with the root URL redirecting to `/tenant/overview`. Route paths MUST come from the central route-constant registry, and prior tenant/auth placeholder pages/routes MUST be replaced or removed. The existing `/platform` placeholder route MUST remain but render inside the new Helix shell with a generic topbar title; its content is otherwise unchanged.

#### Reusable Components

- **FR-014**: Before page assembly, the system MUST provide reusable, business-logic-free layout components (shell, sidebar, nav group/item, topbar, page container, page header) and UI components (dashboard card, metric card, status/channel/sentiment badges, avatar, search input, icon button, empty state, loading state, section header, toolbar, data table wrapper), all consuming tokens; pages MUST compose these rather than duplicating markup or styles.
- **FR-015**: The system MUST provide the AI-specific visual components (AI confidence badge, AI suggestion card, AI thinking indicator, AI tool timeline, knowledge citation, escalation banner, agent preview card, prompt editor shell) as visual-only components for use on the Conversations and AI Agent pages.
- **FR-016**: The established UI library MUST be styled only through the token layer, never with page-scattered one-off overrides. Library components used as shared visual elements (buttons, badges, icon buttons, search input) MUST be wrapped inside project components that expose the Helix appearance. Library form controls used directly inside a single feature page (e.g., selects, tabs, switches, text fields) MAY be used unwrapped provided they consume `--app-*` tokens exclusively and any theme/library CSS-variable bridging happens once in the shared theme layer, not per component.

#### State Management

- **FR-017**: Theme mode and sidebar collapsed state MUST continue to live in the existing global UI state slice (`themeMode: 'light' | 'dark' | 'system'`, `sidebarCollapsed: boolean`); the sidebar and topbar MUST consume it, and the same state MUST NOT be duplicated elsewhere.
- **FR-018**: Feature-local page UI state MUST NOT use the global store. State shared across multiple sibling components on a page (selected conversation + filters on Conversations; active tab on AI Agent; active tab on Settings) MUST use the feature-local signal-store mechanism. State confined to a single component (analytics date range/channel selection, knowledge-base search/category filter, customers search) MAY use plain component-local signals instead, unless it grows cross-component consumers, in which case it MUST be promoted to a signal-store.

#### Pages

- **FR-019**: The Overview page MUST render: a dismissible amber alert banner (icon, title, description, dismiss); five metric cards (total conversations, resolved by AI, escalation rate, avg. response time, satisfaction) each with icon badge, label, value, delta badge, and sparkline; a large trend chart card (conversation / AI-resolved / escalation series with legend) built as a lightweight vector graphic without adding a chart library; a channel breakdown card (Website, WhatsApp, Telegram, Mobile SDK); and a recent activity/inbox preview panel.
- **FR-020**: The Conversations page MUST render the three-pane inbox (list, thread, customer sidebar) from fixture data, covering all channel/status/sentiment variants with badges; the thread MUST include customer, AI, and human messages, system events, a reply composer, an AI suggestion card, and a takeover/hand-back control; selecting a conversation MUST update the thread and customer panel; filters MUST narrow the list.
- **FR-021**: The AI Agent page MUST render the configuration surface (agent profile card, name, tone selector, language mode, response length, system prompt editor shell, allowed topics, blocked topics, escalation triggers, test-assistant preview) organized in Behavior / Prompt / Escalation / Testing tabs, with no real AI execution.
- **FR-022**: The Customers page MUST render a search/filter toolbar and a Helix-style panel table (compact rows, subtle borders, muted metadata, badges) listing fixture customers with avatar, name, email, channel, tier, last interaction, interaction count, CSAT, and spend/orders fields.
- **FR-023**: The Knowledge Base page MUST render a "New article" header action, search field, category filter, article/document cards with status badges (Draft, Published, Archived), source-type badges (Article, FAQ, PDF, URL), last-updated metadata, a re-index status indicator, and the shared empty state when no results match.
- **FR-024**: The Integrations page MUST render a card grid of the eight named integrations, each with icon placeholder, name, description, status badge (Connected / Not connected / Coming soon), and an action button.
- **FR-025**: The Analytics page MUST render date-range and channel filter controls, metric cards, four lightweight chart visuals (conversation volume, AI resolution, CSAT, handoff rate), and a top-knowledge-articles table, without adding a chart library.
- **FR-026**: The Settings page MUST render General / Team / Billing / API Keys / Security tabs containing: workspace profile, theme preference, notification preference, team member list, usage bars, invoice list, masked API key field, 2FA toggle, and active sessions list — all visual, with no persistence.
- **FR-027**: The auth screens (login, signup, forgot password, verify email) MUST render centered auth cards with product branding, labeled form fields, primary accent button, secondary links, and OTP boxes on the verification screen, in both themes, with no real authentication logic.

#### Interactions

- **FR-028**: The following interactions MUST work using global or local UI state only: sidebar collapse/expand, theme toggle, route-driven active nav, overview alert dismiss, conversation selection, conversation filter change, AI Agent tab switch, Settings tab switch, visual toggle switches, and any drawer open/close used.

#### Mock Data

- **FR-029**: All page content MUST come from clearly named fixture files in a dedicated fixtures location (conversations, customers, analytics, knowledge, integrations, settings), kept separate from real services so future backend integration can replace them without touching page composition; no real backend calls, authentication, persistence, or business workflows may be introduced.

#### Responsive & Accessibility

- **FR-030**: The dashboard MUST remain usable at desktop, laptop, and tablet widths: card grids wrap, the topbar search shrinks or hides, the Conversations page stacks or hides its customer sidebar, and auth screens stay centered. Full mobile optimization is out of scope, but no layout may completely break.
- **FR-031**: All clickable actions MUST be real buttons or links (no clickable divs) with visible keyboard focus states; all inputs MUST have proper labels; both themes MUST provide sufficient text/border contrast.
- **FR-032**: The implementation MUST recreate the reference design in the application's own components and styles; the reference HTML's markup, embedded assets, and fonts MUST NOT be copied directly.

### Key Entities *(fixture data only — no persistence)*

- **Conversation (fixture)**: customer reference, channel (web/whatsapp/telegram), status (open/escalated/closed), sentiment (positive/neutral/angry), snippet, timestamp, unread/escalation flags, message list.
- **Message (fixture)**: author type (customer/AI/human/system), body, timestamp; AI messages may carry confidence and knowledge-citation visuals.
- **Customer (fixture)**: name, email, avatar, channel, tier, customer-since date, orders/spend, CSAT, interaction count, recent activity entries.
- **Metric (fixture)**: label, value, delta, trend series for sparkline/chart visuals.
- **Knowledge Article (fixture)**: title, category, status (Draft/Published/Archived), source type (Article/FAQ/PDF/URL), last-updated, re-index state.
- **Integration (fixture)**: name, description, status (Connected/Not connected/Coming soon).
- **Workspace Settings (fixture)**: workspace profile, team members, usage, invoices, masked API key, security items (2FA, sessions).
- **App UI State**: theme mode and sidebar collapsed flag (existing global slice, reused).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A reviewer comparing the running application side-by-side with the Helix Admin reference confirms the shell (sidebar, topbar, backgrounds, cards, badges, accent color, density) matches the reference's visual direction in both light and dark themes on all 12 routes.
- **SC-002**: All 8 dashboard pages and 4 auth screens render their required sections from fixture data with zero blank or broken regions, in both themes, at desktop and tablet widths.
- **SC-003**: A user can collapse/expand the sidebar, switch themes, dismiss the overview alert, select conversations, change filters, and switch AI Agent and Settings tabs — each interaction responding immediately (perceived as instant) with no page reload.
- **SC-004**: 100% of navigation items reflect the active route correctly, and the topbar title/subtitle match the defined mapping on every dashboard route.
- **SC-005**: Keyboard-only navigation can reach and operate every interactive control, including icon-only collapsed sidebar items (each announcing an accessible name), on shell and all pages.
- **SC-006**: The full quality gate (build, tests, lint, format check) passes with zero errors, and existing foundation tests continue to pass alongside new visual-behavior tests (sidebar groups/collapse, topbar title, theme toggle, overview metrics, conversation selection, auth render).
- **SC-007**: Zero real backend calls, authentication logic, or persistence beyond the already-established theme-mode persistence are introduced (verifiable by inspection of the network activity and the fixtures-only data flow).
- **SC-008**: The next feature (real authentication UI integration) can begin without reworking this spec's screens — auth screens and shell expose the visual structure it needs.

## Assumptions

- Placeholder branding "Helix / Support AI" is used for the sidebar brand block and auth screens; it will be replaced by the real product name in a later spec.
- The exact color, dimension, and typography values supplied in the feature input (light/dark palettes, layout constants, font sizes) are the authoritative initial token values; minor refinements are allowed if side-by-side review shows a closer match to the reference.
- The existing foundation's placeholder routes (`/tenant/overview-placeholder`, `/platform/overview-placeholder`, `/auth/login-placeholder`) are superseded: tenant and auth placeholders are replaced by the real visual routes; the platform placeholder keeps its content but is re-hosted inside the new Helix shell (full platform dashboard visuals remain out of scope for this spec).
- The root redirect changes from the tenant overview placeholder to `/tenant/overview`.
- Theme-mode persistence and "system" mode behavior established in spec 002 are reused unchanged; no additional persistence (sidebar state, alert dismissal, settings values) is added.
- Charts and sparklines are lightweight custom vector visuals; no charting library is added in this spec.
- The Geist display font is self-hosted via an openly licensed package dependency; the system font stack applies wherever it fails to load. Nothing is extracted from the reference HTML.
- The Conversations sidebar badge count and topbar subtitle counts are static fixture values (e.g., "6 open, 2 escalated") and need not stay in sync with fixture list contents.
- "Session" for alert dismissal means in-memory only: the alert may reappear after a full reload.
- Tablet width means approximately 768–1024px; the foundation's existing breakpoint tokens define the exact values.
