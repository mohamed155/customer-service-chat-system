# Quickstart: Website Chat Widget — validation guide

Proves the feature end-to-end against [spec.md](spec.md) user stories. Contracts: [public API](contracts/public-widget-api.md), [admin API](contracts/widget-admin-api.md). Data model: [data-model.md](data-model.md).

## Prerequisites

- PostgreSQL running with `DATABASE_URL` set; from `backend/`: `sqlx migrate run` (applies `0050_website_chat_widget.sql`).
- Backend server running (from `backend/`: `cargo run -p server`, or the repo's start script) with an AI provider configured for the test tenant (spec 021 setup) so replies actually generate.
- Frontend deps installed (`cd frontend && pnpm install`).
- A signed-in tenant Owner/Admin user (spec 007) for the dashboard steps.

## Automated gates (run all before claiming done)

```bash
# backend — from backend/
cargo test                       # unit + DB tests incl. widgets module, rate limiter, origin check, router permission tests

# frontend — from frontend/
pnpm ng build widget             # must stay within the 97KB initial budget
pnpm build:widget-loader         # loader bundle ≤ 10KB (script added by this feature; fails if oversize)
pnpm ng test widget
pnpm ng build dashboard && pnpm ng test dashboard
pnpm test:e2e                    # includes widget embed/chat/handoff specs (e2e/ host-page fixture)
pnpm lint && pnpm format:check
```

## Scenario 1 — Tenant creates & embeds a widget (US2, US5)

1. Dashboard → tenant → Widgets: create instance "Demo site"; set display name, primary color, welcome message, position, theme. Verify the live preview tracks each edit before saving.
2. Copy the embed snippet (`GET /tenant/widgets/{id}/snippet`).
3. Open the e2e host-page fixture (`frontend/e2e/fixtures/widget-host.html` served locally) with the snippet pasted in.
4. **Expected**: launcher renders with the configured color/position; opening it shows the branded window with the welcome message. Config endpoint check: `curl "http://localhost:<port>/widget/v1/config?widgetId=wgt_…"` returns exactly the public fields — no tenant IDs.
5. Negative: change the snippet's `data-widget-id` to garbage → page renders no widget, no console errors thrown into the host page (FR-005). Disable the instance in the dashboard → next page load renders nothing.
6. Allowlist: set `allowedDomains` to `only-this.example`; reload host page → widget silent; clear the list → widget back.

## Scenario 2 — Visitor chats with the AI (US1)

1. On the host page, open the widget and send "What are your support hours?".
2. **Expected**: message appears instantly; responding indicator shows; AI reply streams in incrementally (deltas visible, not one paste) attributed to the assistant; indicator clears when the reply starts.
3. Send a follow-up → appended to the same conversation in order.
4. Dashboard inbox: the conversation appears with `channel: widget` and the instance name attribution (FR-018/FR-032).
5. Negative: send 6+ messages rapidly → friendly slow-down notice (429 mapped), conversation intact; empty message blocked; >4000 chars blocked with feedback.
6. Failure state: stop the AI worker (or unset provider) → after timeout the widget shows the friendly error with retry; typed input preserved.

## Scenario 3 — Human handoff & away (US3)

1. Drive the conversation into escalation (e.g., trigger the tenant's escalation rule or ask for a human, per 014/021 behavior) while an agent is Available in the dashboard.
2. **Expected**: widget switches to the handoff state within ~5 s (SC-006); agent claims and replies from the inbox → reply renders in the widget attributed by display name, styled distinctly from AI messages; further visitor messages get no AI replies.
3. Toggle all agents to unavailable, escalate a fresh conversation → widget shows the **away** variant ("team will reply when back"); visitor messages still deliver (FR-028).
4. Resolve the conversation from the dashboard → widget shows the "conversation ended" note; next visitor message starts a brand-new conversation (FR-027); reloading shows no closed history.

## Scenario 4 — Session continuity (US4)

1. Mid-conversation, reload the host page → reopen widget → same conversation and history restored (FR-008).
2. Expire the session (shrink `expires_at` in DB or config) → next open silently mints a new session, fresh start, no errors (FR-010).
3. Different browser profile → independent session.

## Cross-cutting checks

- **Isolation (FR-006)**: host fixture includes hostile CSS (`* { all: unset !important }`-style rules) → widget unaffected; widget styles absent from host DOM.
- **Tenant isolation (SC-005)**: create instances under two tenants; verify each host page only ever reaches its own tenant's config/conversations; a session token from tenant A used against tenant B's conversation → 404/401.
- **Double include (US2-5)**: snippet twice on one page → single launcher.
- **Observability**: public endpoints log with request IDs; SSE relay and rate limiter emit tracing.
