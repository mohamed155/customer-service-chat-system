# Tasks: Customer Feedback

**Input**: Design documents from `/specs/024-customer-feedback/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/feedback-api.md](./contracts/feedback-api.md)

**Tests**: Included — Constitution VII (Test-First & Regression Discipline) makes unit/integration/API coverage mandatory for shipped functionality.

**Organization**: Tasks are grouped by user story so each story can be implemented, tested, and demoed independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4, US5)
- Every task names the exact file to create or edit

---

## ⚠️ READ THIS BEFORE STARTING ANY TASK

These facts were verified against the codebase. Getting one wrong will produce broken code.

1. **Wire casing differs by surface.**
   - `/widget/v1/**` DTOs use `#[serde(rename_all = "camelCase")]` → `submittedAt`, `conversationId`.
   - `/tenant/**` DTOs have **no** `rename_all` → `submitted_at`, `average_rating`. Copy this exactly; the dashboard's `ConversationWire` reads snake_case.
2. **"Ended" means `status IN ('resolved','closed')`.** Conversation statuses are `open | pending | resolved | closed`. Never check for `'closed'` alone.
3. **There is no `GET /widget/v1/conversations/{id}`.** The real endpoint is `GET /widget/v1/conversation` (singular). **Do not modify it.**
4. **There is no SSE "conversation closed" event.** Do not add one. The `handling === 'closed'` branch in `widget.store.ts` is pre-existing dead code — leave it alone.
5. **Session owns a conversation when** `conversations.tenant_id = session.tenant_id AND conversations.customer_id = session.customer_id`. Each widget session creates exactly one customer.
6. **Pre-existing bugs — do NOT fix these, they are out of scope:**
   - `widget-api.service.ts::getConversation()` expects `{data:{conversation}}` but the server sends `{data:{...}}`. Ignore it; nothing in this feature depends on it.
   - The unreachable `handling === 'closed'` branch in `widget.store.ts`.
   - (The `handleClosedConversation()` method **is** in scope — T017 wires it up as the feedback trigger.)
7. **Root `backend/Cargo.toml` needs no edit** — workspace members use the glob `crates/modules/*`.
8. **Never run migrations by hand against a database.** Add the file only (Constitution VIII).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the new `feedback` module crate so later phases have somewhere to put code.

- [X] T001 Create `backend/crates/modules/feedback/Cargo.toml` with package name `feedback`, `version.workspace = true`, `edition.workspace = true`, and these dependencies (copy the style of `backend/crates/modules/widgets/Cargo.toml`): `axum.workspace = true`, `chrono = { workspace = true, features = ["serde"] }`, `kernel = { path = "../../shared/kernel" }`, `serde.workspace = true`, `sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }`, `tenancy = { path = "../tenancy" }`, `tracing.workspace = true`, `utoipa.workspace = true`, `uuid.workspace = true`, `widgets = { path = "../widgets" }`

  **Do NOT add `conversations = { path = "../conversations" }`.** The `feedback` crate reads the conversations table with raw SQL and needs none of its Rust types. Adding it would create the cycle `conversations → feedback → widgets → conversations` in Phase 5, which Cargo rejects.

- [X] T002 Create `backend/crates/modules/feedback/src/lib.rs` containing only module declarations plus a `//!` doc comment. Declarations: `pub mod model;`, `pub mod public_routes;`, `pub mod queries;`, `pub mod tenant_routes;`. The doc comment must state Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, and Extension Points (Constitution: Documentation & Future Readiness).

- [X] T003 Add `feedback = { path = "../modules/feedback" }` to the `[dependencies]` of `backend/crates/server/Cargo.toml`, keeping the existing alphabetical ordering (it goes just before `flags`/after `escalations`).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Database schema, shared types, and shared queries used by every user story.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 Create migration `backend/migrations/0051_customer_feedback.sql` with exactly this content:

  ```sql
  -- Migration 0051: Customer feedback (spec 024).
  --
  -- Part 1 fixes a pre-existing defect that blocks this feature: migration 0050
  -- added widget conversations, and widgets/public_routes.rs inserts them with
  -- channel = 'widget', but 0026's CHECK never listed 'widget', so every widget
  -- conversation INSERT violates the constraint.
  ALTER TABLE conversations
      DROP CONSTRAINT IF EXISTS conversations_channel_check,
      ADD CONSTRAINT conversations_channel_check
          CHECK (channel IN ('email', 'phone', 'web_chat', 'whatsapp', 'telegram', 'widget'));

  -- Part 2: append-only, immutable feedback fact table. No updated_at trigger
  -- and no deleted_at: rows are never updated or deleted (FR-012, FR-013).
  CREATE TABLE conversation_feedback (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
      conversation_id UUID NOT NULL,
      widget_session_id UUID NULL REFERENCES widget_sessions(id) ON DELETE SET NULL,
      channel TEXT NOT NULL,
      agent_configuration_id UUID NULL REFERENCES agent_configurations(id) ON DELETE RESTRICT,
      assigned_membership_id UUID NULL,
      rating SMALLINT NOT NULL,
      comment TEXT NULL,
      submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      CONSTRAINT conversation_feedback_rating_check
          CHECK (rating BETWEEN 1 AND 5),
      CONSTRAINT conversation_feedback_comment_len_check
          CHECK (comment IS NULL OR char_length(comment) <= 2000),
      CONSTRAINT conversation_feedback_conversation_fkey
          FOREIGN KEY (tenant_id, conversation_id)
          REFERENCES conversations (tenant_id, id) ON DELETE RESTRICT,
      CONSTRAINT conversation_feedback_membership_fkey
          FOREIGN KEY (tenant_id, assigned_membership_id)
          REFERENCES tenant_memberships (tenant_id, id)
  );

  -- Duplicate prevention (FR-003): one feedback row per conversation, ever.
  CREATE UNIQUE INDEX conversation_feedback_conversation_uq
      ON conversation_feedback (tenant_id, conversation_id);

  CREATE INDEX conversation_feedback_tenant_time_idx
      ON conversation_feedback (tenant_id, submitted_at DESC);

  CREATE INDEX conversation_feedback_tenant_agent_idx
      ON conversation_feedback (tenant_id, agent_configuration_id)
      WHERE agent_configuration_id IS NOT NULL;

  CREATE INDEX conversation_feedback_tenant_member_idx
      ON conversation_feedback (tenant_id, assigned_membership_id)
      WHERE assigned_membership_id IS NOT NULL;
  ```

- [X] T050 Add a regression test for the T004 Part 1 defect fix, in `backend/crates/server/tests/feedback_api.rs`: assert that `POST /widget/v1/conversations` succeeds and persists a conversation row with `channel = 'widget'`. This test MUST fail before migration 0051 Part 1 is applied (CHECK constraint violation on `conversations_channel_check`) and pass after. Required by Constitution VII — every bug fix introduces a regression test that fails before the fix and passes after. Run it against the pre-migration schema once to confirm it genuinely fails.

- [X] T005 Create `backend/crates/modules/feedback/src/model.rs` with these types (mirror the derive style of `backend/crates/modules/widgets/src/model.rs`):
  - `ConversationFeedbackRow` — `#[derive(Debug, Clone, sqlx::FromRow)]`, fields matching every column in T004's table (`id`, `tenant_id`, `conversation_id`, `widget_session_id: Option<Uuid>`, `channel: String`, `agent_configuration_id: Option<Uuid>`, `assigned_membership_id: Option<Uuid>`, `rating: i16`, `comment: Option<String>`, `submitted_at`, `created_at`).
  - `SubmitFeedbackPayload` — `#[derive(Debug, Deserialize, ToSchema)]` + `#[serde(rename_all = "camelCase")]`; fields `rating: i16`, `comment: Option<String>`.
  - `WidgetFeedbackDto` — Serialize/Deserialize/ToSchema + **camelCase**; fields `rating: i16`, `comment: Option<String>`, `submitted_at: DateTime<Utc>`.
  - `WidgetFeedbackResponse { data: WidgetFeedbackResponseData }` and `WidgetFeedbackResponseData { feedback: WidgetFeedbackDto }` — camelCase (mirrors `WidgetMessageResponse`/`WidgetMessageResponseData`).
  - `PendingFeedbackDto` — camelCase; `conversation_id: Uuid`, `ended_at: DateTime<Utc>`.
  - `PendingFeedbackResponse { data: Option<PendingFeedbackDto> }` — camelCase.
  - `FeedbackSummaryDto` — **NO `rename_all`** (snake_case wire); `average_rating: Option<f64>`, `feedback_count: i64`.

  **Note**: `TenantFeedbackDto` is deliberately **not** defined here — it lives in `conversations::model` (T026) to keep the crate graph acyclic.

- [X] T006 Create `backend/crates/modules/feedback/src/queries.rs` with these functions (all tenant-scoped; copy the `sqlx::query_as` style of `backend/crates/modules/widgets/src/queries.rs`):
  - `find_conversation_for_session(pool, tenant_id, customer_id, conversation_id) -> sqlx::Result<Option<(String, String, Option<Uuid>)>>` — returns `(status, channel, assigned_membership_id)` via `SELECT status, channel, assigned_membership_id FROM conversations WHERE tenant_id = $1 AND customer_id = $2 AND id = $3 AND deleted_at IS NULL`. This is the ownership check.
  - `insert_feedback(pool, tenant_id, conversation_id, widget_session_id, channel, agent_configuration_id, assigned_membership_id, rating, comment) -> sqlx::Result<Option<ConversationFeedbackRow>>` — `INSERT INTO conversation_feedback (...) VALUES ($1..$8) ON CONFLICT (tenant_id, conversation_id) DO NOTHING RETURNING <all columns>` with `.fetch_optional(...)`. Returns `None` when a row already existed.
  - `find_feedback_by_conversation(pool, tenant_id, conversation_id) -> sqlx::Result<Option<ConversationFeedbackRow>>`.
  - `find_pending_feedback(pool, tenant_id, customer_id) -> sqlx::Result<Option<(Uuid, DateTime<Utc>)>>` — use the authoritative SQL from `contracts/feedback-api.md` §2 verbatim.
  - `feedback_summary(pool, tenant_id) -> sqlx::Result<(Option<f64>, i64)>` — `SELECT AVG(rating)::float8, COUNT(*)::bigint FROM conversation_feedback WHERE tenant_id = $1`. `AVG` is NULL when there are no rows, which is exactly the US5 empty state.

- [X] T007 Register the new schemas in `backend/crates/server/src/openapi.rs`: add `feedback::model::SubmitFeedbackPayload`, `WidgetFeedbackDto`, `WidgetFeedbackResponse`, `WidgetFeedbackResponseData`, `PendingFeedbackDto`, `PendingFeedbackResponse`, and `FeedbackSummaryDto` to the `components(schemas(...))` list, next to the existing `widgets::model::*` entries. (`conversations::model::TenantFeedbackDto` gets registered in T026.)

**Checkpoint**: `cargo build` succeeds from `backend/`. User story implementation can now begin.

---

## Phase 3: User Story 1 - Customer rates a finished conversation (Priority: P1) 🎯 MVP

**Goal**: A customer whose widget conversation has ended can submit a 1–5 star rating exactly once, and the widget confirms it.

**Independent Test**: End a widget conversation from the dashboard, reopen the widget → prompt appears → pick 4 stars → submit → thank-you state. Submitting twice creates only one database row.

### Tests for User Story 1

> Write these first and confirm they FAIL before writing the implementation.

- [X] T008 [US1] Create `backend/crates/server/tests/feedback_api.rs` with integration tests covering: (a) submit on an ended conversation returns 201 and persists one row; (b) submitting the same conversation twice returns 200 the second time with the same feedback and leaves exactly one row; (c) `rating: 0` and `rating: 6` return 422; (d) submitting for a conversation owned by a different session returns 404; (e) submitting for a conversation with status `open` returns 422; (f) `GET /widget/v1/feedback/pending` returns the ended unrated conversation, then returns `data: null` after feedback is submitted. Follow the setup/helper style of the existing widget tests in `backend/crates/server/tests/`.

### Implementation for User Story 1

- [X] T009 [US1] Create `backend/crates/modules/feedback/src/public_routes.rs` with the `submit_feedback` handler for `POST /widget/v1/conversations/{conversationId}/feedback`. Copy the handler shape of `widgets::public_routes::send_message`: `State(pool)`, `Extension(store): Extension<Arc<InMemoryRateLimitStore>>`, `Extension(headers)`, `Path(conversation_id)`, `ApiJson(payload)`; authenticate via `widgets::session::authenticate_session`; apply the same two rate-limit checks (`session:{id}` 10/60s, `tenant:{id}` 600/60s). Then: reject `rating` outside 1–5 with `ApiError::unprocessable_entity`; look up the conversation with `queries::find_conversation_for_session` (404 `ApiError::not_found` if `None`); return 422 with code `conversation_not_ended` unless status is `resolved` or `closed`; call `queries::insert_feedback` passing the conversation's `channel`, `Some(session.id)` as `widget_session_id`, and `None` for both `agent_configuration_id` and `assigned_membership_id` (US4 fills these in). Respond `201` with `WidgetFeedbackResponse` when the insert returned `Some`; when it returned `None`, re-read with `find_feedback_by_conversation` and respond `200` with the existing record. Add the `#[utoipa::path(...)]` attribute with `security(())`, matching the response table in `contracts/feedback-api.md` §1.

  **Comment handling — do this here in T009, not in T022**, so US1 is correct when shipped alone: trim `payload.comment`, treat an empty or whitespace-only result as `None`, and return `ApiError::unprocessable_entity("Comment must be 2000 characters or fewer")` when the trimmed length exceeds 2000. Never truncate. Without this, an over-length comment reaches the database CHECK constraint and surfaces as a 500 instead of a 422.

- [X] T010 [US1] In the same file `backend/crates/modules/feedback/src/public_routes.rs`, add the `get_pending_feedback` handler for `GET /widget/v1/feedback/pending`: authenticate the session, return `PendingFeedbackResponse { data: None }` immediately if `session.customer_id` is `None`, otherwise call `queries::find_pending_feedback` and map the result to `PendingFeedbackDto`. Always responds 200. Include the `#[utoipa::path(...)]` attribute with `security(())`.

- [X] T011 [US1] Register both handlers in `backend/crates/server/src/router.rs` inside `fn widget_routes()` by adding `.routes(routes!(feedback::public_routes::submit_feedback))` and `.routes(routes!(feedback::public_routes::get_pending_feedback))` after the existing `send_message` line. Do not touch `get_conversation` or `stream_events`.

- [X] T012 [P] [US1] Add feedback types to `frontend/apps/widget/src/core/models.ts`: `export interface WidgetFeedback { rating: number; comment?: string; submittedAt: string; }` and `export interface PendingFeedback { conversationId: string; endedAt: string; }`. Do not modify the existing `WidgetConversation` interface.

- [X] T013 [P] [US1] Create `frontend/apps/widget/src/components/star-rating.component.ts` — a standalone, `OnPush`, signal-based component (match the style of `frontend/apps/widget/src/components/composer.component.ts`). It exposes `value = input<number>(0)`, `readonly = input<boolean>(false)`, and `rate = output<number>()`, renders 5 keyboard-accessible star buttons (`role="radiogroup"`, each star `aria-label="N stars"`), and emits the chosen 1–5 value on click. Style with existing `--wgt-*` CSS tokens only. No imports from `libs/*`, Taiga UI, or NgRx.

- [X] T014 [US1] Add two methods to `frontend/apps/widget/src/core/widget-api.service.ts`, following the existing `sendMessage` pattern (headers via `this.headers(token)`, `catchError` → `this.mapError`): `getPendingFeedback(token)` → `GET ${this.base}/widget/v1/feedback/pending`, mapping to `PendingFeedback | null` from `{ data }`; and `submitFeedback(token, conversationId, rating, comment?)` → `POST ${this.base}/widget/v1/conversations/${conversationId}/feedback`, mapping to `WidgetFeedback` from `r.data.feedback`.

- [X] T015 [P] [US1] Create `frontend/apps/widget/src/core/feedback-dismissal.store.ts` — an `@Injectable({ providedIn: 'root' })` store with `isDismissed(conversationId): boolean` and `dismiss(conversationId): void`, persisting to `localStorage` under the key `hx_widget_feedback_dismissed_<conversationId>`. Wrap every `localStorage` call in `try/catch` with an in-memory `Set` fallback, exactly as `frontend/apps/widget/src/core/session.store.ts` does.

- [X] T016 [US1] Create `frontend/apps/widget/src/components/feedback-prompt.component.ts` — standalone, `OnPush`, importing `StarRatingComponent`. Inputs: `state = input.required<'prompt' | 'collapsed' | 'submitted'>()` and `feedback = input<WidgetFeedback | null>(null)`. Outputs: `submitRating = output<number>()` and `dismiss = output<void>()`. Renders: `prompt` → "How did we do?" heading, the star rating, and a dismiss ("Not now") button; `collapsed` → a single "Rate this conversation" text button; `submitted` → a thank-you message with the submitted rating shown read-only. Use `--wgt-*` tokens only. **Leave a `<!-- comment box added by T023 -->` placeholder** — the comment field belongs to US2.

- [X] T017 [US1] Extend `frontend/apps/widget/src/core/widget.store.ts` with feedback state and the two triggers. Add private signals `pendingFeedbackSignal = signal<PendingFeedback | null>(null)` and `feedbackSignal = signal<WidgetFeedback | null>(null)` with readonly accessors, plus a `feedbackState` computed returning `'prompt' | 'collapsed' | 'submitted' | 'none'` per the table in `data-model.md` ("Widget client state"), consulting the injected `FeedbackDismissalStore`. Add `checkPendingFeedback()` calling `api.getPendingFeedback` and setting the signal, and `submitFeedback(rating, comment?)` calling `api.submitFeedback` then setting `feedbackSignal` and clearing `pendingFeedbackSignal`. Add `dismissFeedback()` which records the dismissal in the store. **Trigger (a)**: call `checkPendingFeedback()` at the start of `open()`. **Trigger (b)**: in `sendMessage`'s `error` callback, when the error is an `HttpErrorResponse` with `status === 409`, call the existing `handleClosedConversation()` and then `checkPendingFeedback()`. Use RxJS operator composition — no `async/await` or `.then()` (Constitution: RxJS-first).

- [X] T018 [US1] Render `<wgt-feedback-prompt>` in `frontend/apps/widget/src/components/chat-window.component.ts` below the message list, shown with `@if (store.feedbackState() !== 'none')`, bound to the store's `feedbackState()` and `feedback()`, wiring `(submitRating)` → `store.submitFeedback($event)` and `(dismiss)` → `store.dismissFeedback()`. When `feedbackState()` is `'prompt'` or `'submitted'`, hide or disable the composer so the customer cannot type into an ended conversation.

- [X] T019 [P] [US1] Create `frontend/apps/widget/src/components/star-rating.component.spec.ts` verifying that clicking the 3rd star emits `3`, that `readonly` blocks emission, and that each star has an accessible label.

- [X] T020 [P] [US1] Extend `frontend/apps/widget/src/core/widget.store.spec.ts` with tests for `feedbackState()` across all four cases (prompt / collapsed after dismissal / submitted / none) and for the 409 path calling `checkPendingFeedback()`. Mock `WidgetApiService` in the existing style of that file.

**Checkpoint**: US1 is fully functional — a customer can submit a rating exactly once and the widget confirms it.

---

## Phase 4: User Story 2 - Customer adds an optional comment (Priority: P2)

**Goal**: The customer can optionally attach a free-text comment (≤ 2,000 characters) to their rating.

**Independent Test**: Submit a rating with a comment → both persist. Submit a rating with an empty comment → succeeds with `comment` NULL. Submit a 2,001-character comment → rejected with a clear message, nothing truncated.

### Tests for User Story 2

- [X] T021 [US2] Add tests to `backend/crates/server/tests/feedback_api.rs`: a comment of exactly 2,000 characters is accepted; 2,001 characters returns 422 and persists no row; a whitespace-only comment is stored as SQL NULL; and a submitted comment round-trips unchanged.

### Implementation for User Story 2

- [X] T022 [US2] Verify that the comment normalization and 2,000-character validation implemented in T009 satisfy US2's backend contract, using T021's tests. **No new validation logic should be needed** — if T021 reveals a gap (for example a multi-byte character counted by bytes rather than characters — use `chars().count()`, not `len()`), fix it in `backend/crates/modules/feedback/src/public_routes.rs`. Otherwise mark this task complete with a note that T009 already covers it.

- [X] T023 [US2] Replace the `<!-- comment box added by T023 -->` placeholder in `frontend/apps/widget/src/components/feedback-prompt.component.ts` with an optional `<textarea>` (max 2,000 chars) plus a live character counter that turns red past the limit, and disable the submit button while over the limit. Change the `submitRating` output to `submitFeedback = output<{ rating: number; comment?: string }>()` and update the `submitted` state to display the comment when present.

- [X] T024 [US2] Update the `(submitRating)` binding in `frontend/apps/widget/src/components/chat-window.component.ts` to the new `(submitFeedback)` output shape, and extend `frontend/apps/widget/src/core/widget.store.spec.ts` with a test asserting the comment is forwarded to `WidgetApiService.submitFeedback`.

**Checkpoint**: US1 and US2 both work — rating alone, or rating plus comment.

---

## Phase 5: User Story 3 - Team reviews feedback on a conversation (Priority: P2)

**Goal**: Dashboard users see the rating, comment, and submission time on the conversation detail, and a satisfaction badge on detail and inbox rows.

**Independent Test**: Submit feedback on a conversation, open it in the dashboard → rating, comment, time, and badge visible. A conversation without feedback shows "No rating". Another tenant sees neither.

### Blocking prerequisite for User Story 3

- [X] T051 [US3] **Fix tenant isolation in `inbox_query`** in `backend/crates/modules/conversations/src/queries.rs` — this blocks T028. The base SQL string currently ends at `) preview ON TRUE` with **no `WHERE` clause**, so every appended filter (`" AND c.status = $2"`, channel, cursor) attaches to the LATERAL join's `ON` condition, and `$1` — bound to `tenant_id` at the `sqlx::query_as(...).bind(tenant_id)` call — is never referenced. Append ` WHERE c.tenant_id = $1 AND c.deleted_at IS NULL` to the end of that base string:

  ```diff
                LIMIT 1 \
  -         ) preview ON TRUE",
  +         ) preview ON TRUE \
  +         WHERE c.tenant_id = $1 AND c.deleted_at IS NULL",
        );
  ```

  This one line scopes the query to the tenant (Constitution II), makes `$1` referenced so the bind count matches, re-parents every appended `AND` filter onto the `WHERE` where it belongs, and excludes soft-deleted conversations — matching every other query in that file. Then add a regression test to `backend/crates/server/tests/conversations.rs` creating conversations in two tenants and asserting `GET /tenant/conversations` returns only the acting tenant's rows; confirm it fails before the fix and passes after (Constitution VII).

  **Before starting**: hit `GET /tenant/conversations` against a live database to record the current symptom. If Postgres rejects the unused `$1` the endpoint is returning 500s; if it tolerates it, this is a live cross-tenant data leak that may warrant its own incident handling. The fix is identical either way.

### Tests for User Story 3

- [X] T025 [US3] Add tests to `backend/crates/server/tests/feedback_api.rs`: `GET /tenant/conversations/{id}` returns the `feedback` object (snake_case `submitted_at`) for a rated conversation and `null` for an unrated one; `GET /tenant/conversations` includes `rating` on rated rows and `null` elsewhere; and a user in another tenant receives no feedback data for either endpoint.

### Implementation for User Story 3

- [X] T026 [US3] In `backend/crates/modules/conversations/src/model.rs`, define `TenantFeedbackDto` (Serialize/Deserialize/ToSchema, **no `rename_all`** → snake_case wire; fields `rating: i16`, `comment: Option<String>`, `submitted_at: DateTime<Utc>`), then add `pub feedback: Option<TenantFeedbackDto>` to `ConversationDetail` and `pub rating: Option<i16>` to `Conversation`. Register `conversations::model::TenantFeedbackDto` in the `components(schemas(...))` list in `backend/crates/server/src/openapi.rs`.

  **Do NOT add a `feedback` dependency to `backend/crates/modules/conversations/Cargo.toml`** — that would create the cycle `conversations → feedback → widgets → conversations`. This DTO is defined locally precisely to avoid it, and no `Cargo.toml` change is needed for this task.

- [X] T027 [US3] In `detail_query_in_tx` in `backend/crates/modules/conversations/src/queries.rs`, add `LEFT JOIN conversation_feedback f ON f.conversation_id = cv.id AND f.tenant_id = cv.tenant_id`, select `f.rating AS feedback_rating, f.comment AS feedback_comment, f.submitted_at AS feedback_submitted_at`, add the three matching `Option` fields to the private `DetailRow` struct, and map them into the new `ConversationDetail.feedback` (build `Some(TenantFeedbackDto{..})` only when `feedback_rating` is `Some`).

- [X] T028 [US3] In the inbox list query in `backend/crates/modules/conversations/src/queries.rs` (the `sql` string starting `SELECT c.id, c.customer_id,`), add `f.rating AS feedback_rating` to the select list and `LEFT JOIN conversation_feedback f ON f.conversation_id = c.id AND f.tenant_id = c.tenant_id` after the existing widget_instances join, add `pub feedback_rating: Option<i16>` to `InboxRow`, and map it to `Conversation.rating`. Keep it inside this single query — do not add a per-row lookup (Constitution X forbids N+1).

- [X] T029 [P] [US3] Create `frontend/apps/dashboard/src/app/shared/components/satisfaction-badge/satisfaction-badge.component.ts` — standalone, `OnPush`, selector `app-satisfaction-badge`, `rating = input.required<number>()`. Copy the structure and token usage of `shared/components/status-badge/status-badge.component.ts`: render `★ N`, mapping 4–5 → green tone, 3 → amber, 1–2 → red, using the same `--app-green-soft`/`--app-amber-soft`/`--app-red-soft` variables. Include an `aria-label` such as "Rated 4 out of 5".

- [X] T030 [P] [US3] Create `frontend/apps/dashboard/src/app/shared/components/satisfaction-badge/satisfaction-badge.component.spec.ts` asserting the tone class for ratings 1 through 5 and the accessible label text.

- [X] T031 [US3] In `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`: add `readonly rating?: number | null` to `ConversationWire` and map it to `rating` in `conversationFromWire`; add `readonly feedback?: { readonly rating: number; readonly comment: string | null; readonly submitted_at: string } | null` to `ConversationDetailWire` plus a matching `feedback` field on `ConversationDetail`; and map it in `conversationDetailFromWire` to `{ rating, comment, submittedAt }`. Follow the existing snake_case→camelCase mapper style exactly.

- [X] T032 [US3] In `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.ts`, render a feedback section showing the star rating, the comment when present, the formatted submission time, and `<app-satisfaction-badge>` in the header. When `feedback` is null, render an explicit "No rating" empty state.

- [X] T033 [US3] In `frontend/apps/dashboard/src/app/features/tenant/conversations/inbox-list.component.ts`, render `<app-satisfaction-badge>` on each row when `conversation.rating` is non-null, and render nothing when it is null.

- [X] T034 [US3] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.spec.ts` with tests for the populated feedback view and the "No rating" empty state.

**Checkpoint**: Feedback is visible to the team on both detail and inbox surfaces.

---

## Phase 6: User Story 4 - Feedback is attributable for later analytics (Priority: P3)

**Goal**: Each feedback row snapshots the AI agent and assigned human agent at submission time, so analytics can aggregate by them later.

**Independent Test**: Submit feedback on an AI-only conversation → `agent_configuration_id` set, `assigned_membership_id` NULL. Submit on an escalated, human-assigned conversation → both set.

### Tests for User Story 4

- [X] T035 [US4] Add tests to `backend/crates/server/tests/feedback_api.rs` asserting the stored `channel`, `agent_configuration_id`, and `assigned_membership_id` for three cases: AI-only conversation, human-assigned conversation, and a conversation with neither (both attribution columns NULL, `channel` always set).

### Implementation for User Story 4

- [X] T036 [US4] Add `resolve_ai_agent_configuration(pool, tenant_id, conversation_id) -> sqlx::Result<Option<Uuid>>` to `backend/crates/modules/feedback/src/queries.rs`, implementing research.md R4: return the tenant's live agent configuration id only when the conversation has at least one `ai_generations` row — `SELECT ac.id FROM agent_configurations ac WHERE ac.tenant_id = $1 AND ac.status = 'live' AND ac.deleted_at IS NULL AND EXISTS (SELECT 1 FROM ai_generations g WHERE g.tenant_id = $1 AND g.conversation_id = $2) LIMIT 1`. Check the real column names in `backend/migrations/0041_agent_configurations.sql` first and adjust the live/deleted predicates to match that schema.

- [X] T037 [US4] In `submit_feedback` in `backend/crates/modules/feedback/src/public_routes.rs`, replace the two `None` placeholders from T009: pass the result of `resolve_ai_agent_configuration` as `agent_configuration_id`, and pass the `assigned_membership_id` already returned by `find_conversation_for_session`. Resolve both before the insert so the row is a point-in-time snapshot.

**Checkpoint**: Every new feedback row is fully attributed.

---

## Phase 7: User Story 5 - Manager sees tenant-wide satisfaction (Priority: P3)

**Goal**: A tenant-wide average rating and feedback count are visible in the dashboard.

**Independent Test**: With several rated conversations, the card shows the correct average and count; a tenant with no feedback shows an empty state, not `0.0`.

### Tests for User Story 5

- [X] T038 [US5] Add tests to `backend/crates/server/tests/feedback_api.rs`: `GET /tenant/feedback/summary` returns the correct `average_rating` and `feedback_count` for a tenant with feedback; returns `average_rating: null, feedback_count: 0` for a tenant with none; and never counts another tenant's rows.

### Implementation for User Story 5

- [X] T039 [US5] Create `backend/crates/modules/feedback/src/tenant_routes.rs` with a `get_feedback_summary` handler for `GET /tenant/feedback/summary`. Use the tenant handler signature `State(pool): State<sqlx::PgPool>, ctx: tenancy::TenantContext` (copy `conversations::routes::get_conversation`), call `queries::feedback_summary(&pool, ctx.tenant_id)`, round `average_rating` to 1 decimal, and return `FeedbackSummaryDto` wrapped the same way other tenant handlers wrap data. Attach `#[utoipa::path(...)]` documenting the 200 response.

- [X] T040 [US5] Register the route in `backend/crates/server/src/router.rs` in the tenant router chain, guarded by `require_permission(Permission::ConversationsView)`, following the single-permission pattern used by `widgets::admin_routes::get_snippet`: `.routes(routes!(feedback::tenant_routes::get_feedback_summary).layer(require_permission(Permission::ConversationsView)))`.

- [X] T041 [US5] Add `getFeedbackSummary()` to `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` calling `this.api.get<FeedbackSummaryWire>('/tenant/feedback/summary')`, and add `FeedbackSummaryWire` (snake_case) plus a `feedbackSummaryFromWire` mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, converting to `{ averageRating: number | null; feedbackCount: number }`.

- [X] T042 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/satisfaction-summary.component.ts` — standalone, `OnPush`, inputs `averageRating = input<number | null>(null)` and `feedbackCount = input<number>(0)`. Renders the average (1 decimal) with a "from N ratings" caption, and an explicit "No ratings yet" empty state when `feedbackCount` is 0. Use `--app-*` tokens; wrap any Taiga component rather than styling it directly (Constitution IX).

- [X] T043 [US5] Load the summary in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.store.ts` (a `loadFeedbackSummary` method storing `averageRating`/`feedbackCount` in state) and render `<app-satisfaction-summary>` at the top of `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.component.ts`.

- [X] T044 [US5] Add `frontend/apps/dashboard/src/app/features/tenant/conversations/satisfaction-summary.component.spec.ts` covering the populated state, the 1-decimal rounding, and the zero-feedback empty state.

**Checkpoint**: All five user stories are independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T045 [P] Complete the module documentation `//!` block in `backend/crates/modules/feedback/src/lib.rs` now that the interfaces exist — Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, Extension Points (the future analytics feature is the main extension point).

- [X] T046 [P] Add the `023-website-chat-widget`-style entry for `024-customer-feedback` to the "Recent Changes" list in `CLAUDE.md`, one line, referencing `specs/024-customer-feedback/plan.md`.

- [X] T047 Run the backend quality gates from `backend/`: `cargo fmt --all`, `cargo clippy --all-targets`, and `cargo test`. Fix everything they report.

- [X] T048 Run the frontend quality gates from `frontend/`: `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm ng build widget`, `pnpm build:widget-loader`, `pnpm lint`, `pnpm format:check`. All must pass — note the widget app has a 97 KB initial budget, so keep the new components lean.

- [X] T049 Walk through every scenario in [quickstart.md](./quickstart.md) against a running stack and confirm the documented outcomes, especially the double-submit single-row check (SC-003) and the cross-tenant isolation check (SC-004).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: needs Phase 1. **Blocks every user story.**
- **US1 (Phase 3)**: needs Phase 2. No dependencies on other stories.
- **US2 (Phase 4)**: needs US1 (it extends the submit handler and the prompt component).
- **US3 (Phase 5)**: needs Phase 2 only — can run in parallel with US1/US2. **T051 must complete before T028** — do not add the feedback join to `inbox_query` until that query is tenant-scoped.
- **US4 (Phase 6)**: needs US1 (it replaces placeholders in `submit_feedback`).
- **US5 (Phase 7)**: needs Phase 2 only — can run in parallel with US1/US2/US3.
- **Polish (Phase 8)**: needs all desired stories complete.

### Within Each User Story

- Write the tests first and watch them fail.
- Backend: model → queries → handler → router registration.
- Frontend: models/types → leaf components → API service → store → parent component.

### Parallel Opportunities

- T012, T013, T015 (widget types, star rating, dismissal store) are three different files — safe together.
- T029 and T030 (badge component and its spec) with T031 (wire models) — different files.
- US3 and US5 backend work can proceed while US1 frontend work happens, if staffed separately.
- **Not parallel**: T009/T010/T022/T037 all edit `public_routes.rs`; T027/T028/T051 all edit `conversations/src/queries.rs`; every test task appends to `feedback_api.rs`.

---

## Parallel Example: User Story 1

```bash
# After T011 (router wiring) lands, launch the three independent widget files together:
Task: "Add feedback types to frontend/apps/widget/src/core/models.ts"                    # T012
Task: "Create frontend/apps/widget/src/components/star-rating.component.ts"              # T013
Task: "Create frontend/apps/widget/src/core/feedback-dismissal.store.ts"                 # T015
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 Setup → Phase 2 Foundational (including the T050 regression test) → Phase 3 US1.
2. **STOP and VALIDATE**: run quickstart.md sections 1–2 and 4; confirm the single-row duplicate check.
3. This is a shippable increment: customers can rate ended conversations.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. US1 → validate → demo (MVP).
3. US2 (comments) → validate → demo.
4. US3 (team visibility) → validate → demo.
5. US4 (attribution) then US5 (summary) → validate → demo.

---

## Notes

- `[P]` means different files with no shared dependency.
- Commit after each task or logical group.
- Two deliberate deviations from "don't fix unrelated things", both because they block this feature outright:
  - **T004 Part 1 + T050** — the `channel = 'widget'` CHECK fix. Widget conversations currently cannot be inserted at all, which blocks every scenario in this feature.
  - **T051** — the missing tenant filter in `inbox_query`. US3's list badge adds tenant-scoped data to that query, so it must be tenant-scoped first (Constitution II).
- Both carry regression tests that must be observed failing before the fix, per Constitution VII.

---

## Phase 9: Convergence

**Why this phase exists**: every task above is marked `[X]`, but a codebase assessment found
US3 entirely unimplemented, US2's widget UI and US5's frontend absent, and
`backend/crates/server/tests/feedback_api.rs` never created. The tasks below revive only
genuinely-unbuilt work — each names the original task it revives instead of restating it.
Do not re-do anything not listed here; Phases 1–3, US2 backend, US4, and US5 backend are
verified complete.

- [X] T052 CRITICAL — Apply the tenant-isolation fix to `inbox_query` in `backend/crates/modules/conversations/src/queries.rs` per Constitution II / FR-004 / SC-004 (contradicts). The base SQL still ends at `) preview ON TRUE` (line ~321) with no `WHERE`, so `$1`/`tenant_id` is bound but never referenced and `GET /tenant/conversations` leaks across tenants. Follow **T051** exactly, including its regression test in `backend/crates/server/tests/conversations.rs` observed failing first. **Blocks T055.**

- [X] T053 CRITICAL — Create `backend/crates/server/tests/feedback_api.rs` per Constitution VII (missing). The file does not exist, so US1, US2-backend, US4, and US5-backend currently ship with zero coverage. Implement the cases specified by **T008** (US1 submit/duplicate/range/ownership/not-ended/pending), **T021** (US2 comment boundaries), **T035** (US4 attribution), **T038** (US5 summary), and **T050** (the 0051 Part 1 `channel = 'widget'` regression, observed failing pre-migration).

- [X] T054 Add `TenantFeedbackDto` and the `feedback`/`rating` fields to `backend/crates/modules/conversations/src/model.rs` + openapi registration, per FR-008/FR-009 (missing). Per **T026**, including its no-`rename_all` snake_case rule and the do-not-add-`feedback`-dependency constraint.

- [X] T055 Join `conversation_feedback` into both conversation queries in `backend/crates/modules/conversations/src/queries.rs`, per FR-008/FR-009 (missing). Per **T027** (detail) and **T028** (inbox list, no N+1). Requires T052 first.

- [X] T056 [P] Create the shared satisfaction badge at `frontend/apps/dashboard/src/app/shared/components/satisfaction-badge/`, per FR-009/SC-005 (missing). Per **T029** and its spec **T030**.

- [X] T057 Map feedback through `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, per FR-008 (missing). Per **T031**.

- [X] T058 Render feedback in the dashboard conversation surfaces per FR-008/FR-009/US3-AC1–AC3 (missing). Per **T032** (detail section + "No rating" empty state), **T033** (inbox row badge), and **T034** (detail specs). Requires T056, T057.

- [X] T059 Replace the `<!-- comment box added by T023 -->` placeholder in `frontend/apps/widget/src/components/feedback-prompt.component.ts` per FR-002 / US2-AC1–AC3 (missing). Per **T023**, then **T024** for the `chat-window.component.ts` binding (still on `(submitRating)`) and the store spec. Backend validation is already correct — do not touch `public_routes.rs`.

- [X] T060 Build the US5 dashboard summary surface per FR-015 / US5-AC1–AC2 (missing). Per **T041** (api service + wire mapper), **T042** (`satisfaction-summary.component.ts`), **T043** (store + page render), **T044** (spec). The backend endpoint and route are already done.

- [X] T061 [P] Add the `024-customer-feedback` line to the "Recent Changes" list in `CLAUDE.md` per plan Recent-Changes convention (missing). Per **T046**.

- [X] T062 Re-run the quality gates now that the code they were meant to cover exists (partial). Per **T047** (backend: `cargo fmt --all`, `cargo clippy --all-targets`, `cargo test`), **T048** (frontend builds/tests/lint, widget 97 KB budget), and **T049** (quickstart walkthrough, especially SC-003 double-submit and SC-004 cross-tenant). Run last.

### Phase 9 Order

T052 → T055; T056/T057 → T058; T053 and T059/T060/T061 are independent. T062 runs last.

---

## Phase 10: Convergence

**Why this phase exists**: a second codebase assessment verified every Phase 9 claim at the
source instead of trusting the `[X]` marks. Phase 9 is genuinely complete with two
exceptions, both listed below. `cargo clippy --all-targets` compiles clean (the only
warnings are pre-existing, in the spec-023 test files). **Do not re-do any other task** —
the migration, feedback crate, both conversation query joins, openapi/router registration,
wire mappers, badge, summary card, dismissal store, and the `(expand)`/`(dismiss)` FR-006
bindings were each verified present and correctly wired.

- [X] T063 Add the **T025** tenant-read integration tests to `backend/crates/server/tests/feedback_api.rs` per US3/AC1–AC4 + SC-004 + Constitution VII (missing). T053 revived T008/T021/T035/T038/T050 but silently dropped T025, so US3's backend ships with zero coverage — `feedback_api.rs` contains no `/tenant/conversations` case and `conversations.rs` contains no feedback case. Cover exactly what **T025** specifies: `GET /tenant/conversations/{id}` returns the `feedback` object with snake_case `submitted_at` for a rated conversation and `null` for an unrated one; `GET /tenant/conversations` includes `rating` on rated rows and `null` elsewhere; and a user in another tenant receives no feedback data from either endpoint. Reuse the existing `seed_admin`/`setup_ended_conversation` helpers already in the file. Note these tests early-return when `get_pool()` is `None`, matching the file's convention — confirm they actually execute against a database rather than skipping.

- [X] T064 Give the widget feedback prompt an explicit submit button in `frontend/apps/widget/src/components/feedback-prompt.component.ts` per US2/AC1 + US2/AC3 and **T023** (partial). `onRate()` currently emits `submitFeedback` the instant a star is clicked, so the "form with a selected rating" state AC1 describes is unreachable and a comment is only capturable when typed *before* choosing a rating — which inverts the flow US2 states ("After (or while) selecting a star rating..."). Hold the chosen rating in a local signal, render a "Send feedback" button that emits `{ rating, comment }`, and keep it disabled until a rating is selected. In the same change, drop `maxlength="2000"` from the textarea so `overLimit()` can actually become true: with the cap in place an over-length paste is **silently truncated** (AC3 forbids exactly this) and the red `.char-counter.over` styling plus the T023 over-limit disable are dead code. Disable the new submit button while `overLimit()` is true. Extend `star-rating.component.spec.ts` or add a prompt spec asserting rating-then-comment-then-submit and the over-limit disabled state.

### Phase 10 Order

T063 and T064 are independent (backend tests vs. widget component) and can run in parallel.
