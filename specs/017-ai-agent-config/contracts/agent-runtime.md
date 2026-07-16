# Runtime Contract: Agent Responder Pipeline

**Feature**: 017-ai-agent-config | **Date**: 2026-07-16

The behavioral contract between the conversations module (event producer), `modules/ai` (responder), and the escalations module (handoff sink). Internal — no HTTP surface.

## Event: `conversation.customer_message`

Emitted by `conversations` in the **same transaction** as every `customer`-kind message insert (`add_message` and the initial message of `create_conversation`), via the module's existing `outbox.rs`:

```json
{
  "event_type": "conversation.customer_message",
  "aggregate_type": "conversation",
  "aggregate_id": "<conversation uuid>",
  "tenant_id": "<tenant uuid>",
  "payload": { "conversation_id": "…", "message_id": "…", "channel": "web_chat" }
}
```

Conversations gains no AI knowledge — it announces a domain fact (Constitution I).

## Consumer: agent responder worker (`modules/ai`)

Mirrors the escalation outbox worker pattern: `run_agent_responder_worker` (server-spawned loop) claiming unprocessed events with the shared claim semantics, plus `process_agent_responder_once` for deterministic tests.

### Pipeline, per event (fixed order)

1. **Load** the tenant's live `agent_configurations` row.
   - No row → **unconfigured-fallback branch** (R13, FR-004a/b):
     - `ai_handling = 'human'` → mark processed, done (escalation already exists from decision time).
     - `ai_handling IS NULL` → if the conversation has no `system`-kind message yet, insert the one-time auto-acknowledgment (`kind='system'`, fixed platform text) and bump last-activity; mark processed. No AI reply either way — the conversation awaits the staff decision.
     - `ai_handling = 'platform_ai'` → continue the pipeline from step 3 using the **platform default persona** (code-constant name/tone/prompt, no tenant rules) and 015 AI-layer resolution at step 6; the baseline human-request catalog (step 4a) still applies — it is a property of any AI participation, not of tenant rules. If the AI layer no longer resolves, fall through to the auto-ack-once-then-wait behavior (spec edge case).
   - Row exists → `ai_handling` is ignored entirely (FR-004c) and the pipeline continues below.
2. **Gate on channel**: event channel ∉ `enabled_channels` → mark processed, done (FR-012).
3. **Gate on conversation state**: conversation is `resolved`/`closed`, or already has an open escalation → mark processed, done (never talk over a human handoff).
4. **Evaluate escalation rules** against the triggering customer message body, in order:
   a. **Baseline first, always**: built-in human-request phrase catalog (code constant, case-insensitive substring match). Fires even with zero configured rules and cannot be disabled (FR-011).
   b. Tenant rules in stored order: `human_request` rules match the same catalog (their value over baseline: custom `name` in the routing reason and `required_skill_ids` routing); `topic_keywords` rules match any keyword case-insensitively.
   c. **First match wins.** On match: create an escalation through the escalations module's transactional routing entry (`route_new_escalation_in_tx` path) with reason = rule name (baseline: fixed reason string), `required_skill_ids` = the rule's live skills (broken refs dropped at fire time), and mark the event processed. **No AI reply is generated for this message.**
5. **Compose the prompt** (deterministic — R12, Constitution IV). System message, fixed order, byte-stable for identical config:
   1. `system_prompt` verbatim;
   2. tone directive from the fixed tone→directive map;
   3. business rules block: header line + rules numbered in stored order (omitted entirely when the list is empty);
   4. platform guardrail line: the agent identifies as "<name>", an AI assistant, and never claims to be human.
   Conversation history (bounded: most recent N=20 `customer`/`reply`/`ai` messages, chronological; `note` kind excluded — internal) maps to user/assistant messages. Customer content never enters the system message.
6. **Resolve provider/model**: agent override if set **and** its provider credential resolves; otherwise the 015 AI-layer resolution (tenant → platform default). Override unusable → fall back exactly as FR-008 requires (the GET surface flags it `stale`). Nothing resolvable → mark processed, no reply (AI effectively unconfigured; usage-record and error semantics stay 015's).
7. **Call** `AiService::complete` (blocking completion in v1 — the dashboard timeline has no streaming surface; widget streaming is a later feature). 015 owns retries/failover/usage recording.
8. **Insert the AI reply**: `messages` row `kind='ai'`, body = completion text, membership ids NULL; bump conversation `last_activity_at`; emit the module's existing message-created side effects so the dashboard timeline updates like any other message.
9. **Mark processed.** Vendor failure after 015's retries: log/trace (structured, no content), mark processed **without** a reply — customers get silence + existing human flow rather than an error message in v1; the failure is visible in usage records and traces (Constitution VI).

### Config binding

The pipeline reads the agent row once at step 1. A PUT landing after step 1 affects the **next** event, satisfying FR-016/SC-002 ("next AI-generated response") — in-flight generation completes under the prior config (spec edge case).

### Idempotency & ordering

- One event = at most one AI reply, one auto-acknowledgment, or one escalation. Replays (crash between insert and mark-processed) are tolerated by idempotency guards: skip if an `ai` message or open escalation already exists for the conversation newer than the triggering message; the auto-ack guard is the existence of any `system` message in the conversation.
- Events for the same conversation process in `created_at` order (single-claimer semantics of the outbox pattern).

## Determinism guarantees (testable)

- Same agent config + same transcript → identical composed message array (unit test byte-compares).
- Rule evaluation is pure string matching — no LLM involvement in the escalate/respond decision (R7).
- Tone directives and the human-request phrase catalog are code constants; changing them is a reviewed code change.

## Observability

Every pipeline run emits structured trace events (request-id propagated end-to-end into 015's vendor call and usage record): event id, tenant, conversation, gate outcomes, rule fired (name/id) or none, provider/model resolved, latency. Message content and system prompts never appear in logs or traces (015 invariant upheld).
