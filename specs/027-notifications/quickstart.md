# Quickstart: Notifications (027) — validation guide

Proves the feature end-to-end. Contracts: [contracts/notifications-api.md](contracts/notifications-api.md); data model: [data-model.md](data-model.md); design rationale: [research.md](research.md).

## Prerequisites

- PostgreSQL up with migrations applied through `0054_notifications.sql` (`cd backend && sqlx migrate run` or the project's usual migration flow).
- Backend: `cd backend && cargo run -p server` (API at `http://localhost:<port>/api/v1`). The notifications worker starts with the server — confirm `notifications outbox worker started` in the logs before testing.
- Frontend: `cd frontend && pnpm ng serve dashboard`.
- Seeded users in one tenant: an Owner/Admin, **two** Agents (needed for the fan-out and auto-resolve scenarios), and a second tenant with its own member for the isolation check. Dev identity flow per 006/007 (login sets `app_session` cookie; tenant calls need `X-Tenant-ID`).
- An AI agent configured well enough to run a generation, and one tool with `approval_required` set (per 022).

## Automated checks (all must pass)

### Backend (unit + integration w/ PostgreSQL)

```bash
# Start services (Docker required):
docker compose -f infra/docker-compose.yml up -d postgres redis

# Run migrations:
cd backend && DATABASE_URL="postgres://customer_service:customer_service_dev@localhost:5432/customer_service" sqlx migrate run

# Run all backend tests (unit only, skips DB-gated tests):
cargo test --workspace

# Run with database integration (full coverage):
REQUIRE_DB_TESTS=1 DATABASE_URL="postgres://customer_service:customer_service_dev@localhost:5432/customer_service" cargo test -p server --test notifications

# Performance test (SC-004) — run separately, expects empty notifications table:
REQUIRE_DB_TESTS=1 DATABASE_URL="..." cargo test -p server --test notifications list_under_one_second_with_one_thousand_notifications -- --ignored
```

### Frontend

```bash
cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check
```

Key suites: `server/tests/notifications.rs` (per-trigger creation, dedup, auto-resolve, isolation), `server/tests/rbac.rs` (inbox reachable by every tenant role), `server/tests/openapi_coverage.rs` (4 new EXPECTED entries), `server/tests/team_members.rs` (must still pass after the `notifications` → `email` crate rename — this is the regression guard for R7).

---

## Scenario 1 — Bell, list, unread count, mark read (US1 / SC-002, SC-004, SC-006)

1. As Agent A, seed one notification (easiest: have Agent B assign a conversation to Agent A).
2. Bell badge shows `1` **without reloading the page** — this is the SC-002 check; it should appear within ~1 s.
3. Open the bell → panel lists the notification, newest first, with relative time.
4. Click it → navigates to the conversation, badge returns to `0`, row shows as read.
5. Seed three more, then use "mark all as read" → badge `0`, all rows read.
6. `curl` equivalents:
   ```bash
   GET  /api/v1/tenant/notifications              # 200, newest first
   GET  /api/v1/tenant/notifications/unread-count # 200, {"data":{"count":N}}
   POST /api/v1/tenant/notifications/read-all     # 200, {"data":{"marked":N}}
   ```

## Scenario 2 — Assignment and escalation triggers (US2 / SC-001)

1. **Assignment**: as Admin, assign a conversation to Agent A → A gets one notification; Admin gets none; Agent B gets none.
2. **Self-assignment**: as Agent A, assign a conversation to yourself → **no** notification (FR-009).
3. **Escalation, routed**: trigger an AI→human escalation while Agent A is available → A receives **exactly one** notification, the escalation one, and **no** separate "assigned to you" notification (FR-009a / SC-010). This is the double-notify regression check — verify by counting rows, not by eyeballing the panel.
4. **Escalation, queued**: set all agents unavailable, trigger an escalation → every member holding `conversations.manage` receives one.

## Scenario 3 — Auto-resolve on claim (US2-AC4 / FR-011a / SC-009)

1. With a queued escalation notified to Agents A and B (Scenario 2.4), confirm both badges show it.
2. As Agent A, claim the escalation.
3. Agent B's badge **decreases within ~5 s with no action on B's part**, and the notification still appears in B's list marked resolved, still linking to the escalation.
4. If B had already *read* that notification, it stays `read` — a resolve must not rewrite it (see data-model.md state transitions).

## Scenario 4 — AI triggers (US3 / SC-001)

1. **Tool approval**: trigger a generation that calls an approval-required tool → all `conversations.manage` holders get a "tool approval required" notification linking to the pending request. Have one decide it → the others auto-resolve (same check as Scenario 3).
2. **AI failure**: force a generation failure (e.g. invalid provider credentials) → Owners/Admins plus the conversation assignee get a "failed AI response" notification.
3. **Suppression window**: force three failures on the same conversation inside 15 minutes → exactly **one** notification per recipient (R4). Then wait out the bucket and fail again → a second notification appears.

## Scenario 5 — Tenant isolation and per-user isolation (SC-003)

1. Seed notifications in tenants A and B for a user who belongs to both.
2. With `X-Tenant-ID: A`, list and count → only A's notifications; switch to B → only B's. Neither count includes the other.
3. As Agent B, `POST /api/v1/tenant/notifications/{id}/read` using **Agent A's** notification id → **404** (not 403 — see contract note on id probing).
4. Direct DB check: every row has a non-null `tenant_id`.

## Scenario 6 — Dedup and replay (SC-007 / FR-010)

1. Note a notification's `dedupe_key` in the DB.
2. Re-insert the same `notification.requested` outbox event (or restart the worker mid-batch to force a redelivery).
3. Row count for that `(recipient_membership_id, dedupe_key)` stays at 1 — the unique index absorbs it, and the worker logs no error.

## Scenario 7 — Degraded and edge paths (Edge Cases / SC-005)

1. **Missing subject**: delete the conversation behind a notification, then click it → an "unavailable" state, never an error page.
2. **Removed member**: deactivate a membership with unread rows → their notifications are unreachable; re-adding does not resurface the old count.
3. **Non-blocking**: stop the notifications worker, then assign a conversation → the assignment **still succeeds** (FR-017). Restart the worker → the queued event drains and the notification appears.
4. **Retention**: back-date rows past 90 days and run the sweeper → they are deleted.

---

## What "done" looks like

All seven scenarios pass, both automated suites are green, and the topbar's old ephemeral counter is gone — `grep -r inAppSignal frontend/` returns nothing (R8). A lingering `inAppSignal` means the badge is double-counting escalations.
