# Quickstart Validation: Customer Feedback (024)

Runnable scenarios proving the feature end-to-end. Contracts: [contracts/feedback-api.md](./contracts/feedback-api.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL running with migrations applied through `0051_customer_feedback.sql` (see `backend/migrations/README.md` for the migration workflow).
- Backend server running from `backend/` (existing dev setup).
- A widget instance with an allowed origin, and its `widgetId` (create via dashboard Widgets settings or admin API).

## 1. Backend test suites

```bash
cd backend
cargo test -p feedback            # module unit tests (validation, payloads)
cargo test -p server              # integration tests incl. feedback API surface
```

Expected: all green, including new tests for submit/duplicate/isolation/summary.

## 2. Widget submission flow (curl)

```bash
# 1. Create a widget session (public)
curl -s -X POST 'http://localhost:8080/widget/v1/sessions' \
  -H 'Origin: http://allowed-origin.example' -H 'Content-Type: application/json' \
  -d '{"widgetId": "<WIDGET_PUBLIC_ID>"}'
# → capture sessionToken

# 2. Create a conversation, send a message, then END it from the tenant side
#    (dashboard, or PATCH /tenant/conversations/{id} with status "resolved" or "closed")

# 3. Confirm the conversation is now pending feedback
curl -s 'http://localhost:8080/widget/v1/feedback/pending' \
  -H 'Authorization: Bearer <SESSION_TOKEN>' -H 'Origin: http://allowed-origin.example'
# → {"data":{"conversationId":"<CONV_ID>","endedAt":"..."}}

# 4. Submit feedback
curl -s -X POST 'http://localhost:8080/widget/v1/conversations/<CONV_ID>/feedback' \
  -H 'Authorization: Bearer <SESSION_TOKEN>' \
  -H 'Origin: http://allowed-origin.example' -H 'Content-Type: application/json' \
  -d '{"rating": 4, "comment": "Great help"}'
```

Expected outcomes:

- First submit → `201` with the feedback object.
- Repeat the same submit → `200` with the **same** record; DB has exactly one row (`SELECT count(*) FROM conversation_feedback WHERE conversation_id = '<CONV_ID>'` → 1).
- After submitting, step 3 returns `{"data":null}` — the conversation is no longer pending (this is what prevents re-prompting).
- `"rating": 6` → `422`; comment > 2,000 chars → `422` with an explicit message (no truncation).
- Submit while the conversation is still `open` → `422` (`conversation_not_ended`).
- Submit with another session's token → `404`.
- `GET /widget/v1/conversation` (singular) is unchanged and still returns `{"data":null}` for the ended conversation.

## 3. Tenant surfaces (dashboard)

1. Sign in, open the conversation from scenario 2 in **Conversations → detail**: rating stars, comment, and submission time visible; satisfaction badge in the header; a conversation without feedback shows the explicit "No rating" state.
2. Inbox list: the rated conversation's row shows the satisfaction badge; unrated rows show none.
3. Conversations page summary card: average + count match `GET /tenant/feedback/summary`; on a tenant with zero feedback the card shows the empty state (no fake 0.0).
4. Tenant isolation: switch tenant context (topbar switcher) → other tenant sees neither the feedback nor the first tenant's summary numbers.

## 4. Widget UI flow

Serve the widget against an ended conversation (existing widget dev setup in `frontend/apps/widget`):

1. End the conversation from the dashboard, then reopen the widget launcher → the feedback prompt appears with 5 stars + optional comment box. (Also verify the other trigger: with the widget already open, end the conversation from the dashboard and try to send a message — the `409` response surfaces the prompt.)
2. Submit → thank-you state; reloading the widget shows no prompt at all, never a re-prompt.
3. Dismiss instead → prompt collapses; a passive "rate this conversation" affordance remains and reopens the form; it survives a reload (dismissal is per-conversation `localStorage`).
4. Frontend tests:

```bash
cd frontend
npx nx test widget      # or the repo's configured test runner for apps/widget
npx nx test dashboard
```

## 5. Success-criteria spot checks

- **SC-003** (no duplicates): step 2's double-submit check.
- **SC-004** (tenant isolation): step 3.4 plus the server integration test asserting cross-tenant 404.
- **SC-002/SC-006** (attribution): after an escalated-then-closed conversation, verify the row: `SELECT channel, agent_configuration_id, assigned_membership_id FROM conversation_feedback WHERE conversation_id = '...'` — channel always set; AI config set when AI replied; membership set when a human was assigned at close.
