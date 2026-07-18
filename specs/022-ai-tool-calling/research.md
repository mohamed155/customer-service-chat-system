# Research: AI Tool Calling

**Feature**: 022-ai-tool-calling | **Date**: 2026-07-18

## §1 Provider-agnostic tool calling across OpenAI / Anthropic / Gemini

**Decision**: Extend the `ai-providers` contract once, mapped per vendor:

- `ChatRequest.tools: Vec<ToolSpec>` where `ToolSpec { name, description, input_schema: serde_json::Value }` (JSON Schema object, the de-facto interchange format all three vendors accept).
- `ToolCall { id, name, arguments: serde_json::Value }` produced by completions/streams.
- Conversation replay: `Message` gains an enum-shaped content extension — assistant messages may carry `tool_calls`, and a new `Role::Tool` message carries `{ tool_call_id, content }` results. Mapping: OpenAI → `tool_calls` array + `role:"tool"` messages; Anthropic → `tool_use` / `tool_result` content blocks; Gemini → `functionCall` / `functionResponse` parts (Gemini call ids are synthesized from name + index since the API has none).
- `FinishReason::ToolUse` added; `ChatCompletion.tool_calls: Vec<ToolCall>`.
- Streaming: provider SSE decoders **accumulate** tool-call argument deltas internally and emit a single `StreamEvent::ToolCall(ToolCall)` per completed call, followed by `Done { finish: ToolUse }`. The engine (and everything downstream) never sees partial tool-call JSON.

**Rationale**: One contract change implements Principle IV's provider independence; JSON Schema input specs and complete-call events are the intersection all three vendors support cleanly. Accumulating argument deltas in the decoder keeps the engine loop simple and avoids streaming malformed partial JSON anywhere.

**Alternatives considered**: (a) Non-streaming `complete()` whenever tools are enabled — rejected: loses 021's streaming UX for the (common) final answer turn. (b) Prompt-engineered pseudo-tools (ask the model to emit JSON) — rejected: fragile parsing, no vendor-side validation, violates the spirit of deterministic tool mediation. (c) Streaming partial argument deltas to the engine — rejected: no consumer needs partial args; complexity without value.

## §2 Two-phase approval resumption mechanism

**Decision**: Reuse the outbox → responder-worker pipeline. When a decision is reached (approve / deny / expire / cancel-on-escalation), the decider writes the `tool_requests` status transition and emits an `ai.tool_decision` outbox event in the same transaction (`emit_tool_decision_in_tx`). The existing `agent_responder` worker claims it like any other event and runs a follow-up generation whose assembled context includes the tool request outcome (and result, if executed). Expiry is handled by a periodic sweep (≤ 30 s cadence, piggybacked on the existing worker loop) that transitions overdue `awaiting_approval` rows to `expired` and emits the same event.

**Rationale**: The claim/coalesce/retry semantics 021 built (single claim via `FOR UPDATE SKIP LOCKED`, at-most-one in-flight generation per conversation, supersede rules) all apply unchanged to decision-triggered generations; a second trigger path (direct spawn from the HTTP handler) would duplicate that machinery and break the one-worker invariant.

**Alternatives considered**: (a) Execute + generate synchronously inside the decide HTTP handler — rejected: couples request latency to provider latency, bypasses supersede/claim semantics. (b) A dedicated tool-decision queue table — rejected: `outbox_events` already provides exactly this with worker plumbing in place.

## §3 Credential sealing for tenant-defined tools

**Decision**: Move `modules/ai/src/crypto.rs` (AES-256-GCM `MasterKey` envelope, keyed by `APP_AI_KEY_ENCRYPTION_KEY`) into the `ai-providers` crate as `ai_providers::crypto`, leaving a re-export shim in `ai`. The `tools` module uses it to seal tenant endpoint credentials at write time and unseal only inside the executor at call time. Credentials are write-only through the API (never echoed; updates replace), never logged, never serialized into `tool_requests`, and never enter AI context.

**Rationale**: `ai` will depend on `tools` (engine → executor), so `tools` cannot depend on `ai` — the sealing code must live below both. `crypto.rs` already depends on `ai_providers::SecretKey`, so relocating it into `ai-providers` requires no new workspace crate and no new key-management surface (same master key, same envelope format).

**Alternatives considered**: (a) New `crypto` workspace crate — workable but adds a crate for one file; revisit if a third consumer appears. (b) Duplicate the sealing code in `tools` — rejected: two copies of key handling is a security-review liability. (c) Plaintext + DB-level encryption — rejected: violates Principle III and SC-008.

