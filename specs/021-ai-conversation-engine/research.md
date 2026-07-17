# Research: AI Conversation Engine

All Technical Context unknowns and clarification-driven design questions resolved below. Numbering is referenced from plan.md.

## 1. Engine placement and shape

**Decision**: Evolve the existing `ai::agent_responder` worker into a pipeline: `agent_responder.rs` keeps outbox claiming + gating (channel, conversation state, escalation, rule evaluation — all pre-existing from 017/020), then delegates to a new `engine.rs` that owns assemble → stream → supersede-check → resume → store | fallback. Confidence, generation records, and summary live in sibling files within `modules/ai`.

**Rationale**: 017 (config, gating, rules, prompt composition) and 020 (retrieval injection, citations) already built two-thirds of the engine inside `modules/ai`; the outbox claim/`FOR UPDATE SKIP LOCKED` pattern already provides per-message exactly-once processing and multi-instance safety. A new module would force either duplicated gating or a cross-module call mesh.

**Alternatives considered**: A separate `engine` module crate — rejected: it would own no tables of its own except `ai_generations` and would need nearly every `ai`-module private helper (prompt composition, credential resolution, config loading); the boundary would be artificial. Rewriting as a request-scoped (HTTP-triggered) generation — rejected: customer messages arrive via the outbox regardless of channel, and the worker model is what guarantees FR-016's single in-flight generation per conversation.

## 2. Streaming transport to the dashboard

**Decision**: Broadcast AI generation progress on the existing per-tenant `/tenant/events` SSE stream via the `escalations::presence::Runtime` broadcast channel, with a new `Event::ConversationAi(ConversationAiEvent)` variant. Event types: `ai.message.started`, `ai.message.delta`, `ai.message.completed`, `ai.message.superseded`, `ai.message.failed` (see `contracts/ai-events-sse.md`). Every payload carries `conversationId` + a per-generation `generationId`; the dashboard applies events only for the currently open conversation and ignores the rest.

**Rationale**: The tenant SSE stream, its auth, reconnect handling, and the fetch-based frontend client all exist (014). The responder worker already holds an `Arc<presence::Runtime>` handle, so the engine can broadcast without new wiring. Delta fan-out to all connected staff of a tenant is acceptable at current scale (deltas are small text fragments; conversations are tenant-scoped).

**Boundary note (Principle I)**: this makes the `escalations` module the de-facto tenant realtime hub carrying an AI-owned event. Recorded trade-off: the event *payload structs* are defined in `escalations::model` (the module that owns the wire contract of its stream), and `ai` depends on `escalations` (a dependency that already exists for routing/presence). If a third module needs realtime events, extract a `realtime` module owning the runtime + event enum; designed so that extraction is a move, not a rewrite.

**Alternatives considered**: Per-conversation SSE endpoint — rejected: N connections per agent, new auth surface, duplicate reconnect logic. WebSockets — rejected: new infrastructure for no additional requirement; SSE already proven here. Polling the timeline — rejected: violates Principle X's explicit streaming call-out and SC-002's 5-second first-content target.

## 3. Supersede & cancellation semantics (clarifications Q2/Q5, FR-016)

**Decision**: Three enforcement points, all deterministic:

1. **Claim-time coalescing**: when claiming a `conversation.customer_message` outbox event, delete any *older* unclaimed events for the same conversation (their content is already in the history the engine loads). The engine always answers the newest known state.
2. **Mid-stream checks**: during streaming, every ~1 s (checked between deltas, cheap indexed query) the engine looks for (a) a customer message newer than the trigger message and (b) an open escalation / `ai_handling = 'human'`. On (a): abort the provider stream, broadcast `ai.message.superseded`, discard partial content, delete the claimed event — the newer message's own outbox event drives exactly one regeneration. On (b): abort, broadcast `ai.message.superseded`, discard, delete the event (engine stays silent per Q5).
3. **Pre-commit re-check**: immediately before the insert transaction, re-run the newer-message + escalation checks and the existing `has_ai_reply_since` idempotency guard. Any hit ⇒ discard instead of insert.

**Rationale**: The outbox already serializes triggers per message; coalescing + pre-commit checks give supersede semantics without a cancellation token registry or cross-worker signaling. Mid-stream polling bounds wasted provider spend to ~1 s after a superseding event, which is the cheapest mechanism that meets "cancelled cleanly".

**Alternatives considered**: In-memory cancellation registry keyed by conversation — rejected: breaks under multiple server instances; the DB checks are instance-agnostic. Letting the in-flight response complete and post both — rejected by clarification Q2.

## 4. Resume-on-mid-stream-failure (clarification Q4)

