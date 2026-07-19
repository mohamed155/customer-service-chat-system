# Feature Specification: Website Chat Widget

**Feature Branch**: `023-website-chat-widget`

**Created**: 2026-07-18

**Status**: Draft

**Input**: User description: "Website Chat Widget — Create the first customer-facing communication channel. Scope: embeddable script, widget UI, anonymous customer session, conversation creation, message sending, AI response display, human handoff state. Backend: public widget configuration endpoint, public conversation endpoint, secure tenant identification for widget, rate limiting. Frontend: widget package, chat launcher, chat window, message list, composer, loading state, handoff state. Acceptance: a tenant can embed the widget on a website, customers can start conversations, AI can reply through the widget, widget respects tenant branding."

## Clarifications

### Session 2026-07-18

- Q: Should tenants be able to restrict which website domains can load their widget? → A: Optional allowlist — tenant may configure allowed domains; empty list means the widget works anywhere; requests from non-allowed origins are rejected.
- Q: When a widget conversation is resolved/closed, what should the visitor experience be? → A: Closed conversations are locked; the widget shows a "conversation ended" note and the visitor's next message starts a fresh conversation; prior closed conversations are not shown on reload.
- Q: How should AI replies appear in the widget? → A: Streamed — reply text appears incrementally as it is generated.
- Q: When a conversation is escalated but no human agents are available, what should the visitor see? → A: An "away" variant of the handoff state ("our team is currently away — we'll reply as soon as someone is back"); visitor messages still queue for later.
- Q: How much widget-settings UI should the tenant dashboard get in v1? → A: Full settings experience — a rich dashboard page with live widget preview, position and theme options, and support for multiple widget instances per tenant (each with its own identifier and configuration).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Customer chats with the AI through the widget (Priority: P1)

A website visitor on a tenant's site sees a chat launcher button in the corner of the page. They click it, a chat window opens with a welcome message, and they type a question and send it. The widget shows that the AI assistant is working on a reply, and the AI's answer appears in the conversation shortly after. The visitor can continue the conversation with follow-up messages.

**Why this priority**: This is the entire reason the channel exists — a customer getting an AI answer through the widget is the smallest end-to-end slice that delivers value. Without it, nothing else in this feature matters.

**Independent Test**: Can be fully tested by loading a test page that hosts the widget for a known tenant, opening the chat window, sending a message, and observing an AI reply appear in the message list.

**Acceptance Scenarios**:

1. **Given** a page where the widget is active for a tenant, **When** the visitor clicks the chat launcher, **Then** the chat window opens and displays the tenant's configured welcome message.
2. **Given** an open chat window with no prior conversation, **When** the visitor sends their first message, **Then** a new conversation is created for that tenant, the visitor's message appears immediately in the message list, and a visible "assistant is responding" indicator is shown.
3. **Given** a visitor has sent a message, **When** the AI produces a reply, **Then** the reply text appears incrementally in the message list as it is generated, attributed to the assistant, and the responding indicator disappears once the reply begins.
4. **Given** an ongoing conversation, **When** the visitor sends additional messages, **Then** each message and its AI reply are appended to the same conversation in order.
5. **Given** the visitor sends a message and the reply cannot be produced (service failure or timeout), **When** the failure occurs, **Then** the widget shows a friendly error state and lets the visitor retry without losing their typed message or the conversation history.

---

### User Story 2 - Tenant embeds and brands the widget (Priority: P2)

