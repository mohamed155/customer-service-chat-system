# Research: Customer Feedback (024)

No NEEDS CLARIFICATION markers remained in the Technical Context; research resolved the design unknowns below against the existing codebase (widgets module, conversations schema, dashboard features).

## R1 — Module placement: new `feedback` crate

- **Decision**: Create `backend/crates/modules/feedback` as a new module crate with `model.rs` / `queries.rs` / `public_routes.rs` / `tenant_routes.rs`, mounted by `server/src/router.rs` (public routes inside the existing widget CORS/rate-limit layer group; tenant routes inside the tenant RBAC group).
- **Rationale**: Constitution I requires extractable module boundaries. Feedback has its own lifecycle, tables, and both public and tenant surfaces — the same shape as the `widgets` module, which is the closest structural precedent.
- **Alternatives considered**: (a) Extend the `widgets` module — rejected: feedback is channel-agnostic by design (future email/social channels attach feedback too); (b) use the `analytics` placeholder crate — rejected: capture ≠ analytics; the future analytics feature is a *consumer* of this table.

## R2 — Duplicate prevention: DB uniqueness + idempotent success

- **Decision**: `UNIQUE (tenant_id, conversation_id)` index on `conversation_feedback`; submission uses `INSERT ... ON CONFLICT DO NOTHING` followed by a read. First write returns `201` with the record; a duplicate (including concurrent/retried) returns `200` with the existing record — the customer always sees success.
- **Rationale**: The spec's concurrency edge case ("exactly one record, customer sees success") can only be guaranteed at the database layer (Constitution II/VIII); idempotent success matches Constitution V's idempotency guidance and the widget's retry behavior after network timeouts.
- **Alternatives considered**: Application-level existence check then insert — rejected: racy under concurrent submits; `409 Conflict` on duplicates — rejected: spec requires the customer to see success, and the widget would need special-casing.
- **Note**: A widget conversation belongs to exactly one widget session (the creating session is the only one that can access it), so per-conversation uniqueness also satisfies the spec's "per conversation per customer session" boundary.

## R3 — Attribution snapshot columns

- **Decision**: Store on each feedback row: `channel` (copied from the conversation), `assigned_membership_id` (the conversation's assignee at submission time, nullable), `agent_configuration_id` (nullable, see R4), `widget_session_id` (nullable FK, `ON DELETE SET NULL` so session purging doesn't destroy feedback).
- **Rationale**: FR-005/FR-012 require attribution to survive later conversation changes (reassignment, archival). Snapshotting at write time is the only way to preserve state-at-conversation-end; justified as a Complexity Tracking deviation in plan.md.
- **Alternatives considered**: Join `conversations` at read time — rejected: reports current, not historical, assignee/channel; a separate attribution history table — rejected: over-engineering for one snapshot per conversation.

## R4 — Determining "the AI agent involved"

- **Decision**: At submission time, if the conversation has at least one `ai_generations` row, set `agent_configuration_id` to the tenant's live `agent_configurations` row (the schema enforces at most one live config per tenant via `agent_configurations_tenant_single_live_uq`); otherwise `NULL`.
- **Rationale**: `ai_generations` proves AI participation but does not record which agent configuration produced the response; the single-live-config invariant makes the tenant's live config the correct attribution today. Nullable column keeps human-only and platform-fallback conversations honest.
- **Alternatives considered**: Add `agent_configuration_id` to `ai_generations` and derive from the latest generation — rejected for this feature: touches the AI engine's write path for no additional fidelity while the single-live-config invariant holds; can be adopted later without changing the feedback schema.

## R5 — Public API surface & conversation-end signal

**Codebase findings that drove this decision** (verified in `crates/modules/widgets/src/`):

- There is **no** `GET /widget/v1/conversations/{id}`. The real endpoint is `GET /widget/v1/conversation` (singular, no path param); it resolves the session's *active* conversation and filters `status NOT IN ('resolved','closed')` — so it returns `data: null` once a conversation ends. Feedback cannot be embedded there.
- There is **no SSE `closed` event**. `public_events.rs::filter_and_map` emits only `ai.delta`, `message.created`, and `conversation.updated` with `handling: "human"`. The `handling === 'closed'` branch in `widget.store.ts` is unreachable dead code today.
- "Ended" in this codebase is `status IN ('resolved','closed')` (statuses are `open|pending|resolved|closed` per migration 0033), not `closed` alone.
- `send_message` already returns `409 conversation_closed` for ended conversations, and `widget.store.ts::handleClosedConversation()` exists but is never called.

