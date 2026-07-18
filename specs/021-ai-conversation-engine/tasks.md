---
description: "Task list for AI Conversation Engine (021-ai-conversation-engine)"
---

# Tasks: AI Conversation Engine

**Input**: Design documents from `/specs/021-ai-conversation-engine/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included — the constitution (Principle VII: Test-First & Regression Discipline) mandates unit/integration/API coverage; FR-014/SC-004 require an automated cross-tenant isolation test.

**Organization**: Grouped by user story (US1 = Story 1 respond+store P1 · US2 = Story 2 graceful failure P2 · US3 = Story 3 streaming P3 · US4 = Story 4 confidence P4 · US5 = Story 5 summary P5).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: May run in parallel (different file, no dependency on an incomplete task).
- **[Story]**: US1–US5 — omitted for Setup, Foundational, Polish.
- File paths are exact, relative to the repository root.

## How to read a task (for the implementer)

Each task names: the **exact file**, the **exact function/type signature** to add or change, the **doc section** that fully specifies behavior (open the referenced `plan.md` / `research.md` / `data-model.md` / `contracts/*` section — it leaves no design decisions), and an **Accept:** line stating the check that proves the task done. Do not invent behavior not in the referenced section; if something seems missing, it is in the referenced doc. After finishing a task, change its ` - [x]` to `- [X]` in this file.

## Reference: key existing symbols you will call (already implemented)

Backend (`backend/crates/`):
- `ai_providers::{Message, Role, ChatRequest, ChatStream, StreamEvent, ErrorCategory, ProviderError}` — `ai-providers/src/contract.rs`. `ErrorCategory::retriable()` returns true for `RateLimited|Unavailable|Timeout`.
- `ai::service::{AiService, AiCallContext, AiInput, AiCallResult, AiCallError, AiStreamEvent, AiResultStream}` — `modules/ai/src/service.rs`. Existing methods: `complete`, `complete_with_override(ctx, input, provider, model)`, `stream(ctx, input)`, `embed_platform(ctx, inputs)`. `AiStreamEvent = Delta(String) | Done(AiCallResult) | Error{category}`.
- `ai::agent_config::{load_live, credential_resolves, AgentConfigurationRow, EscalationRule}` — `modules/ai/src/agent_config.rs`.
- `ai::usage::{UsageWrite, insert}` — `modules/ai/src/usage.rs` (do NOT duplicate; `ai_generations` links to a usage row by id).
- `conversations::queries::{recent_history, message_body, customer_display_name, insert_ai_reply_in_tx, insert_citations_in_tx, insert_auto_ack_in_tx, has_ai_reply_since, has_system_message, conversation_ai_state}` — `modules/conversations/src/queries.rs`.
- `conversations::model::{Message, CitationView, CitationToInsert}` — `modules/conversations/src/model.rs`.
- `knowledge::retrieval::{search, RetrievedChunk}` — `modules/knowledge/src/retrieval.rs` (`RetrievedChunk` has `item_id, item_title, content, similarity`).
- `escalations::routing::{route_new_escalation_in_tx, has_open_escalation}` — `modules/escalations/src/routing.rs`.
- `escalations::presence::{Runtime, Event}` — `modules/escalations/src/presence.rs`. `Runtime::broadcast(tenant_id, Event)` fans an event to all connected members of a tenant.
- The agent responder worker/loop and all gating logic: `modules/ai/src/agent_responder.rs` (`process_agent_responder_once`, `run_agent_responder_worker`).

Frontend (`frontend/apps/dashboard/src/app/`):
- `core/realtime/realtime.service.ts` — `RealtimeService.events(): Observable<SseEvent>` where `SseEvent = { event: string; id: string; data: string }`. Already parses arbitrary SSE frames; you add typed handling for `ai.message.*`.
- `features/tenant/conversations/{conversation-thread.component.ts, conversation-detail.component.ts, conversation-detail.store.ts, conversations-api.service.ts}`.
- `shared/components/citation-list/citation-list.component.ts` — pattern to copy for new shared components.

---

## Phase 1: Setup

**Purpose**: Confirm a green baseline before changes so later failures are attributable.

 - [x] T001 Run `cd backend && cargo build --workspace` and `cd frontend && pnpm install`; confirm both succeed. Confirm the next migration number is `0048` by listing `backend/migrations/` (last existing is `0047_embeddings_rag.sql`). **Accept:** workspace builds; no migration named `0048_*` exists yet.

**Checkpoint**: Baseline compiles; migration slot confirmed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema, the generation-record and confidence modules, and the streaming provider call that every user story depends on. **No user-story phase may start until this phase is complete.**

 - [x] T002 Write migration `backend/migrations/0048_ai_conversation_engine.sql` creating table `ai_generations` and altering `messages`, exactly per [data-model.md](data-model.md) §1 and §2. Include: all `ai_generations` columns with the stated types/NULLability, the `outcome` CHECK constraint (`success|superseded|cancelled_escalation|failed|fallback`), the `confidence_score` CHECK (`>= 0 AND <= 1`), FKs (`tenant_id→tenants`, `conversation_id→conversations`, `trigger_message_id→messages`, `response_message_id→messages` NULL, `usage_record_id→ai_usage_records` NULL); the two b-tree indexes `(tenant_id, conversation_id, created_at DESC)` and `(tenant_id, created_at DESC)`; and `ALTER TABLE messages ADD COLUMN ai_confidence_score real NULL CHECK (ai_confidence_score >= 0 AND ai_confidence_score <= 1)`. Follow existing migration conventions (see `0047_embeddings_rag.sql`: `gen_random_uuid()` PK default, `timestamptz DEFAULT now()`). **Accept:** migration applies cleanly on a fresh DB per `backend/migrations/README.md`; `\d ai_generations` shows all columns and constraints.

 - [x] T003 [P] Create `backend/crates/modules/ai/src/generation_record.rs` and add `pub mod generation_record;` to `backend/crates/modules/ai/src/lib.rs`. Define `pub enum GenerationOutcome { Success, Superseded, CancelledEscalation, Failed, Fallback }` with `pub fn as_str(&self) -> &'static str` mapping to the exact CHECK strings in [data-model.md](data-model.md) §1. Define `pub struct GenerationRecord` with fields mirroring the `ai_generations` columns (see data-model §1 table; use `Uuid`, `Option<Uuid>`, `Option<String>`, `i16` for `attempts`, `bool` flags, `Option<f32>` scores, `i32 latency_ms`, `Option<String> request_id`). Implement `pub async fn insert(pool: &sqlx::PgPool, rec: &GenerationRecord) -> sqlx::Result<uuid::Uuid>` doing a single INSERT returning `id`. This table is append-only — no update/delete functions. **Accept:** `cargo build -p ai` passes; a unit test in `#[cfg(test)]` asserts `GenerationOutcome::Fallback.as_str() == "fallback"` for every variant.

 - [x] T004 [P] Create `backend/crates/modules/ai/src/confidence.rs` and add `pub mod confidence;` to `backend/crates/modules/ai/src/lib.rs`. Implement per [research.md](research.md) §6 and [data-model.md](data-model.md) §5:
  - `pub struct ConfidenceInputs { pub top_chunk_similarity: f32, pub chunk_count: u32, pub finish_length: bool, pub retrieval_degraded: bool, pub continuation_used: bool }`.
  - `pub fn confidence_score(i: &ConfidenceInputs) -> f32` implementing exactly: `clamp01(0.35 + 0.45*top_chunk_similarity + 0.10*(min(chunk_count,3) as f32/3.0) - 0.25*(finish_length as f32) - 0.15*(retrieval_degraded as f32) - 0.10*(continuation_used as f32))` where `clamp01(x)=x.clamp(0.0,1.0)`. When there are no chunks, `top_chunk_similarity` is `0.0`.
  - `pub enum Band { High, Medium, Low }` with `pub fn as_str(&self)->&'static str` (`"high"|"medium"|"low"`), and `pub fn confidence_band(score: f32) -> Band` using thresholds `high >= 0.70`, `medium >= 0.40`, else `low`.
  - **Accept:** `#[cfg(test)]` unit tests assert exact values, e.g. no grounding (`similarity 0, count 0, all flags false`) → `0.35` → `Low`; strong grounding (`similarity 0.9, count 3, flags false`) → `clamp01(0.35+0.405+0.10)=0.855` → `High`; truncated (`finish_length true`) subtracts `0.25`. All tests pass.

 - [x] T005 Add `pub async fn stream_with_override(&self, ctx: AiCallContext, input: AiInput, provider: &str, model: &str) -> Result<AiResultStream, AiCallError>` to `backend/crates/modules/ai/src/service.rs`, per [research.md](research.md) §9. Mirror the existing `complete_with_override` credential resolution (resolve config for `Scope::Tenant`, resolve credential for the passed `provider`, build a single `Attempt` with the passed `model`) but drive the streaming path like the existing `stream` method (branch on `provider_kind.supports_streaming()`; on unsupported provider fall back to `run_attempts_traced` then emit `Delta`+`Done`; on supported provider call `provider.stream(&key, &req)` and map `StreamEvent::Delta→AiStreamEvent::Delta`, `StreamEvent::Done{..}→AiStreamEvent::Done(AiCallResult{..})`, provider error→`AiStreamEvent::Error{category}`). Record one `ai_usage_records` row with `streamed: true` when the stream terminates (reuse the `usage::UsageWrite`/`usage::insert` pattern already in `stream`). **Accept:** `cargo build -p ai` passes; method signature matches; usage row written with `streamed=true` on completion.

**Checkpoint**: `ai_generations` exists, generation records are writable, confidence is computable/bandable and unit-tested, and a tenant-overridable streaming call is available. US1 can now be built.

---

## Phase 3: User Story 1 — A customer message receives an AI response (P1) 🎯 MVP

**Goal**: Refactor the existing responder so assembly + provider call + store live in a new streaming `engine.rs`, consuming the provider stream to a full reply, storing it (with citations) tenant-scoped, and writing a `success` generation record. (Streaming to the UI, confidence, retries/fallback, and summary are later stories.)

**Independent Test**: quickstart.md Scenario 1 — publish a knowledge article with a distinctive fact, send a customer message about it in an AI-handled conversation, confirm an AI-attributed reply reflecting the fact persists after reload, and one `ai_generations` row with `outcome='success'` references it.

### Tests for User Story 1

 - [x] T006 [P] [US1] Add integration test `backend/crates/server/tests/engine_respond.rs`: seed a tenant with a configured active agent (reuse existing agent-config test helpers) and a conversation with a customer message; run `ai::agent_responder::process_agent_responder_once` until it returns `Ok(false)`; assert an `ai`-kind message was stored in the conversation and exactly one `ai_generations` row with `outcome='success'`, `response_message_id` = the stored message id, and `latency_ms >= 0`. **Accept:** test compiles and fails only because engine wiring (T008–T011) is not yet done, then passes after.
 - [x] T007 [P] [US1] Add integration test `backend/crates/server/tests/engine_isolation.rs` for FR-014/SC-004: two tenants A and B each with distinct agent config + one knowledge article + a conversation; drive the responder for B; assert B's stored reply and B's `ai_generations` row contain no reference to A's data and that querying `ai_generations` filtered by tenant A returns zero rows for B's conversation. **Accept:** test present; passes after T008–T011.
 - [x] T007a [P] [US1] Prompt-determinism unit test for FR-005 (Principle IV) — a `#[cfg(test)]` test in `backend/crates/modules/ai/src/engine.rs` that calls `assemble_context` (T008) twice with identical inputs (same `row`, same history, same fixed retrieved chunks — stub/inject the chunks so retrieval is not network-dependent) and asserts the two produced `AiInput` values are byte-for-byte equal, including the knowledge-block ordering and the system-message composition. **Accept:** test present; passes after T008; fails if any nondeterminism (e.g., `HashMap` iteration order) leaks into the prompt.

### Implementation for User Story 1

 - [x] T008 [US1] Create `backend/crates/modules/ai/src/engine.rs` and add `pub mod engine;` to `backend/crates/modules/ai/src/lib.rs`. Move the **assembly** logic currently inline in `agent_responder.rs` (the block that builds `system_message`, `history`, the RAG query string, runs `knowledge::retrieval::search`, injects the knowledge block, and collects `retrieved_chunks` — current `agent_responder.rs` lines ~241–389) into a function:
  `pub struct AssembledContext { pub input: crate::AiInput, pub retrieved_chunks: Vec<knowledge::retrieval::RetrievedChunk>, pub retrieval_degraded: bool }`
  `pub async fn assemble_context(pool: &sqlx::PgPool, ai: &crate::AiService, tenant_id: Uuid, conversation_id: Uuid, row: &crate::agent_config::AgentConfigurationRow, is_platform_persona: bool, channel: &str) -> sqlx::Result<AssembledContext>`.
  Preserve current behavior exactly (same prompt composition via `agent_prompt::compose_system_message`, same variable substitution, same 20-message `recent_history`, same 800ms retrieval timeout, same `rag.retrieve` tracing span). **Accept:** `agent_responder.rs` no longer contains the moved code; `cargo build -p ai` passes; behavior identical (T006 still green once T010 lands).
 - [x] T009 [US1] In `backend/crates/modules/ai/src/engine.rs` add the provider-call + buffer function:
  `pub struct GenerationOutput { pub content: String, pub provider: String, pub model: String, pub usage: ai_providers::TokenUsage, pub finish_length: bool, pub continuation_used: bool, pub usage_record_id: Option<Uuid> }`
  `pub async fn generate(ai: &crate::AiService, ctx: crate::AiCallContext, input: crate::AiInput, provider_override: Option<(&str,&str)>) -> Result<GenerationOutput, crate::AiCallError>`.
  For US1: resolve the stream via `ai.stream_with_override(...)` when `provider_override` is `Some`, else `ai.stream(...)`; consume the `AiResultStream`, concatenating every `AiStreamEvent::Delta` into `content`; on `AiStreamEvent::Done(result)` capture provider/model/usage and set `finish_length = matches!(result finish, Length)` (see note); on `AiStreamEvent::Error{category}` return `AiCallError::Provider{..}`. Set `continuation_used=false` for US1 (US2 adds continuation). Note: `AiCallResult` currently has no `finish` field — add `pub finish: ai_providers::FinishReason` to `AiCallResult` in `service.rs` and populate it in both `run_attempts_traced` (from `completion.finish`) and the streaming `Done` mapping; default to `FinishReason::Stop` where unknown. **Accept:** `cargo build -p ai` passes; `generate` returns concatenated stream content equal to the provider's full response.
 - [x] T010 [US1] In `backend/crates/modules/ai/src/engine.rs` add the store + record entry point:
  `pub async fn run_generation(pool: &sqlx::PgPool, ai: &crate::AiService, tenant_id: Uuid, conversation_id: Uuid, trigger_message_id: Uuid, row: &crate::agent_config::AgentConfigurationRow, is_platform_persona: bool, channel: &str) -> sqlx::Result<()>`.
  Sequence: call `assemble_context` (T008); resolve provider/model override exactly as the current responder does (`row.provider`/`row.model` + `agent_config::credential_resolves`); call `generate` (T009); on success, run the existing pre-commit idempotency guard `conversations::queries::has_ai_reply_since(pool, tenant_id, conversation_id, trigger_message_id)` and if false open a tx, `insert_ai_reply_in_tx`, `insert_citations_in_tx` (only when chunks present, same mapping as current lines ~460–479), commit; then write a `generation_record::insert` row with `outcome=Success`, `response_message_id=Some(stored id)`, `provider/model`, `retrieval_chunk_count`, `retrieval_top_similarity`, `retrieval_degraded`, `attempts=1`, `continuation_used=false`, `confidence_score=None` (US4 fills this), `usage_record_id`, `latency_ms` (measure claim→store), `request_id=None`. Wrap the whole run in an `engine.generate` `tracing::info_span!` carrying `tenant_id, conversation_id, trigger_message_id, generation_id`. Keep the existing `AiCallError::NotConfigured` platform-persona auto-ack behavior (current lines ~409–431) by returning early without a success record. **Accept:** T006 passes; a success `ai_generations` row is written with the stored message id.
 - [x] T011 [US1] Refactor `backend/crates/modules/ai/src/agent_responder.rs` so `process_agent_responder_once` keeps Phase A (claim) + all gating (channel gate, conversation-state gate, open-escalation gate, escalation-rule evaluation/routing — current lines ~14–237) unchanged, then replaces Phase B/C (current lines ~239–491) with a single call to `engine::run_generation(...)` followed by the existing `DELETE FROM outbox_events WHERE id = $1`. Do not change gating semantics. **Accept:** `cargo build -p ai` passes; existing responder tests still green; T006/T007 pass.

**Checkpoint**: US1 independently testable — AI replies are generated by the engine, stored tenant-scoped with citations, and traced by a success generation record. This is the MVP.

---

## Phase 4: User Story 2 — Graceful failure (P2)

**Goal**: Bounded retry with continuation-on-partial (research §4), and on exhaustion a fallback message plus human routing, all recorded. Depends on US1 (engine exists).

**Independent Test**: quickstart.md Scenario 4 — point the tenant provider at an always-failing mock, send a customer message, and within 60s see a fallback `system` message, the conversation routed for human attention, and an `ai_generations` row `outcome='fallback'` with `error_category` set; the customer message is never dropped.

### Tests for User Story 2

 - [x] T012 [P] [US2] Integration test `backend/crates/server/tests/engine_fallback.rs`: configure a tenant whose provider base URL points at a mock that always returns a retriable error (reuse existing provider-mock harness used by ai-providers/service tests); drive the responder; assert (a) a `system`-kind fallback message exists with the platform default text, (b) an open escalation now exists for the conversation (`escalations::routing::has_open_escalation` true), (c) one `ai_generations` row `outcome='fallback'` with non-null `error_category` and `attempts >= 1`, (d) no `ai`-kind message was stored. **Accept:** present; passes after T014–T015.

### Implementation for User Story 2

 - [x] T013 [US2] Add `pub async fn insert_fallback_in_tx(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: Uuid, conversation_id: Uuid, body: &str) -> sqlx::Result<Uuid>` to `backend/crates/modules/conversations/src/queries.rs`, inserting a `kind='system'` message (copy `insert_auto_ack_in_tx` exactly, returning the id) and bumping `last_activity_at`. **Accept:** builds; returns the new message id.
 - [x] T014 [US2] Extend `engine::generate` (T009) with the bounded retry + continuation loop per [research.md](research.md) §4 and §5: attempt the streaming call up to **3 provider attempts** total, only retrying when the error `category.retriable()` is true; use exponential backoff with jitter mirroring `service.rs` `retry_delay`/`RETRY_BASE_MS`; enforce an **outer 45s deadline** from the start of `generate` (wrap the loop in `tokio::time::timeout(Duration::from_secs(45), ...)`). If a stream fails **after** partial content was received, re-issue the request with the accumulated partial appended as a trailing `ai_providers::Message{ role: Assistant, content: <partial> }` plus the fixed continuation instruction string `"Continue the previous assistant message exactly where it stopped. Do not repeat any text already written. Do not add any preamble."` and set `continuation_used=true`; stitch `partial + continuation` (trim a single boundary whitespace). Treat an empty/whitespace-only final content as a non-retriable failure. Return a distinct `Err` carrying the last `ErrorCategory` when the budget/deadline is exhausted. **Accept:** unit or integration test shows ≤3 attempts and that a mid-stream failure followed by success yields stitched content with `continuation_used=true`.
 - [x] T015 [US2] In `engine::run_generation` (T010) handle the exhausted-failure branch: in one tx, `insert_fallback_in_tx` with the platform default body `"I'm sorry — I'm having trouble responding right now. A team member will follow up shortly."` then call `escalations::routing::route_new_escalation_in_tx(&mut tx, pool, tenant_id, conversation_id, "AI assistant unavailable", &[], &[], &present_ids, Uuid::nil())` (obtain `present_ids` via `presence.present_membership_ids_async(tenant_id)` — thread the `&Arc<presence::Runtime>` through `run_generation`), commit; then write a `generation_record::insert` row `outcome=Fallback` with `error_category=Some(<last category as_str>)`, `attempts`, `response_message_id=None`. Distinguish `outcome=Failed` (the rare case where the fallback insert itself errored) per [data-model.md](data-model.md) §1. Never leave the outbox row (caller still deletes it). **Accept:** T012 passes; a total provider outage yields fallback + routing + record within the 60s bound (SC-005).

**Checkpoint**: US2 independently testable — failures degrade to a fallback message + human routing, fully recorded; US1 success path unaffected.

---

## Phase 5: User Story 3 — Streaming responses & thinking indicator (P3)

**Goal**: Broadcast `ai.message.*` events on the existing tenant SSE stream as the engine streams; enforce supersede/cancel; render thinking indicator + live text in the dashboard. Depends on US1.

**Independent Test**: quickstart.md Scenario 1 (visual) + Scenario 2 (supersede) — thinking indicator then incremental text appears; a rapid second customer message supersedes cleanly, producing exactly one final reply.

### Backend — SSE events & supersede

 - [x] T016 [US3] In `backend/crates/modules/escalations/src/model.rs` add serializable payload structs for the five event types in [contracts/ai-events-sse.md](contracts/ai-events-sse.md): `ConversationAiStarted{ conversation_id, generation_id, trigger_message_id, started_at }`, `ConversationAiDelta{ conversation_id, generation_id, text }`, `ConversationAiCompleted{ conversation_id, generation_id, message: serde_json::Value }`, `ConversationAiSuperseded{ conversation_id, generation_id, reason: String }`, `ConversationAiFailed{ conversation_id, generation_id, category: String }`. Use `#[serde(rename_all="camelCase")]` to match the contract JSON keys. **Accept:** builds; JSON field names match the contract exactly.
 - [x] T017 [US3] In `backend/crates/modules/escalations/src/presence.rs` add one enum variant `Event::ConversationAi(ConversationAiEvent)` where `pub enum ConversationAiEvent { Started(model::ConversationAiStarted), Delta(model::ConversationAiDelta), Completed(model::ConversationAiCompleted), Superseded(model::ConversationAiSuperseded), Failed(model::ConversationAiFailed) }`. **Accept:** builds; existing `Runtime::broadcast` accepts the new variant.
 - [x] T018 [US3] In `backend/crates/modules/escalations/src/events.rs` `GuardedStream::poll_next`, add match arms for `Event::ConversationAi(...)` mapping each inner variant to the SSE event names `ai.message.started|delta|completed|superseded|failed` (serialize the inner payload with `serde_json::to_string`), following the existing arm pattern (increment `self.seq`, set `.event(name).data(json).id(seq)`). These events are delivered to every connected member of the tenant (no per-member filtering). Also extend the `stream_events` doc-comment event list. **Accept:** builds; an SSE client receives `ai.message.*` frames when `broadcast` is called.
 - [x] T019 [US3] Thread an `&Arc<presence::Runtime>` into `engine::run_generation`/`generate` and broadcast per [contracts/ai-events-sse.md](contracts/ai-events-sse.md): emit `Started` before the first provider attempt; emit `Delta` for streamed text **throttled to ~4/s** (coalesce deltas in a time bucket; the stored/`Completed` content must remain the exact full text); emit `Completed{message}` where `message` is the serialized timeline `Message` object of the stored reply (build it from the same fields the timeline query returns, including `citations`; the `confidence` field is null here and is populated once US4 lands — see T026, which MUST update this construction so streamed-in messages carry the badge without a reload); emit `Failed{category}` on the exhausted-failure branch (sanitized category only — no provider detail). Generate one `generation_id` (`Uuid::new_v4()`) at run start and reuse it for the record `id` and all events. **Accept:** Scenario 1 shows started→delta→completed on `/tenant/events`; final stored content equals concatenated deltas.
 - [x] T020 [US3] Implement supersede/cancel per [research.md](research.md) §3 & [data-model.md](data-model.md) §4. (a) **Claim coalescing**: in `agent_responder.rs` after claiming a `conversation.customer_message` event, delete any older unclaimed `conversation.customer_message` outbox rows for the same conversation. (b) **Mid-stream checks**: in `engine::generate`, at most ~once per second between deltas, run two cheap indexed queries — a new `conversations::queries::has_customer_message_after(pool, tenant_id, conversation_id, trigger_message_id) -> bool` and the existing `escalations::routing::has_open_escalation`; on a newer customer message abort the stream and return a `Superseded{reason:"newer_message"}` signal; on an open escalation / `ai_handling='human'` abort and return `Superseded{reason:"escalated"}`. (c) **Pre-commit re-check**: immediately before the insert tx in `run_generation`, re-run both checks plus `has_ai_reply_since`; any hit ⇒ discard (no insert). In all supersede/cancel cases broadcast `ai.message.superseded` and write an `ai_generations` row `outcome=Superseded` or `CancelledEscalation` (partial discarded, `response_message_id=None`). Add `has_customer_message_after` to `queries.rs` (SELECT EXISTS of a customer message with `created_at >` the trigger's). **Accept:** Scenario 2 yields one `superseded` + one `success` record and exactly one final reply; Scenario 3 yields `cancelled_escalation` and no AI message.

### Backend tests — US3

 - [x] T021 [P] [US3] Integration test `backend/crates/server/tests/engine_supersede.rs`: (Scenario 2) enqueue two customer messages for one conversation before draining; drive the responder to completion; assert exactly one `ai`-kind reply and the `ai_generations` outcomes include one `superseded` and one `success`; (Scenario 3) open an escalation mid-run (simulate by inserting an open escalation before the pre-commit check) and assert `cancelled_escalation` with no AI message. **Accept:** passes.

### Frontend — streaming UI

 - [x] T022 [P] [US3] Add SSE event TypeScript types to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` matching [contracts/ai-events-sse.md](contracts/ai-events-sse.md): `AiMessageStarted`, `AiMessageDelta`, `AiMessageCompleted`, `AiMessageSuperseded`, `AiMessageFailed` (camelCase fields). **Accept:** `pnpm -C frontend exec tsc -p apps/dashboard` (or `pnpm lint`) passes.
 - [x] T023 [P] [US3] Create `frontend/apps/dashboard/src/app/shared/components/ai-thinking-indicator/ai-thinking-indicator.component.ts` — a standalone, zoneless-friendly component rendering an animated "agent is typing" indicator using Helix design tokens (copy structure/styling conventions from `shared/components/citation-list/citation-list.component.ts`). No inputs required. **Accept:** component compiles; a `.spec.ts` renders it and asserts the host element exists.
 - [x] T024 [US3] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.store.ts` with in-flight generation state per [research.md](research.md) §10: signals/state for the currently open conversation holding `{ generationId, phase: 'idle'|'thinking'|'streaming', buffer: string }`. Subscribe (RxJS, per constitution's RxJS-first rule) to `RealtimeService.events()` filtered to `ai.message.*` whose `conversationId` equals the open conversation: `started`→`thinking`; `delta`→`streaming`, append `text` to `buffer`; `completed`→append `message` to the timeline list, then clear `buffer` and set `phase='idle'`; `superseded`/`failed`→clear `buffer` and set `phase='idle'` (on `failed`, a subsequent timeline refresh surfaces the fallback message). Every terminal event (`completed`/`superseded`/`failed`) MUST return `phase` to `idle` so the thinking indicator hides when the attempt concludes (FR-009). The buffer is display-only and must never be written into the persisted timeline cache (guarantees SC-003 on reconnect). **Accept:** store spec simulates the event sequence and asserts buffer/phase transitions (including `phase==='idle'` after each terminal event) and that `completed` yields exactly one appended message.
 - [x] T025 [US3] Update `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-thread.component.ts` to render, when `phase==='thinking'`, the `<app-ai-thinking-indicator/>`, and when `phase==='streaming'`, a provisional AI message bubble bound to `buffer`. Reuse the existing message-article markup; the provisional bubble must visually match a normal AI message. **Accept:** with the store in `streaming` state the thread shows incremental text; on `completed` the provisional bubble is replaced by the persisted message (no duplicate).
 - [x] T025a [P] [US3] Reconnect/reload coherence test for SC-003 in `conversation-detail.store.spec.ts`: (a) feed `started`+several `delta` events, then simulate a reload/timeline-refetch that already contains the completed AI message, then deliver the `completed` event — assert the timeline holds exactly one copy of that message (no duplicate from buffer + refetch). (b) feed `started`+`delta`, then simulate a dropped connection (no terminal event) followed by a timeline refetch containing the persisted message — assert the buffer is discarded and exactly one persisted message remains. **Accept:** both cases assert a single, non-duplicated message; buffer never leaks into the persisted list.

**Checkpoint**: US3 independently testable — live streaming + thinking indicator, with deterministic supersede/cancel.

---

## Phase 6: User Story 4 — Staff see AI confidence (P4)

**Goal**: Compute+store a confidence score on each AI reply and expose it to staff as a band badge on a distinct AI response card. Customer-facing surfaces never carry it. Depends on US1 (uses `confidence.rs` from Foundational).

**Independent Test**: quickstart.md Scenario 6 — a knowledge-grounded reply bands higher than an ungrounded one; both cards show a badge; `messages.ai_confidence_score` is populated; no confidence data on non-staff payloads.

### Backend — US4

 - [x] T026 [US4] In `engine::run_generation` (T010/T015 success path) compute `let inputs = confidence::ConfidenceInputs { top_chunk_similarity: <retrieval_top_similarity or 0.0>, chunk_count: <retrieved_chunks.len()>, finish_length: output.finish_length, retrieval_degraded, continuation_used: output.continuation_used }; let score = confidence::confidence_score(&inputs);` and pass `Some(score)` into the message insert and the generation record's `confidence_score`. **Also update the `ai.message.completed` payload construction in `engine::run_generation` (T019)** so the serialized `message.confidence` carries `{ score, band }` (band via `confidence::confidence_band`) — otherwise a message that streamed in shows no badge until a reload (closes the C1 gap). **Accept:** stored AI messages have non-null `ai_confidence_score`; grounded > ungrounded in Scenario 6; the `ai.message.completed` event's `message.confidence` is populated (assert in the T024 store spec or T029).
 - [x] T027 [US4] Add a confidence parameter to the AI-reply insert: change `conversations::queries::insert_ai_reply_in_tx` signature to `(tx, tenant_id, conversation_id, body, ai_confidence_score: Option<f32>)` and include the column in the INSERT; update all callers (engine). **Accept:** builds; column populated.
 - [x] T028 [US4] Expose confidence on the timeline `Message` per [contracts/message-confidence.md](contracts/message-confidence.md): add `pub struct ConfidenceView { pub score: f32, pub band: String }` and `pub confidence: Option<ConfidenceView>` to `conversations::model::Message`; select `ai_confidence_score` in the timeline and detail queries (`timeline_query_in_tx`, `detail_query_in_tx`) and in the `AddMessageResponse` mapping; map to `ConfidenceView` only for `kind='ai'` rows using the band function. **Do NOT depend on the `ai` crate for the band function**: `ai` already depends on `conversations` (the engine calls `conversations::queries`), so importing `ai::confidence` into `conversations` would create a circular crate dependency that fails to compile. Instead, add a small local `fn confidence_band(score: f32) -> &'static str` inside `conversations` duplicating the exact thresholds (`high >= 0.70`, `medium >= 0.40`, else `low`); message-confidence.md documents the band as server-owned, so this deliberate 3-line duplication is expected. **Accept:** timeline JSON shows `confidence:{score,band}` on AI messages, `null`/absent otherwise; `conversations` does not import `ai`; OpenAPI schema updated.
 - [x] T029 [P] [US4] API test `backend/crates/server/tests/message_confidence.rs`: after an AI reply, GET the timeline and assert the AI message carries `confidence.band` in `{high,medium,low}` and a `score` in `[0,1]`, and that a customer/system message carries no confidence. **Accept:** passes.

### Frontend — US4

 - [x] T030 [P] [US4] Create `frontend/apps/dashboard/src/app/shared/components/ai-confidence-badge/ai-confidence-badge.component.ts` — standalone component with an input `band: 'high'|'medium'|'low'` rendering a Helix-token chip (distinct color per band; `low` most prominent). Add a `.spec.ts` asserting the rendered label/class per band. **Accept:** component + spec compile and pass.
 - [x] T031 [US4] Add `confidence?: { score: number; band: 'high'|'medium'|'low' }` to the `Message` model in `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` (and any conversations feature model that mirrors it). **Accept:** `pnpm lint` passes.
 - [x] T032 [US4] In `conversation-thread.component.ts`, render `kind==='ai'` messages as a visually distinct **AI response card** (extend existing styling) and, when `message.confidence` is present, render `<app-ai-confidence-badge [band]="message.confidence.band"/>`. The badge must not render for non-AI messages and must never appear on any customer-facing view. **Accept:** Scenario 6 — AI cards show badges; customer/human/system messages show none.

**Checkpoint**: US4 independently testable — confidence stored, banded, and shown to staff only.

---

## Phase 7: User Story 5 — Conversation summary (P5)

**Goal**: On-demand, staff-only conversation summary via a new endpoint; nothing persisted. Depends on US1 (provider plumbing) only.

**Independent Test**: quickstart.md Scenario 7 — request a summary of a ~dozen-message conversation; a concise summary renders ≤10s; empty conversation → 422; provider failure → non-blocking error.

### Backend — US5

 - [x] T033 [US5] Add `pub async fn summary_history(pool: &sqlx::PgPool, tenant_id: Uuid, conversation_id: Uuid, limit: i64) -> sqlx::Result<Vec<(String,String)>>` to `conversations::queries` returning the last `limit` messages of kinds `customer|ai|reply|system` (exclude `note`) ordered `created_at ASC, seq ASC`, tenant-scoped. **Accept:** builds; returns tenant-scoped rows.
 - [x] T034 [US5] Create `backend/crates/modules/ai/src/summary.rs` (add `pub mod summary;` to `lib.rs`) implementing the handler for `POST /tenant/conversations/{id}/summary` per [contracts/conversation-summary.md](contracts/conversation-summary.md): resolve `TenantContext` + conversation read authz (same gate as `GET /tenant/conversations/{id}`; 404 if not in tenant); load `summary_history(..., 50)`; if empty return `422`; build the summary request using the **verbatim** `SUMMARY_SYSTEM_PROMPT` constant and the exact message-turn mapping specified in [research.md](research.md) §8 (do not paraphrase the prompt; `customer`→User, `ai`/`reply`→Assistant, `system`→inline `[system: …]` on a User turn); call `AiService::complete` using the same provider/model resolution chain as the engine (tenant override if `credential_resolves`, else platform default); return `200 { summary, generatedAt, messageCount }`; map provider failure → `502`, not-configured → `503`, using the standard `ErrorEnvelope`. Add `#[utoipa::path(...)]` annotation. **Accept:** endpoint returns the contract shape; error statuses per contract.
 - [x] T035 [US5] Register the route in `backend/crates/server/src/router.rs` alongside the other `conversations` routes (use the `routes!()` co-registration macro so it appears in OpenAPI), path `POST /tenant/conversations/{id}/summary`. **Accept:** route reachable; appears in generated OpenAPI.
 - [x] T036 [P] [US5] API test `backend/crates/server/tests/conversation_summary.rs`: 200 with `summary` non-empty for a populated conversation; 422 for an empty conversation; 404 for another tenant's conversation id; authz — a role without conversation read access gets 403. **Accept:** passes.

### Frontend — US5

 - [x] T037 [P] [US5] Add `requestSummary(conversationId: string): Observable<{ summary: string; generatedAt: string; messageCount: number }>` to `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` (POST, RxJS Observable, no `firstValueFrom`). **Accept:** `pnpm lint` passes.
 - [x] T038 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-summary.component.ts` — standalone panel with a "Summarize" trigger that calls `requestSummary`, shows a loading state, renders the returned summary, and on error shows a non-blocking inline error (thread stays usable). Wire it into `conversation-detail.component.ts` (staff-only placement). Add a `.spec.ts` covering success and error states. **Accept:** Scenario 7 — summary renders ≤10s; provider error shows non-blocking message; not visible to customers.

**Checkpoint**: US5 independently testable — staff-only on-demand summary.

---

## Phase 8: Polish & Cross-Cutting

 - [x] T039 [P] Update `backend/crates/server/tests/openapi_coverage.rs` and `backend/crates/server/tests/openapi_valid.rs` to include the new summary route and the additive `confidence` message field; ensure coverage test passes. **Accept:** both OpenAPI tests green.
 - [x] T040 [P] Run the full quickstart validation ([quickstart.md](quickstart.md) Scenarios 1–7) against a local stack; record any deviation as a follow-up. **Accept:** all seven scenarios behave as documented.
 - [x] T041 Run all quality gates: `cd backend && cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` and `cd frontend && pnpm lint && pnpm format:check && pnpm ng test dashboard --watch=false`. Fix any failures. **Accept:** every command exits 0.

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T005)** must finish before any user story.
- **US1 (T006–T011)** depends only on Foundational. **This is the MVP.**
- **US2 (T012–T015)** depends on US1 (engine exists).
- **US3 (T016–T025)** depends on US1. T016→T017→T018 sequential (same modules); T019/T020 depend on T016–T018; frontend T022–T025 depend on backend events existing (T016–T019) but T022/T023 are independent scaffolding.
- **US4 (T026–T032)** depends on US1 and Foundational T004 (`confidence.rs`). Independent of US3.
- **US5 (T033–T038)** depends on US1 only. Independent of US2/US3/US4.
- **Polish (T039–T041)** last.

Story independence: US2, US4, US5 can each be built directly on top of US1 in any order. US3 is the only one that also touches the escalations SSE module.

## Parallel Opportunities

- Foundational: **T003, T004** in parallel (different new files); T005 after (edits `service.rs`).
- US1 tests **T006, T007** in parallel; implementation T008→T009→T010→T011 sequential (same file `engine.rs` / `agent_responder.rs`).
- US3 frontend **T022, T023** in parallel with backend T016–T021.
- US4 **T029, T030** in parallel; US5 **T036, T037** in parallel.
- Polish **T039, T040** in parallel; T041 last.

## MVP Scope

**US1 (T001–T011)** = smallest shippable increment: the engine generates, stores, and traces AI replies tenant-scoped. Everything after (failure fallback, live streaming, confidence badge, summary) is additive on top of it.

## Implementation Strategy

Build in priority order US1 → US2 → US3 → US4 → US5, shipping/validating each story's checkpoint before starting the next. Because every story after US1 is additive and independently testable, work can stop after any checkpoint with a coherent, releasable system.

---

## Phase 9: Convergence

**Purpose**: Close remaining gaps found by assessing the implemented code against the spec, plan, and constitution. Everything else specified for this feature (engine assembly/generation/store, tenant isolation, streaming SSE events, thinking indicator, supersede/cancel, retry+continuation, fallback+routing, confidence storage/badge, on-demand summary endpoint, claim coalescing) is implemented and present in code — only the item below remains.

 - [X] T042 Implement the prompt-determinism test that T007a required but left as an empty stub (`backend/crates/modules/ai/src/engine.rs:883–888` — `test_assemble_context_determinism_placeholder()` currently calls nothing and asserts nothing). Replace it with a real test that proves FR-005 / Constitution Principle IV determinism: run the assembly path twice with identical inputs (same `AgentConfigurationRow`, same conversation history, same fixed retrieved chunks) and assert the two produced prompts are byte-for-byte equal, including knowledge-block ordering and system-message composition. Note the two design constraints the stub author hit: (a) `AiInput` (`service.rs:22`) derives only `Clone, Debug` — add `#[derive(PartialEq)]` (its `messages` field is `Vec<ai_providers::Message>`, which already derives `PartialEq`) or compare via a stable serialization; (b) `assemble_context` makes 5 DB calls (`load_bootstrap`, `recent_history`, `customer_display_name`, `fetch_tenant`, `retrieval::search`), so a pure `#[cfg(test)]` unit test cannot drive it as-is — either implement this as a Postgres-backed test in `backend/crates/server/tests/` (mirroring `engine_respond.rs`), or refactor the deterministic prompt-composition core (system-message + knowledge-block assembly, excluding the DB reads) into a pure function and unit-test that. **Accept:** the test calls the real assembly code twice and asserts byte-for-byte prompt equality; it fails if any nondeterminism (e.g., `HashMap` iteration order) leaks into the prompt; `cargo test -p ai` (and the server test if used) passes. (partial — per FR-005 / Constitution IV & VII)
