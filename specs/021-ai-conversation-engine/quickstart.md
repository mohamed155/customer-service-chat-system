# Quickstart: AI Conversation Engine — validation guide

Runnable scenarios proving the feature end-to-end. References: [data-model.md](data-model.md), [contracts/](contracts/), [spec.md](spec.md).

## Prerequisites

- PostgreSQL with migrations applied through `0048_ai_conversation_engine.sql` (`backend/migrations/README.md` workflow).
- Backend running (`cargo run -p server` from `backend/`) with at least one AI provider configured (platform default credentials, or a tenant credential via the AI settings page). For deterministic local runs, point the provider base URL at a mock (existing `ai_openai_base_url`-style config).
- A tenant with: a configured & active AI agent (017), the conversation's channel enabled in agent settings, and ideally a published + indexed knowledge article containing a distinctive fact (019/020).
- Dashboard running (`pnpm start` in `frontend/`), signed in as a member of that tenant.

## Scenario 1 — End-to-end AI response (US1, SC-001)

1. Create a conversation and add a customer message asking about the distinctive fact (dashboard new-conversation dialog, or `POST /tenant/conversations` + `POST /tenant/conversations/{id}/messages` with a customer sender).
2. Watch the thread: thinking indicator appears, text streams in, final AI card renders with citations and a confidence badge.
3. Reload the page → the AI message persists in correct order with the same content, citations, and badge.
4. Verify the trace: `SELECT outcome, attempts, retrieval_chunk_count, confidence_score FROM ai_generations WHERE conversation_id = '…'` → one `success` row referencing the response message.

**Expected**: response reflects the article fact and agent persona; `ai.message.started/delta/completed` observed on `GET /tenant/events` (SC-002: first content ≤ 5 s).

## Scenario 2 — Supersede on rapid second message (clarification Q2, FR-016)

1. Send a customer message that yields a long answer; while streaming, send a second customer message.
2. **Expected**: `ai.message.superseded` (`reason: "newer_message"`) clears the partial; a new generation starts; exactly **one** final AI message answering both messages; `ai_generations` shows one `superseded` + one `success` row; no interleaved/duplicate replies (also verify via timeline API).

## Scenario 3 — Escalation cancels in-flight generation (clarification Q5)

1. Trigger a generation; while streaming, claim/escalate the conversation as a human agent (or send a message matching an escalation rule keyword beforehand for the rule path).
2. **Expected**: `ai.message.superseded` (`reason: "escalated"`); no AI message posted; outcome `cancelled_escalation`; subsequent customer messages get no AI response.

## Scenario 4 — Provider failure → resume, then fallback (US2, Q4, SC-005)

1. Point the tenant's provider at the mock configured to fail mid-stream once, then succeed. Send a customer message.
   **Expected**: single coherent final message (continuation stitched), `ai_generations.continuation_used = true`, confidence slightly reduced.
2. Reconfigure the mock to always fail. Send a customer message.
   **Expected** within 60 s: `ai.message.failed` event; fallback `system` message in the thread ("…a team member will follow up…"); conversation routed/queued for human attention (escalation visible in queue); `ai_generations` outcome `fallback` with `error_category` set; customer message never dropped; human-handled conversations unaffected.

## Scenario 5 — Tenant isolation (FR-014, SC-004)

Run the dedicated integration tests (two tenants, distinctive knowledge + configs):

```
cd backend && cargo test -p server engine_isolation
```

**Expected**: tenant B's prompts/responses never contain tenant A's config, history, or knowledge; `ai_generations` rows invisible cross-tenant; summary endpoint 404s for the other tenant's conversation.

## Scenario 6 — Confidence badge (US4)

1. Trigger one well-grounded response (knowledge hit) and one ungrounded response (question with no relevant knowledge).
2. **Expected**: both AI cards show badges; grounded response bands higher than ungrounded; `messages.ai_confidence_score` populated; badge absent on customer/human/system messages; no confidence data in any non-staff payload.

## Scenario 7 — Conversation summary (US5, SC-007)

1. In a conversation with ~a dozen mixed messages, use the summary control in the conversation detail view.
2. **Expected**: summary panel renders ≤ 10 s, accurately covering customer goal / what happened / current state; nothing persisted (repeat call regenerates); `POST /tenant/conversations/{id}/summary` returns the [contract shape](contracts/conversation-summary.md); summary of an empty conversation → `422`; provider failure → non-blocking error toast, thread unaffected.

## Quality gates (run before completion)

```
cd backend  && cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm format:check && pnpm ng test dashboard --watch=false
```

Plus: OpenAPI validity/coverage tests (`cargo test -p server openapi`) must include the summary route and confidence fields.