## §4 Safe execution of tenant-defined external endpoints

**Decision**: The executor calls tenant endpoints as `POST {endpoint_url}` with a JSON body `{ tool, arguments, conversation_id, request_id }` and the tenant-configured credential in an `Authorization` header. Guardrails: HTTPS-only URLs; deny endpoints resolving to loopback/private/link-local ranges (SSRF guard at registration **and** at call time); 15 s total timeout; 1 MiB response cap; response must be JSON (else recorded as failed with a sanitized error). Raw failure details are stored in `tool_requests.error` (sanitized via the existing `sanitize_error_detail`) — visible to staff, never to customers, never containing credentials.

**Rationale**: The platform is making outbound calls with tenant secrets on behalf of an LLM — the guardrails bound blast radius (SSRF, giant responses, hung sockets) while keeping the tenant contract simple (one POST shape).

**Alternatives considered**: (a) Arbitrary method/URL templating per tool — rejected for v1: larger config surface and attack surface; the fixed POST contract covers the use case. (b) Signature-based (HMAC) auth instead of bearer credentials — good future option, additive later; header credential matches what most tenant systems accept today.

## §5 Bounds and defaults

**Decision**: Platform defaults — max **5** tool calls per generation (chain cutoff recorded on the generation, engine then forces a final answer); built-in execution timeout **10 s**; external endpoint timeout **15 s**; approval window **5 minutes** (`expires_at` stamped at request creation); expiry sweep cadence ≤ 30 s. All constants live in module-level config with env overrides, values asserted in tests.

**Rationale**: 5 steps covers realistic lookup→act chains without runaway cost; 5 minutes matches the escalation-grade attention span of an active inbox (approvals surface over live SSE, not email); timeouts sit inside the engine's existing 45 s outer deadline.

**Alternatives considered**: Per-tenant configurable bounds — deferred; spec only requires platform-set values (planning decision), and per-tenant knobs are additive schema later.

## §6 At-most-once execution and single effective decision

**Decision**: The `tool_requests.status` state machine is enforced by conditional UPDATEs: `UPDATE ... SET status='approved', decided_by=..., decided_at=now() WHERE id=$1 AND tenant_id=$2 AND status='awaiting_approval'` — zero rows affected means the request was already settled, and the API returns the settled state (idempotent-safe decide). Execution transitions `approved → executing` the same way before any side effect, guaranteeing at-most-once even with concurrent workers. Side-effecting tools are never auto-retried (a failed execution is terminal); the auto-approved read-only chain may retry only within the engine's existing attempt budget.

**Rationale**: Status-gated writes are the established pattern in this codebase (escalation claims, outbox claims) and give the FR-014 single-decision guarantee without advisory locks.

**Alternatives considered**: Per-tenant advisory locks (014 pattern) — heavier than needed; row-level conditional update suffices since contention is per-request.

## §7 v1 built-in tools

**Decision**: Ship two built-ins exercising both classifications through existing module APIs:

- `lookup_customer` (auto-approved, read-only): returns the conversation's customer profile (name, contact fields, recent conversation count) via `customers` module public queries.
- `update_customer_contact` (approval-required, side-effecting): updates a customer contact field (e.g., email/phone) via the `customers` module, demonstrating the full two-phase approval flow.

**Rationale**: Both are genuinely useful to a support AI, need no new external dependencies, stay inside existing module boundaries, and together exercise every framework path (auto chain, approval, denial, audit, timeline).

**Alternatives considered**: Knowledge-search tool — rejected: RAG already injects knowledge (020); a tool duplicate would confuse retrieval semantics. Conversation-close/escalate tools — rejected for v1: escalation already has dedicated engine pathways; overlapping them with tools invites conflicting state transitions.

## §8 Timeline delivery and SSE events

**Decision**: Tool activity is stored in `tool_requests` and delivered two ways: (a) batched with conversation detail via `GET /tenant/conversations/{id}/tool-activity` (paged, newest-first, joined in one query — no N+1); (b) live over the existing tenant SSE stream as a new `Event::ConversationTool` variant with payloads for `tool.request.created` (includes approval-required flag), `tool.request.updated` (status transitions incl. decisions and results-ready), keeping the 021 event-shape conventions. The approval card and timeline entries update from the same events.

**Rationale**: Matches 021's precedent exactly (dedicated variant on the escalations-owned runtime, documented boundary trade-off, future `realtime` extraction path unchanged).

**Alternatives considered**: Embedding tool events inside `ConversationAi` payloads — rejected: tool lifecycle outlives generations (pending approvals persist after the generation ends), so it needs its own event identity.
