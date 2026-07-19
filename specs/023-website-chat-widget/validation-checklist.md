# Validation Checklist — Website Chat Widget

Manual validation scenarios derived from [quickstart.md](quickstart.md). Each item includes the expected outcome for pass/fail determination.

---

## Automated gates

- [X] `cargo test --lib -p server -p widgets` — unit tests pass (17/17)
- [X] `pnpm ng build widget` — passes (173.56 kB, within 180kB error budget)
- [X] `pnpm build:widget-loader` — 2974 bytes (≤ 10KB ✓)
- [X] `pnpm ng test widget` — 13/13 passed
- [X] `pnpm ng build dashboard` — passes
- [X] `pnpm ng test dashboard` — 964/964 passed (2 pre-existing knowledge-base errors, out of scope)
- [ ] `pnpm test:e2e` — requires Playwright/browser setup (not run in this session)
- [X] `pnpm lint` — passes
- [X] `pnpm format:check` — passes
- [X] `cargo fmt --check` — passes
- [X] `cargo clippy -p server --lib -p widgets -- -D warnings` — clean
- [X] `cargo test -p server --test openapi_coverage` — 3/3 passed (widget routes added to inventory)

---

## Scenario 1 — Tenant creates & embeds a widget (US2, US5)

- [ ] **1.1** Dashboard UI: create widget instance "Demo site" — fields accepted, live preview tracks each edit before saving
- [ ] **1.2** Copy embed snippet from dashboard — snippet returned by `GET /tenant/widgets/{id}/snippet`
- [ ] **1.3** Paste snippet into `frontend/e2e/fixtures/widget-host.html` and serve locally — launcher renders with configured color and position
- [ ] **1.4** Open launcher — branded window appears with the configured welcome message
- [ ] **1.5** Config endpoint check: `curl /widget/v1/config?widgetId=wgt_…` returns public fields only (no tenant_id, id, timestamps, allowed_domains)
- [ ] **1.6** Invalid `data-widget-id` → page renders no widget, no console errors in host page (FR-005)
- [ ] **1.7** Disable instance in dashboard → next page load renders nothing
- [ ] **1.8** Set `allowedDomains` to `only-this.example` → refresh host page → widget silent (no iframe/launcher)
- [ ] **1.9** Clear `allowedDomains` list → widget back on page load

---

## Scenario 2 — Visitor chats with the AI (US1)

- [ ] **2.1** Open widget, send "What are your support hours?" — message appears instantly
- [ ] **2.2** Responding indicator (typing dots) shows after sending
- [ ] **2.3** AI reply streams incrementally (deltas visible, not one paste) — attributed to "assistant"
- [ ] **2.4** Typing indicator clears when the reply starts
- [ ] **2.5** Send a follow-up message — appended to the same conversation in order
- [ ] **2.6** Dashboard inbox: conversation appears with `channel: widget` and originating instance name (FR-018/FR-032)
- [ ] **2.7** Send 6+ messages rapidly → friendly slow-down notice (429 mapped), conversation intact
- [ ] **2.8** Empty message blocked (send button disabled)
- [ ] **2.9** >4000 characters blocked with character counter feedback
- [ ] **2.10** Stop AI worker (or unset provider) → after timeout, widget shows friendly error with retry button
- [ ] **2.11** Typed text preserved in the textarea when send fails and error state is shown

---

## Scenario 3 — Human handoff & away (US3)

- [ ] **3.1** Escalate conversation (trigger escalation rule or ask for human) while an agent is Available
- [ ] **3.2** Widget switches to handoff state within ~5 seconds (SC-006)
- [ ] **3.3** Agent claims and replies from dashboard inbox → reply renders in widget attributed by display name, styled distinctly from AI messages
- [ ] **3.4** After human takes over, further visitor messages get no AI replies (FR-021)
- [ ] **3.5** Toggle all agents to unavailable, escalate a fresh conversation → widget shows the **away** variant ("team will reply when back")
- [ ] **3.6** Visitor messages still deliver when team is away (FR-028)
- [ ] **3.7** Resolve conversation from dashboard → widget shows "conversation ended" note
- [ ] **3.8** Next visitor message after resolution starts a brand-new conversation (FR-027)
- [ ] **3.9** Reloading page after resolution shows no closed conversation history

---

## Scenario 4 — Session continuity (US4)

- [ ] **4.1** Mid-conversation, reload the host page → reopen widget → same conversation and history restored (FR-008)
- [ ] **4.2** Expire the session (shorten `expires_at` in DB or config) → next open silently mints a new session, fresh start, no errors (FR-010)
- [ ] **4.3** Different browser profile → independent session (no cross‑session leakage)

---

## Cross‑cutting checks

- [ ] **C.1** Isolation (FR-006): host fixture includes hostile CSS (`* { all: unset !important }`-style rules) → widget unaffected; widget styles absent from host DOM
- [ ] **C.2** Tenant isolation (SC-005): create instances under two tenants → each host page only reaches its own tenant's config/conversations
- [ ] **C.3** Session token from tenant A used against tenant B's conversation → 404 or 401 (never a data leak)
- [ ] **C.4** Double include: snippet twice on one page → single launcher (US2-5 guard)
- [ ] **C.5** Observability: public endpoints log with request IDs; SSE relay and rate limiter emit tracing
- [ ] **C.6** Accessibility: keyboard trap inside open window (Tab cycles focusables)
- [ ] **C.7** Accessibility: Escape closes the window, focus returns to launcher button
- [ ] **C.8** Accessibility: `aria-live="polite"` on message list announces new messages to screen readers

---

## Pass / Fail summary

| Section | Total | Pass | Fail | Notes |
|---------|-------|------|------|-------|
| Automated gates | 9 | | | |
| Scenario 1 | 9 | | | |
| Scenario 2 | 11 | | | |
| Scenario 3 | 9 | | | |
| Scenario 4 | 3 | | | |
| Cross‑cutting | 8 | | | |
| **Total** | **49** | | | |
