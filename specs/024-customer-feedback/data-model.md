# Data Model: Customer Feedback (024)

Migration: `backend/migrations/0051_customer_feedback.sql`

## Table: `conversation_feedback`

Append-only, immutable, tenant-scoped analytics fact. One row per conversation, ever. No `UPDATE`/`DELETE` paths exist in application code; no `deleted_at` (justified in plan.md Complexity Tracking).

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| `id` | UUID | PK, `DEFAULT gen_random_uuid()` | |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` ON DELETE RESTRICT | Principle II |
| `conversation_id` | UUID | NOT NULL; composite FK `(tenant_id, conversation_id)` → `conversations(tenant_id, id)` ON DELETE RESTRICT | Uses existing `conversations_tenant_id_id_uq` (0037); RESTRICT preserves feedback per FR-012 |
| `widget_session_id` | UUID | NULL, FK → `widget_sessions(id)` ON DELETE SET NULL | Provenance; survives session purging |
| `channel` | TEXT | NOT NULL, CHECK same domain as `conversations_channel_check` | Snapshot at submission (R3) |
| `agent_configuration_id` | UUID | NULL, FK → `agent_configurations(id)` ON DELETE RESTRICT | Tenant's live AI config if conversation had AI participation (R4), else NULL |
| `assigned_membership_id` | UUID | NULL; composite FK `(tenant_id, assigned_membership_id)` → `tenant_memberships(tenant_id, id)` | Assignee at submission time (mirrors `conversations` 0033 pattern), else NULL |
| `rating` | SMALLINT | NOT NULL, CHECK `rating BETWEEN 1 AND 5` | FR-001 |
| `comment` | TEXT | NULL, CHECK `char_length(comment) <= 2000` | FR-002; trimmed, never empty string (NULL instead) |
| `submitted_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` | Analytics time dimension |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT `now()` | Repo convention |

No `updated_at` / `set_updated_at` trigger: rows are never updated (FR-013).

### Indexes

| Index | Definition | Serves |
|-------|-----------|--------|
| `conversation_feedback_conversation_uq` | UNIQUE `(tenant_id, conversation_id)` | Duplicate prevention (FR-003, R2); also the detail/list join path |
| `conversation_feedback_tenant_time_idx` | `(tenant_id, submitted_at DESC)` | Summary + future time-window analytics |
| `conversation_feedback_tenant_agent_idx` | `(tenant_id, agent_configuration_id)` WHERE `agent_configuration_id IS NOT NULL` | Future per-AI-agent aggregation (SC-006) |
| `conversation_feedback_tenant_member_idx` | `(tenant_id, assigned_membership_id)` WHERE `assigned_membership_id IS NOT NULL` | Future per-human-agent aggregation (SC-006) |

## Rust types (module `feedback`)

- `ConversationFeedbackRow` — full DB row (sqlx `FromRow`).
- `SubmitFeedbackPayload` — `{ rating: i16, comment: Option<String> }`; deserialization + handler validation (1–5, ≤ 2,000 chars after trim).
- `WidgetFeedbackDto` — **camelCase** (`#[serde(rename_all = "camelCase")]`): `{ rating, comment, submittedAt }`. Returned by the public POST.
- `PendingFeedbackDto` — **camelCase**: `{ conversationId, endedAt }`. Returned by `GET /widget/v1/feedback/pending`.
- `TenantFeedbackDto` — **snake_case** (no `rename_all`): `{ rating, comment, submitted_at }`. Embedded in tenant conversation detail.
- `FeedbackSummaryDto` — **snake_case**: `{ average_rating: Option<f64>, feedback_count: i64 }`; `average_rating` is `null` when count is 0 (US5 empty state — never a fake 0.0).
- `SubmitFeedbackPayload` — request body, camelCase in, `{ rating: i16, comment: Option<String> }`.

## Extensions to existing types

- `conversations::model::ConversationDetail` → add `feedback: Option<TenantFeedbackDto>` (single LEFT JOIN in `detail_query_in_tx`; also add `feedback_rating` / `feedback_comment` / `feedback_submitted_at` to the private `DetailRow`).
- `conversations::model::Conversation` (list row) → add `rating: Option<i16>` (LEFT JOIN in the inbox list query; also add `feedback_rating` to `InboxRow`) — no N+1, Principle X.
- `widgets::model::WidgetConversationDto` → **unchanged**. Feedback is not embedded in `GET /widget/v1/conversation`, because that endpoint filters out ended conversations (see research.md R5).

## State & lifecycle

```
(no feedback)  --valid submit-->  (feedback exists, immutable)
```

- Submit preconditions: authenticated widget session owns the conversation (`conversations.tenant_id = session.tenant_id AND conversations.customer_id = session.customer_id`), conversation `status IN ('resolved','closed')`, no existing feedback row.
- Concurrent submits: `INSERT ... ON CONFLICT DO NOTHING` + read-back → exactly one row; both callers observe success (R2).
- No other transitions. Conversation archival/deletion does not cascade (RESTRICT + FR-012).

## Widget client state (not persisted server-side)

Per-conversation prompt state derived in `widget.store.ts` (R7), driven by `GET /widget/v1/feedback/pending`:

```
pending returns a conversation + not dismissed   → active prompt
pending returns a conversation + dismissed       → passive "rate this conversation" entry point
submit succeeded                                 → thank-you state (conversation stops being pending)
pending returns null                             → nothing
```

Dismissal flag lives in `localStorage` keyed by conversation id (key `hx_widget_feedback_dismissed_<conversationId>`), alongside the existing `hx_widget_session_<widgetId>` key in `session.store.ts`.
