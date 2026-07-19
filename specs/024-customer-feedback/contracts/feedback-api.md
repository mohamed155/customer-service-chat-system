# API Contracts: Customer Feedback (024)

**Casing rule (verified against the codebase — do not deviate):**

- `/widget/v1/**` responses are **camelCase** — widget DTOs use `#[serde(rename_all = "camelCase")]` (see `widgets::model::WidgetConversationDto`).
- `/tenant/**` responses are **snake_case** — `conversations::model` structs have no `rename_all`, and the dashboard's `ConversationWire` reads `last_activity_at`, `display_name`.

Public endpoints live in the `/widget/v1` group (widget CORS layer, origin allowlist, in-process rate limiting, `Authorization: Bearer <session token>` via `widgets::session::authenticate_session`). Tenant endpoints live in the `/tenant` group (JWT + tenant context + RBAC).

**Ownership rule for all public endpoints**: a session owns a conversation when `conversations.tenant_id = session.tenant_id AND conversations.customer_id = session.customer_id`. Each widget session lazily creates exactly one customer (`widgets::queries::ensure_customer_for_session`), so this is also the per-session duplicate boundary.

**"Ended" means `status IN ('resolved','closed')`** — conversation statuses are `open | pending | resolved | closed` (migration 0033).

---

## 1. Submit feedback (public, widget) — NEW

`POST /widget/v1/conversations/{conversationId}/feedback`

Request body (camelCase):

```json
{ "rating": 4, "comment": "Quick and helpful, thanks!" }
```

- `rating`: integer 1–5, required.
- `comment`: string, optional; trimmed; ≤ 2,000 chars after trim; empty/whitespace-only → stored as SQL NULL.

Responses:

| Status | When | Body |
|--------|------|------|
| 201 | Feedback created | `{ "data": { "feedback": WidgetFeedbackDto } }` |
| 200 | Feedback already existed (duplicate / concurrent / retried submit — idempotent success) | `{ "data": { "feedback": WidgetFeedbackDto } }` (the existing record) |
| 401 | Missing/invalid/expired session token | `ErrorEnvelope` (`session_invalid`) |
| 404 | Conversation not found, or not owned by this session | `ErrorEnvelope` |
| 422 | Rating outside 1–5, comment > 2,000 chars (explicit message — never truncate), or conversation not ended | `ErrorEnvelope` (`validation_failed` / `conversation_not_ended`) |
| 429 | Rate limited | `ErrorEnvelope` (`rate_limited`) |

`WidgetFeedbackDto` (camelCase):

```json
{ "rating": 4, "comment": "Quick and helpful, thanks!", "submittedAt": "2026-07-19T12:34:56Z" }
```

Response envelope shape mirrors `WidgetMessageResponse` / `WidgetMessageResponseData` in `widgets::model`.

## 2. Pending feedback lookup (public, widget) — NEW

`GET /widget/v1/feedback/pending`

No path/query params — the server resolves session → `customer_id` → most recent ended conversation with no feedback row.

Responses:

| Status | When | Body |
|--------|------|------|
| 200 | A conversation is awaiting feedback | `{ "data": { "conversationId": "…", "endedAt": "2026-07-19T12:30:00Z" } }` |
| 200 | Nothing pending (no ended conversation, or it already has feedback) | `{ "data": null }` |
| 401 | Session invalid | `ErrorEnvelope` (`session_invalid`) |

Selection SQL (authoritative):

```sql
SELECT c.id, c.last_activity_at
FROM conversations c
LEFT JOIN conversation_feedback f
  ON f.conversation_id = c.id AND f.tenant_id = c.tenant_id
WHERE c.tenant_id = $1
  AND c.customer_id = $2
  AND c.status IN ('resolved', 'closed')
  AND c.deleted_at IS NULL
  AND f.id IS NULL
ORDER BY c.last_activity_at DESC
LIMIT 1
```

A session whose `customer_id` is NULL always yields `{ "data": null }` (it never had a conversation).

**Explicitly unchanged**: `GET /widget/v1/conversation` (singular) keeps its current behavior and response shape — it still filters out ended conversations and gains no `feedback` field. No SSE event types are added or modified.

## 3. Conversation detail with feedback (tenant) — EXTENSION

`GET /tenant/conversations/{id}` — `ConversationDetail` gains a nullable `feedback` object (snake_case):

```json
{
  "...existing fields": "...",
  "feedback": { "rating": 4, "comment": "Quick and helpful, thanks!", "submitted_at": "2026-07-19T12:34:56Z" }
}
```

`null` when no feedback exists → UI renders the explicit "No rating" state (FR-008). Sourced from a `LEFT JOIN conversation_feedback` in `conversations::queries::detail_query_in_tx` — no extra query.

## 4. Conversation list rating (tenant) — EXTENSION

`GET /tenant/conversations` — each row of `Conversation` gains:

```json
{ "...existing fields": "...", "rating": 4 }
```

`rating` is an integer 1–5 or `null`. Sourced from a `LEFT JOIN conversation_feedback` added to the inbox list query in `conversations::queries` (same query — no N+1, Principle X).

## 5. Tenant feedback summary (tenant) — NEW

`GET /tenant/feedback/summary`

| Status | When | Body |
|--------|------|------|
| 200 | Always (including a tenant with no feedback) | `{ "data": { "average_rating": 4.2, "feedback_count": 57 } }` |
| 401 / 403 | Unauthenticated / no tenant access | `ErrorEnvelope` |

- `average_rating`: number rounded to 1 decimal, **`null` when `feedback_count` is 0** (US5 empty state — never a fake `0.0`).
- Scoped to the active tenant context (`X-Tenant-ID`). Per-agent / per-channel / time-series breakdowns are intentionally not offered (FR-015).

---

## Non-goals (enforced absences)

- No `PUT` / `PATCH` / `DELETE` on feedback anywhere — immutability (FR-013) is contract-level, not just UI-level.
- No platform-level (cross-tenant) feedback endpoint.
- No dismissal-tracking endpoint — dismissal is client-side `localStorage` state (R7).
- No new SSE events, and no change to `GET /widget/v1/conversation`.
