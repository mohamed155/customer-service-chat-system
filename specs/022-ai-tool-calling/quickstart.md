# Quickstart: AI Tool Calling — Validation Guide

Proves the feature end-to-end. Prerequisites and conventions match 021 (Postgres + Redis up, migrations applied, seeded dev tenant with a configured AI agent, `APP_AI_KEY_ENCRYPTION_KEY` set, dashboard dev server running).

## Setup

```bash
# Backend (from backend/): run migrations + start server
cargo run -p server            # applies 0049_ai_tool_calling.sql on boot

# Frontend (from frontend/):
pnpm ng serve dashboard
```

Enable tools for the dev tenant (as a tenant Admin in the dashboard): Settings → Tools → enable `lookup_customer` (auto) and `update_customer_contact` (approval-required). Contracts: [tool-settings-api.md](contracts/tool-settings-api.md).

## Scenario 1 — Auto-approved tool end-to-end (US1)

1. Open an AI-handled conversation; as the customer, ask: *"What email address do you have on file for me?"*
2. **Expect**: AI reply contains the seeded customer's email; conversation detail shows a tool timeline entry `lookup_customer · succeeded` with duration and arguments; `GET /tenant/conversations/{id}/tool-activity` returns the request with `status: "succeeded"`, `chain_index: 0`.
3. Reload — timeline entry persists (stored in `tool_requests`, [data-model.md](data-model.md)).

## Scenario 2 — Two-phase approval (US2 + clarification Q1)

1. As the customer: *"Please update my email to new@example.com."*
2. **Expect**: interim AI holding message appears; an approval card renders for staff (tool, arguments, expiry countdown) via `tool.request.created` SSE.
3. As an Agent+ member, click **Approve**.
4. **Expect**: card resolves; `update_customer_contact` executes exactly once; a follow-up AI message confirms the change; customer profile shows the new email; request record shows `approved`, decider, `succeeded`, duration.
5. Repeat with **Deny** → tool never executes (`started_at` null), follow-up AI message responds without the change.
6. Repeat and let the 5-minute window lapse → status `expired`, follow-up generation treats it as declined.

## Scenario 3 — Validation refusals & failure visibility (US1/US3)

1. Disable `lookup_customer`, ask the Scenario 1 question again → request recorded `refused`, AI answers without the tool, refusal visible in the timeline.
2. Register a tenant-defined tool pointing at an unreachable HTTPS endpoint, enable it, drive the AI to call it → entry shows `failed`/`timed_out` with sanitized error, distinct failure styling, customer still gets a reply (FR-010).
3. Verify the customer-side widget/view shows only AI messages — no tool names, arguments, results, or errors (FR-020).

## Scenario 4 — Tenant-defined tool & credential confidentiality (US4)

1. Settings → Tools → register `check_order_status` with a test HTTPS endpoint + credential; verify the API response and UI show `has_credential: true` but never the secret.
2. Drive a call; verify the endpoint received the POST contract body and `Authorization` header.
3. Verify the credential appears nowhere: settings responses, tool-activity payloads, SSE events, logs (SC-008).
4. Switch to a second tenant → the tool is invisible in settings and unavailable to its AI (FR-002).

## Scenario 5 — Concurrency & bounds

1. Two staff sessions decide the same pending request simultaneously → one gets `200`, the other `409` with the settled state; exactly one execution ([tool-approvals-api.md](contracts/tool-approvals-api.md)).
2. Ask a question forcing repeated lookups (or lower the max via env override) → chain cuts off at the platform max; AI answers with what it has; cutoff recorded.

## Test suites

```bash
# Backend — unit + integration (Postgres-backed) + API contract
cd backend && cargo test

# Frontend — component/store specs + hygiene
cd frontend && pnpm ng test dashboard && pnpm lint && pnpm format:check
```

Key suites: provider mapping round-trips per vendor (`ai-providers`), policy resolution & state-machine unit tests (`tools`), engine tool-loop + two-phase integration tests (`server/tests`), isolation tests (SC-003), settings/approval RBAC contract tests, timeline/approval-card/result-viewer component specs.