**Decision**: "Resume" is implemented as a **continuation request**: on a retriable stream error after partial content was received, re-issue the same request with the accumulated partial appended as a trailing `Assistant` message plus a fixed continuation instruction (deterministic suffix: continue exactly from where the previous text stops, no repetition, no preamble). The final message is partial + continuation stitched. Continuation attempts consume the same bounded retry budget; if the continuation also fails, partial content is discarded and the fallback path runs (per Q4's fallback clause). If the failure occurs before any content was received, retry is a plain re-generation.

**Rationale**: None of OpenAI/Anthropic/Gemini chat APIs offer native mid-stream resume of a broken response; assistant-prefill continuation is the provider-independent equivalent and works through the existing `ChatRequest` shape with zero contract changes. Stitching is deterministic (concatenation; single trailing/leading whitespace normalization).

**Alternatives considered**: Provider-native resume — does not exist. Always discard-and-regenerate on stream failure — simpler but rejected by clarification Q4 (user explicitly chose resume). Accept-partial-with-marker — rejected in clarification.

## 5. Retry/fallback budget and fallback behavior (FR-012/FR-013, SC-005)

**Decision**: Engine-level budget: **≤ 3 provider attempts** (initial + up to 2 retries/continuations, only on `retriable` error categories — reusing `ErrorCategory::retriable()`), exponential backoff with jitter (mirroring the existing `run_attempts` `RETRY_BASE_MS` pattern), under an **outer deadline of 45 s** from claim. Non-retriable errors (auth, invalid request) and empty/whitespace-only completions short-circuit to fallback. On exhaustion: in one transaction, insert the fallback message (existing `system`-kind path, platform-default text: "I'm sorry — I'm having trouble responding right now. A team member will follow up shortly.") and route the conversation through `escalations::routing::route_new_escalation_in_tx` with reason `"AI assistant unavailable"`; broadcast `ai.message.failed`; write an `ai_generations` row with outcome `fallback`. The outbox event is always deleted (never poison-pilled into infinite retry).

**Rationale**: 3 attempts × backoff + 45 s deadline keeps worst case comfortably inside SC-005's 60-second bound while tolerating transient rate limits. `system` kind reuses the auto-ack rendering path (017) so the fallback needs no new message kind or frontend case. Routing reuse is mandated by the spec's assumptions.

**Alternatives considered**: Reusing `AiService::complete`'s internal retries on top of engine retries — rejected: nested retry multiplication would blow the deadline; the engine drives the streaming call directly with its own budget. Fallback as `ai`-kind message — rejected: it isn't the agent speaking, and it must not carry confidence/citations.

## 6. Confidence derivation (clarification Q3, FR-010)

**Decision**: Deterministic backend heuristic computed at storage time, no extra provider calls:

```
score = clamp01( 0.35                                  # base: model produced a complete answer
        + 0.45 * top_chunk_similarity                  # grounding strength (0 when no chunks)
        + 0.10 * min(chunk_count, 3) / 3               # corroboration breadth
        - 0.25 * (finish == Length)                    # truncated answer
        - 0.15 * retrieval_degraded                    # retrieval failed/timeout
        - 0.10 * (continuation_used) )                 # stitched answer is less certain
```

Bands (derived at read time, single source of truth in one backend function, mirrored constant in frontend models): `high ≥ 0.70`, `medium ≥ 0.40`, `low < 0.40`. Score stored on the message row (`ai_confidence_score`); band never stored.

**Rationale**: Deterministic (Principle IV), explainable to staff, zero latency/cost, and testable with exact-value unit tests. Its inputs are exactly the signals the engine already has (retrieval stats from 020, finish reason and continuation state from the stream). Self-assessment via the model would add cost/latency and is non-deterministic; it can replace the heuristic later without schema changes since only the score is stored.

**Alternatives considered**: Model self-reported confidence (structured suffix or second call) — rejected for v1: non-deterministic, prompt-polluting or doubles cost; heuristic keeps v1 informational role honest. Log-prob based — rejected: not uniformly available across the three providers' APIs.

## 7. Generation records (FR-015, SC-008)

**Decision**: New `ai_generations` table (see data-model.md) written once per engine run — outcomes: `success`, `superseded`, `cancelled_escalation`, `failed`, `fallback`. It references the trigger message, the response message (when one was stored), and the `ai_usage_records` row id produced by usage tracking (linkage, not duplication, per spec assumption). Plus an `engine.generate` tracing span carrying the same correlation ids.

**Rationale**: `ai_usage_records` (015) is provider-call-grained and tenant-billing-oriented; the engine needs conversation-grained outcome semantics (a superseded run may have zero or several provider calls). A linking row satisfies "extend, not duplicate".

**Alternatives considered**: Adding conversation/outcome columns to `ai_usage_records` — rejected: wrong grain (one engine run ↦ N usage rows), and would overload a billing-facing table with engine semantics.

## 8. Summary generation (FR-017, SC-007)

**Decision**: `POST /tenant/conversations/{id}/summary` (authenticated tenant route, conversation read access). Handler loads a bounded window (last 50 messages, tenant-scoped), builds a fixed summary system prompt (customer goal, what was tried/answered, current state — deterministic template, agent persona *not* applied), calls `AiService::complete` with the tenant's resolved provider/model (same resolution chain as the engine: tenant override if credential resolves, else platform default), returns `{ summary, generatedAt }`. Nothing persisted; failures return a standard error envelope (frontend shows non-blocking error per spec).

**Exact summary system prompt (verbatim, deterministic — implementers MUST use this string, no paraphrase):**

```
You are a support-operations assistant helping a human agent take over a customer conversation. Summarize the conversation below for an internal teammate — not for the customer. Write 3 to 5 sentences, plain and factual, covering exactly:
1. What the customer wants (their goal or problem).
2. What has already been tried or answered so far.
3. The current state and any open question or next step.
Do not address the customer. Do not invent details that are not in the transcript. Do not include pleasantries, headings, or bullet formatting — return only the summary prose.
```

The conversation window is supplied as the message turns after this system prompt: each stored message becomes one turn — `customer` → `User` role, `ai`/`reply` → `Assistant` role, `system` → prefixed inline as `[system: <body>]` on a `User` turn (system-kind rows are auto-acks/fallbacks and carry context but have no chat role). `max_output_tokens` is left to the tenant config default. This prompt is a constant (e.g. `const SUMMARY_SYSTEM_PROMPT: &str`) so the template is deterministic per Principle IV.

**Rationale**: On-demand + non-persisted is the spec's assumption; `complete` (non-streaming) is appropriate for a ≤10 s, single-shot payload. Neutral (non-persona) prompt because the audience is staff, not the customer.

**Alternatives considered**: Streaming the summary — rejected: adds SSE machinery for a panel that renders at once within budget. Caching by last-message-id — deferred: premature until usage shows repeat requests dominate.

## 9. Streaming provider call with tenant override

**Decision**: Add `AiService::stream_with_override(ctx, input, provider, model)` mirroring the existing `complete_with_override` (credential resolution, usage recording with `streamed = true`, error mapping), and have the engine consume `AiResultStream` (`Delta` / `Done` / `Error` — types already defined in `service.rs`). Engine buffers deltas for persistence while forwarding them to the SSE broadcast (throttled to ~4 events/s to bound fan-out chatter; final content is always exact).

**Rationale**: `stream()` exists for the platform path; the override variant is a small symmetric addition keeping all provider/credential logic inside `AiService` (Principle IV). Delta throttling protects the broadcast channel without affecting stored content.

## 10. Frontend realtime + rendering approach

**Decision**: `realtime.service.ts` (fetch-based SSE client from 014) gains parsing for the `ai.message.*` event family and exposes them on its typed event stream (RxJS). `conversation-detail.store.ts` holds in-flight generation state per open conversation: `started` ⇒ show `ai-thinking-indicator`; `delta` ⇒ append to a streaming buffer rendered as a provisional AI card; `completed` ⇒ replace buffer with the final message object (id, citations, confidence) appended to the timeline; `superseded`/`failed` ⇒ clear buffer (and on `failed`, the fallback message arrives as a normal timeline refresh/`completed`-adjacent path). Reload/mid-join coherence: timeline fetch remains the source of truth; the streaming buffer is display-only and never written to the timeline cache. New shared components: `ai-confidence-badge` (band chip, Helix tokens), `ai-thinking-indicator` (animated dots). AI response card = `kind === 'ai'` branch of the existing thread message article with distinct styling + badge + existing citation list.

**Rationale**: Matches the established SignalStore/realtime patterns from 014/020; provisional-buffer-vs-timeline separation guarantees SC-003 (no duplicated/lost messages across reconnects — worst case the buffer is dropped and the completed message appears on the next timeline fetch/event).

**Alternatives considered**: Writing deltas into the timeline store — rejected: reconciliation bugs on reconnect (duplicate/partial rows); display-only buffer is strictly simpler.

## 11. Retry-visibility & observability details

**Decision**: `engine.generate` tracing span wraps the whole run (fields: tenant_id, conversation_id, trigger_message_id, generation_id, attempts, outcome, latency_ms, confidence, superseded_by arrival where applicable); existing `rag.retrieve` span remains nested within. `ai.message.failed` SSE payload carries no provider error detail (sanitized category only) — staff-facing failure detail lives in the conversation via the fallback message; operator-facing detail lives in `ai_generations.error_category` + logs.

**Rationale**: Satisfies FR-015/Principle VI without leaking provider internals to browsers.