- **Decision**: Two public endpoints under `/widget/v1`, session-authenticated (`authenticate_session`) and origin/rate-limited like `send_message`, both owned by the new `feedback` module:
  - `POST /widget/v1/conversations/{conversationId}/feedback` — submit.
  - `GET /widget/v1/feedback/pending` — returns the session's most recent **ended** conversation that has **no** feedback row yet, or `null`. The server resolves session → `customer_id` → conversation itself.
  The widget calls the pending endpoint (a) on `open()`, and (b) when `sendMessage` fails with 409 `conversation_closed` (wiring up the existing dead `handleClosedConversation()`). No changes to `get_conversation`, no changes to SSE.
- **Rationale**: A session-keyed server-side lookup means the widget never persists a conversation id, and it sidesteps the pre-existing `res.data?.conversation` response-shape mismatch in `widget-api.service.ts`. Returning "pending" only when no feedback row exists gives FR-007's never-re-prompt behavior for free.
- **Known limitation (accepted for v1)**: if the customer leaves the widget open and idle while an agent closes the conversation, the prompt appears at their next interaction (reopen or send attempt) rather than instantly. Making it instant requires a new `conversation.closed` SSE event.
- **Alternatives considered**: (a) Embed feedback in `GET /widget/v1/conversation` — rejected: that endpoint returns null for ended conversations, which is exactly when feedback is wanted. (b) Extend that query to fall back to the latest ended conversation — rejected: silently changes existing widget-open behavior (ended transcript instead of a fresh chat) and revives dead code paths. (c) Add a `conversation.closed` SSE event — rejected for v1: requires a new `escalations::presence::Event` variant plus publishing from `conversations::routes` (which currently publishes no events), inverting the conversations→escalations module dependency for a UX nicety.

## R6 — Tenant surfaces: detail, list badge, summary card

- **Decision**: (a) Extend the tenant conversation detail DTO with a `feedback` object and the list-row DTO with a nullable `rating` field, fetched in the existing queries (join on `conversation_feedback`) — no per-row requests. (b) New shared `satisfaction-badge` component in `dashboard/src/app/shared/components/` (sibling of `status-badge` / `ai-confidence-badge`). (c) New `GET /tenant/feedback/summary` returning `{ average_rating, feedback_count }` (snake_case — tenant surface); rendered as a summary card on the Conversations page.
- **Rationale**: Constitution X forbids N+1 — rating must ride the list query. Constitution IX puts the badge in shared components. The summary card goes on the Conversations page because Overview and Analytics are still fixture-driven; wiring one real HTTP stat into a fixture page would be misleading, while the Conversations feature already has a real API service and store to extend.
- **Alternatives considered**: Summary on the Overview page — rejected until Overview is de-fixtured; computing the average client-side from loaded conversations — rejected: the list is paginated, the average must be tenant-wide.
- **Wire-format constraint (verified)**: `/tenant` responses are **snake_case** — `conversations::model` structs carry no `rename_all` attribute and the dashboard's `ConversationWire` reads `last_activity_at` / `display_name`. `/widget/v1` responses are **camelCase** (`#[serde(rename_all = "camelCase")]` on the widget DTOs). New tenant DTOs must therefore be snake_case on the wire and get a `*FromWire` mapper in `core/api/tenant-api.models.ts`, matching `conversationFromWire`.

## R7 — Widget prompt/dismissal state

- **Decision**: Prompt state is derived, not stored server-side: show the active prompt when `GET /widget/v1/feedback/pending` returns a conversation; after an explicit dismissal (tracked per-conversation in `localStorage` alongside the existing widget session key), collapse to a passive "rate this conversation" affordance that stays available while the pending lookup keeps returning that conversation. A successful submit permanently replaces both with a thank-you state (and the conversation stops being returned as pending).
- **Rationale**: FR-006/FR-014 make dismissal a UX preference, not a domain fact — persisting it server-side would add an endpoint and table for no tenant-visible value. Browser storage already holds widget session state.
- **Alternatives considered**: Server-persisted dismissal — rejected as above; re-prompting on every reopen — rejected by clarification Q5 (answer B).

## R8 — Validation & limits

- **Decision**: Rating: integer 1–5, enforced in the payload type, handler validation, and a DB `CHECK`. Comment: optional, trimmed, max 2,000 characters enforced in widget UI (live counter), handler validation (422 with explicit message — no silent truncation), and DB `CHECK`. Submission is accepted only when the conversation belongs to the authenticated session (`conversations.tenant_id = session.tenant_id AND conversations.customer_id = session.customer_id`) and its status is `resolved` or `closed`; anything else gets the standard error envelope (404 for not-yours/not-found, 422 for not-ended or invalid payload).
- **Rationale**: Defense in depth matching Constitution II/III; the spec's acceptance scenario requires an explicit over-length message rather than truncation.
- **Alternatives considered**: UI-only validation — rejected: frontend-only checks are never the sole enforcement (Constitution II).