A tenant administrator copies a small embed snippet (containing the tenant's public widget identifier) into their website's HTML. When the site loads, the widget appears styled with the tenant's branding — display name, colors, and welcome text — as configured for that tenant. Visitors on the site can only ever reach that tenant's assistant and data.

**Why this priority**: Embedding is how the channel is distributed and branding is a stated acceptance criterion; but it is only valuable once the core chat loop (US1) works.

**Independent Test**: Can be tested by placing the embed snippet on a blank test page for two different tenants and verifying each page renders its own tenant's branding and routes conversations to the correct tenant.

**Acceptance Scenarios**:

1. **Given** a tenant with a valid public widget identifier, **When** a page containing the embed snippet loads, **Then** the widget initializes by fetching that tenant's public widget configuration and renders the launcher.
2. **Given** a tenant has configured branding (display name, primary color, welcome message), **When** the widget loads on any page, **Then** the launcher and chat window reflect that branding.
3. **Given** an embed snippet with an invalid or unknown widget identifier, **When** the page loads, **Then** the widget does not render a chat interface and fails silently without breaking the host page.
4. **Given** two different tenants embed the widget on their respective sites, **When** visitors chat on each site, **Then** conversations are created under the correct tenant only, and no configuration or conversation data from one tenant is ever exposed on the other's site.
5. **Given** the widget script is included twice on the same page, **When** the page loads, **Then** only one launcher and one chat window instance appear.

---

### User Story 3 - Conversation is handed off to a human (Priority: P3)

During a conversation, the AI determines it cannot help and escalates to a human agent (per the existing escalation flow). The visitor sees the widget change into a handoff state that explains a human will take over. When a human agent replies from the dashboard, their messages appear in the widget attributed to a person rather than the assistant.

**Why this priority**: Handoff is essential for trust and completes the loop with the existing escalation/routing capability, but it depends on the core chat loop existing first.

**Independent Test**: Can be tested by driving a conversation into escalation and verifying the widget displays the handoff state, then sending an agent reply from the dashboard and verifying it renders in the widget.

**Acceptance Scenarios**:

1. **Given** an active widget conversation, **When** the conversation is escalated to a human, **Then** the widget displays a clear handoff state (e.g., "connecting you with a member of our team") instead of the AI-responding indicator.
2. **Given** a conversation in the handoff state, **When** a human agent sends a reply, **Then** the message appears in the widget attributed to a human agent, distinct from assistant messages.
3. **Given** a conversation in the handoff state, **When** the visitor sends further messages, **Then** those messages are delivered to the conversation for the human agent to see, and the AI does not reply.
4. **Given** a conversation waiting for a human, **When** no agent has responded yet, **Then** the widget continues to show the waiting/handoff state rather than an error.
5. **Given** a conversation is escalated while no human agents are available, **When** the widget updates, **Then** it shows an "away" variant of the handoff state telling the visitor the team will reply when someone is back, and further visitor messages are still recorded for later handling.

---

### User Story 4 - Returning visitor resumes their conversation (Priority: P4)

A visitor who chatted earlier reloads the page or navigates to another page on the same site. The widget restores their anonymous session and, when opened, shows their existing conversation history so they can continue where they left off.

**Why this priority**: Session continuity makes the widget feel dependable and is expected behavior, but a first version delivers value even if every page load started fresh.

**Independent Test**: Can be tested by starting a conversation, reloading the page, reopening the widget, and verifying the prior messages are displayed and new messages append to the same conversation.

**Acceptance Scenarios**:

1. **Given** a visitor with an active anonymous session and conversation, **When** they reload the page or navigate within the same site, **Then** the widget restores the same session and shows the existing conversation history.
2. **Given** a visitor whose anonymous session has expired, **When** they open the widget, **Then** they are treated as a new visitor and can start a fresh conversation without errors.
3. **Given** a visitor on a different browser or device, **When** they open the widget on the same site, **Then** they get a new independent session (no cross-device continuity is expected).

---

### User Story 5 - Tenant manages widget settings in the dashboard (Priority: P5)

A tenant administrator opens a widget settings page in the dashboard. They can create one or more widget instances (e.g., one per website), and for each instance edit its display name, colors, welcome message, on-page position, and light/dark theme; toggle it on or off; manage its allowed-domains list; and copy its embed snippet. A live preview next to the settings shows exactly how the widget will look as they edit.

**Why this priority**: Full self-serve configuration completes the tenant experience, but the channel already delivers value once a widget can be embedded and branded (US2); the richer management surface builds on everything before it.

**Independent Test**: Can be tested by creating two widget instances with different branding in the dashboard, verifying the live preview tracks each edit, and confirming each instance's embed snippet renders its own configuration on separate test pages.

**Acceptance Scenarios**:

1. **Given** a tenant administrator on the widget settings page, **When** they create a new widget instance, **Then** it receives its own public widget identifier and embed snippet.
2. **Given** an administrator editing a widget instance's branding, position, or theme, **When** they change a setting, **Then** the live preview updates immediately to reflect it.
3. **Given** an administrator saves changes to a widget instance, **When** a page embedding that instance next loads, **Then** the widget reflects the updated configuration.
4. **Given** a tenant with multiple widget instances, **When** conversations arrive from different instances, **Then** each conversation records which widget instance it originated from.
5. **Given** an administrator disables a widget instance, **When** a page embedding it loads, **Then** no chat interface renders (per the silent-failure rule).

---

### Edge Cases

- Unknown, disabled, or revoked widget identifier: the widget must fail silently and must not disrupt the host page.
- A visitor sends messages faster than allowed or a script floods the endpoints: rate limiting rejects the excess with a clear, non-technical message in the widget, without ending the conversation.
- The AI takes unusually long to respond: the widget keeps showing the responding indicator up to a timeout, then offers a retry rather than hanging forever.
- Network loss mid-conversation: the widget indicates the connection problem and recovers when connectivity returns; unsent visitor text is preserved.
- Empty or whitespace-only messages: the composer prevents sending them.
- Extremely long messages: the composer enforces a maximum length and tells the visitor when they exceed it.
- The host page has aggressive styles or conflicting scripts: the widget must render correctly regardless of host page CSS and must not leak its own styles into the host page.
- A conversation is escalated while the visitor has the window closed: on next open, the widget reflects the current handoff state.
- A conversation is resolved/closed while the visitor has the window closed: on next open, the widget shows the "conversation ended" note and offers a fresh start.
- The tenant changes branding while a widget is open: existing sessions may keep the old branding until next load; new loads get the updated branding.

## Requirements *(mandatory)*

### Functional Requirements

**Embedding & configuration**

- **FR-001**: Tenants MUST be able to embed the widget on any website by adding a single small script snippet containing their public widget identifier.
- **FR-002**: The system MUST expose a public (unauthenticated) widget configuration endpoint that, given a valid public widget identifier, returns only that widget instance's publicly safe settings (display name, branding colors, welcome message, position, theme, enabled/disabled state).
- **FR-003**: The public widget identifier MUST NOT be a secret credential and MUST NOT grant access to any tenant data beyond the public configuration and the visitor's own conversations.
- **FR-004**: The widget MUST render with the tenant's configured branding (display name, primary color, welcome message) and fall back to sensible defaults when branding is not configured.
- **FR-005**: When the widget identifier is invalid, or the widget is disabled for the tenant, the widget MUST NOT render a chat interface and MUST NOT break or visually disrupt the host page.
- **FR-006**: The widget MUST be visually and behaviorally isolated from the host page: host page styles must not corrupt the widget, and widget styles must not alter the host page.
- **FR-026**: Tenants MUST be able to optionally configure an allowed-domains list per widget instance; when the list is non-empty, widget requests originating from other domains MUST be rejected (and the widget fails silently per FR-005), and when the list is empty the widget works on any domain.

**Widget management (dashboard)**

- **FR-029**: Tenants MUST be able to create, edit, disable, and delete multiple widget instances, each with its own public widget identifier, branding, position, theme, allowed-domains list, and embed snippet.
- **FR-030**: The widget settings page MUST provide a live preview that immediately reflects branding, position, and theme edits before saving.
- **FR-031**: Each widget instance's settings MUST include on-page position (e.g., bottom-right or bottom-left) and a light/dark theme choice, and the embedded widget MUST honor them.
- **FR-032**: Every widget conversation MUST record which widget instance it originated from, and this MUST be visible in the tenant dashboard.
- **FR-033**: The settings page MUST present a copyable embed snippet per widget instance.

**Anonymous sessions**

- **FR-007**: The system MUST create an anonymous customer session for each new visitor without requiring registration, login, or personal information.
- **FR-008**: The anonymous session MUST persist across page loads and navigation within the same browser so the visitor can resume their conversation.
- **FR-009**: A visitor's session MUST only grant access to that visitor's own conversations within the owning tenant — never to other visitors' conversations or other tenant data.
- **FR-010**: Anonymous sessions MUST expire after a defined period of inactivity, after which the visitor starts fresh.

**Conversation & messaging**

- **FR-011**: A visitor MUST be able to start a new conversation from the widget, and the conversation MUST be recorded under the correct tenant.
- **FR-012**: A visitor MUST be able to send text messages in an open conversation, and each sent message MUST appear immediately in the widget's message list.
- **FR-013**: AI replies MUST be delivered to the widget and displayed in the message list attributed to the assistant, using the existing AI conversation capability.
- **FR-014**: The widget MUST show a clear "assistant is responding" indicator between sending a message and the start of the reply, and AI reply text MUST appear incrementally as it is generated rather than only after the full reply is complete.
- **FR-015**: The widget MUST display conversation history in chronological order with visually distinct visitor, assistant, and human-agent messages.
- **FR-016**: If a reply fails or times out, the widget MUST show a friendly error state and allow the visitor to retry without losing conversation history or typed input.
- **FR-017**: The composer MUST reject empty messages and enforce a maximum message length with clear feedback.
- **FR-018**: Widget conversations MUST be visible in the tenant dashboard alongside conversations from other sources, consistent with existing conversation views.
- **FR-027**: When a conversation is resolved/closed, it MUST become locked: the widget shows a "conversation ended" note, the visitor's next message starts a fresh conversation, and closed conversations are not displayed on subsequent loads.

**Human handoff**

- **FR-019**: When a conversation is escalated to a human (via the existing escalation flow), the widget MUST switch to a handoff state that tells the visitor a human will take over.
- **FR-020**: Human agent replies MUST appear in the widget attributed to a human agent, visually distinct from assistant messages.
- **FR-021**: While a conversation is in the handoff state, visitor messages MUST continue to be delivered to the conversation and the AI MUST NOT generate replies to them.
- **FR-028**: When a conversation is escalated and no human agents are currently available, the widget MUST show an "away" variant of the handoff state that tells the visitor the team will reply when someone is back; visitor messages continue to queue for later handling.

**Security & abuse protection**

- **FR-022**: All public widget endpoints MUST enforce rate limits per visitor session and per tenant, rejecting excess requests without ending existing conversations.
- **FR-023**: When rate limited, the widget MUST show a clear non-technical message asking the visitor to slow down or wait.
- **FR-024**: The public endpoints MUST NOT expose any tenant-internal data (agent identities beyond a display name, internal notes, configuration secrets, other conversations).
- **FR-025**: Tenant identification for all widget traffic MUST derive from the public widget identifier and the visitor's session — never from client-supplied tenant IDs used by the authenticated dashboard.

### Key Entities

- **Widget Instance**: A tenant-owned widget definition; a tenant may have several (e.g., one per website). Each carries its own configuration and public widget identifier and can be enabled, disabled, or deleted independently.
- **Widget Configuration**: The publicly safe settings of one widget instance that control how it looks and behaves — display name, branding colors, welcome message, on-page position, light/dark theme, enabled/disabled state, optional allowed-domains list.
- **Public Widget Identifier**: A non-secret, per-widget-instance value included in the embed snippet that identifies which widget instance (and therefore tenant) an embed belongs to; grants access only to public configuration and the visitor's own conversations.
- **Anonymous Customer Session**: A visitor identity created without sign-up, scoped to one tenant and one browser, with an inactivity expiry; owns the visitor's conversations.
- **Widget Conversation**: A conversation originating from the widget channel, tied to an anonymous session, a widget instance, and a tenant; carries messages from the visitor, the AI assistant, and (after handoff) human agents; has a state reflecting AI-handled vs. handed-off-to-human vs. closed.
- **Message**: A single entry in a conversation with a sender type (visitor, assistant, human agent), content, and timestamp.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant can go from receiving the embed snippet to a working, branded widget on their website in under 5 minutes, with no changes required beyond pasting the snippet.
- **SC-002**: A first-time visitor can open the widget and send their first message in under 15 seconds, with no sign-up or personal information required.
- **SC-003**: In normal operation, 95% of visitor messages receive a visible AI reply (or an explicit handoff/waiting state) without the visitor needing to refresh the page.
- **SC-004**: The widget begins responding to the visitor (responding indicator or reply) within 2 seconds of sending a message in 95% of cases.
- **SC-005**: 100% of widget conversations are recorded under the correct tenant, and zero cross-tenant configuration or conversation data is reachable through the public endpoints.
- **SC-006**: When a conversation is escalated, the visitor sees the handoff state within 5 seconds, and 100% of subsequent human agent replies are delivered to the widget.
- **SC-007**: Abusive traffic beyond the rate limits is rejected without degrading service for visitors of **other** tenants (tenant budgets are independent). Within a single tenant, the per-session limit bounds how much of the tenant budget any one visitor can consume.

## Assumptions

- The existing AI conversation engine (021), escalation/routing flow (014), and conversations module are reused as-is; this feature adds the customer-facing channel, not new AI or routing behavior.
- The widget is the first channel; the conversation source/channel concept may need a "widget" value but no other channels are in scope.
- Anonymous visitors are acceptable for v1 — no email capture, identity verification, or pre-chat forms are in scope.
- Branding scope for v1 is display name, primary color, welcome message, on-page position, and a light/dark theme; logo upload and fully custom CSS theming are out of scope.
- The dashboard includes a full widget settings experience: multiple widget instances per tenant, live preview, position/theme options, allowed-domains management, and per-instance embed snippets (see FR-029–FR-033).
- One active conversation per session at a time is sufficient for v1; when a conversation is closed, the visitor starts a fresh one (see FR-027) rather than managing a list of past conversations.
- Session inactivity expiry defaults to an industry-standard window (e.g., ~24 hours) unless product decides otherwise.
- Real-time delivery of agent/AI replies uses the platform's existing realtime delivery approach; visitors are not expected to refresh the page to see replies.
- Desktop and mobile browsers are both supported; native mobile SDKs are out of scope.
- The host website is a third-party site the platform does not control; the widget must not depend on anything from the host page.
