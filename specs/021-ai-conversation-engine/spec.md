# Feature Specification: AI Conversation Engine

**Feature Branch**: `021-ai-conversation-engine`

**Created**: 2026-07-18

**Status**: Draft

**Input**: User description: "AI Conversation Engine — Allow AI to respond to customer messages. Scope: conversation context assembly, prompt construction, RAG context injection, AI response generation, streaming response support, AI confidence metadata, error handling, fallback behavior. Backend: build AI response service, load tenant agent config, load relevant conversation history, load retrieved knowledge chunks, call selected provider, store AI response as message. Frontend: AI response card, AI confidence badge, AI thinking indicator, conversation summary component. Acceptance: AI can respond to customer messages; responses are stored in conversation history; AI uses tenant-specific configuration; AI does not access data from other tenants."

## Clarifications

### Session 2026-07-18

- Q: Once a tenant's AI agent is configured and active, which conversations does the AI handle automatically? → A: AI-first by default — every new customer conversation is AI-handled from the start; the AI stops responding only when the conversation is escalated to or claimed by a human via the existing handoff flow.
- Q: A customer sends a second message while the AI is still generating a response to the first — what happens? → A: Supersede and regenerate — the in-flight generation is cancelled and its partial output discarded; one new generation runs with context including both messages, producing a single coherent answer to the customer's latest input.
- Q: How is AI response confidence represented and stored in v1? → A: Bands plus stored score — the staff-facing badge shows a category (high / medium / low) derived from an underlying numeric score (0–1) that is stored with the response, so future threshold-based automation requires no data change. The derivation method is a planning decision.
- Q: What happens to already-streamed partial content when generation fails mid-stream? → A: Attempt to resume/complete — the system first tries to continue the interrupted response from where it stopped (within the bounded retry budget). If resumption succeeds, the completed message stands. If resumption fails, the standard retry/fallback path applies and the partial content is removed — a truncated answer is never left standing as the final response.
- Q: What happens to an in-flight AI generation when the conversation is escalated to / claimed by a human? → A: Cancel cleanly — the generation is cancelled and its partial output discarded; the human takes over with no AI message posted.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A customer message receives an AI response (Priority: P1)

A customer sends a message in a conversation that is being handled by the tenant's AI agent. The system gathers everything the agent needs to answer well — the tenant's configured agent identity and instructions, the relevant recent history of this conversation, and the most relevant passages from the tenant's published knowledge base — composes a request to the tenant's selected AI provider, and produces a response. The response appears in the conversation as a message from the AI agent and is permanently part of the conversation history, exactly like any other message.

**Why this priority**: This is the feature's reason to exist — the complete ask → assemble → generate → store → display loop. Every prior AI feature (provider abstraction, agent configuration, prompt management, knowledge retrieval) converges here into the first end-to-end automated customer answer. Without this story, nothing else in this feature has anything to attach to.

**Independent Test**: Configure a tenant's AI agent, publish a knowledge article containing a distinctive fact, send a customer message asking about that fact in an AI-handled conversation, and verify a response attributed to the AI agent appears in the conversation, reflects the agent's configured persona and the article's content, and is still present after reloading the conversation.

**Acceptance Scenarios**:

1. **Given** a tenant with a configured AI agent and an AI-handled conversation, **When** a customer message arrives, **Then** the system generates a response using the tenant's agent configuration (persona, tone, instructions, selected provider and model) and posts it to the conversation attributed to the AI agent.
2. **Given** a conversation with prior messages, **When** the AI generates a response, **Then** the response is informed by the relevant recent conversation history, so follow-up questions ("what about the second option?") are answered in context.
3. **Given** the tenant has published, indexed knowledge relevant to the customer's question, **When** the AI responds, **Then** the retrieved knowledge passages inform the answer and the response carries the citations defined by the knowledge retrieval feature.
4. **Given** no relevant knowledge exists for the question, **When** the AI responds, **Then** it answers from the agent's instructions and conversation context alone, without fabricated citations.
5. **Given** an AI response has been generated, **When** any participant or tenant staff member views the conversation later (including after reload), **Then** the AI response appears in its correct chronological position in the stored history.
6. **Given** two tenants each with their own agent configuration, conversation history, and knowledge base, **When** the AI responds in a conversation belonging to one tenant, **Then** no configuration, history, or knowledge from any other tenant influences or appears in the response.
7. **Given** a conversation that has been escalated to or claimed by a human agent, **When** a customer message arrives, **Then** the AI does not generate a response for it.

---

### User Story 2 - The conversation degrades gracefully when generation fails (Priority: P2)

A customer sends a message, but the AI provider is unavailable, times out, or returns an unusable result. Instead of the conversation silently stalling, the system retries within a bounded window, and if generation still fails, posts a tenant-appropriate fallback message to the customer and flags the conversation for human attention through the existing escalation and routing flow. Tenant staff can see that the AI failed and why.

**Why this priority**: An AI agent that sometimes answers and sometimes leaves customers hanging is worse than no AI agent — trust in the whole platform depends on every customer message reaching a resolution path. Reliability is the second thing that must be true after "it responds at all."

**Independent Test**: Simulate a provider failure for a tenant, send a customer message to an AI-handled conversation, and verify the customer receives a fallback message within the expected time, the conversation enters the human routing flow, and the failure is recorded and inspectable.

**Acceptance Scenarios**:

1. **Given** the tenant's selected provider fails or times out, **When** a customer message awaits a response, **Then** the system retries a bounded number of times before giving up — it never retries indefinitely and never drops the customer message.
2. **Given** retries are exhausted, **When** generation is abandoned, **Then** a fallback message is posted to the conversation telling the customer their request will be handled, and the conversation is routed for human attention via the existing escalation flow.
3. **Given** a generation failure occurred, **When** platform operators or tenant staff inspect the conversation, **Then** the failure (what failed, when, and why) is recorded and distinguishable from a successful AI response.
4. **Given** generation fails partway through a streamed response, **When** the failure occurs, **Then** the system attempts to resume and complete the interrupted response within the bounded retry budget; if resumption fails, the partial content is removed and the fallback flow takes over — a truncated answer is never left standing as the final response.
5. **Given** the AI engine is degraded, **When** customers continue messaging, **Then** human-handled conversations and all non-AI functionality remain unaffected.

---

### User Story 3 - Responses stream in as they are generated (Priority: P3)

While the AI composes its answer, viewers of the conversation see that the agent is working — a thinking indicator appears as soon as the customer message is picked up, and the response text then appears progressively as it is generated rather than arriving all at once after a long silence. When generation completes, the streamed text becomes the stored message, identical to what was displayed.

**Why this priority**: Streaming transforms perceived responsiveness — a 10-second wait with visible progress feels acceptable; a 10-second dead silence feels broken. It builds directly on Story 1's generation loop and is required by the platform's performance principles, but a complete-then-post response is still a viable (if slower-feeling) MVP without it.

**Independent Test**: Send a customer message that produces a long AI answer and observe the conversation view: a thinking indicator appears promptly, response text arrives incrementally, and the final stored message matches the fully streamed text after reload.

**Acceptance Scenarios**:

1. **Given** a customer message has been picked up for AI handling, **When** generation begins, **Then** conversation viewers see a thinking indicator until the first response content arrives.
2. **Given** generation is producing output, **When** viewers watch the conversation, **Then** the response text appears progressively as it is generated.
3. **Given** a streamed response completes, **When** the conversation is reloaded, **Then** the stored message content is identical to the final streamed content.
4. **Given** a viewer joins or refreshes mid-generation, **When** they open the conversation, **Then** they see a coherent state (the in-progress indicator or the completed message), never a corrupted or duplicated response.

---

### User Story 4 - Staff see how confident the AI was (Priority: P4)

A tenant agent or supervisor reviewing an AI-handled conversation sees each AI response presented as a distinct AI response card, carrying a confidence badge that summarizes how confident the system was in that answer (for example high / medium / low). Low-confidence answers stand out, helping staff decide which conversations deserve review or human follow-up. Confidence is visible to tenant staff only — customers never see it.

**Why this priority**: Confidence metadata turns the AI from a black box into something supervisable, but it refines responses that already exist and fail-safes that already work; it depends on Stories 1–2 and adds oversight value rather than core capability.

**Independent Test**: Trigger AI responses of varying quality (e.g., one well-grounded in knowledge, one with no relevant knowledge), view the conversation as tenant staff, and verify each AI response displays a confidence badge, that the customer-facing rendering carries no confidence information, and that confidence is stored with the message.

**Acceptance Scenarios**:

1. **Given** an AI response is generated, **When** it is stored, **Then** confidence metadata is recorded with it.
2. **Given** tenant staff view an AI-handled conversation, **When** AI responses are displayed, **Then** each appears as a visually distinct AI response card with a confidence badge, clearly distinguishable from customer and human-agent messages.
3. **Given** a customer views the conversation, **When** AI responses are displayed, **Then** no confidence information is exposed to them.
4. **Given** responses with different confidence levels, **When** staff scan a conversation, **Then** low-confidence responses are visually distinguishable from high-confidence ones at a glance.

---

### User Story 5 - Staff catch up via a conversation summary (Priority: P5)

A tenant agent opening a long conversation — for example one just escalated from the AI — can request a concise summary of what has happened so far: what the customer wants, what has been tried or answered, and where things stand. The summary appears in the conversation view for staff only and lets the agent take over without reading the entire transcript.

**Why this priority**: The summary is a productivity multiplier for the human side of the human-AI handoff, but it consumes the engine rather than defining it — it can ship any time after Story 1 exists.

**Independent Test**: Build a conversation with a dozen mixed customer/AI messages, request a summary as tenant staff, and verify a concise, accurate summary of the conversation is displayed within a few seconds and is not visible to the customer.

**Acceptance Scenarios**:

1. **Given** a conversation with message history, **When** a tenant staff member requests a summary, **Then** a concise summary of the conversation so far is generated and displayed to them.
2. **Given** the summary is generated, **When** it is displayed, **Then** it reflects only this conversation's content for this tenant, and is visible to tenant staff only — never to the customer.
3. **Given** summary generation fails, **When** the staff member requested it, **Then** they see a clear, non-blocking error and the conversation view otherwise functions normally.

---

### Edge Cases

- A customer sends a second message while the AI is still generating a response to the first: the in-flight generation is superseded — cancelled with partial output discarded — and a single new generation answers the combined context (see FR-016); never two interleaved answers or a stale reply posted after a newer message.
- The conversation history is longer than what can be included in a single generation: the system includes the most relevant bounded window and older content is omitted predictably rather than failing.
- The tenant's AI agent is deactivated, deleted, or unconfigured mid-conversation: arriving customer messages follow the unconfigured-tenant behavior defined by the agent-configuration feature instead of erroring.
- The provider returns an empty or unusable response payload: treated as a generation failure (retry, then fallback) rather than posting an empty message.
- A conversation is escalated to a human while a generation is in flight: the in-flight generation is cancelled cleanly and its partial output discarded — no AI message is posted after the handoff, and the AI does not respond to subsequent messages.
- The viewer's connection drops mid-stream: the response is still generated and stored server-side; on reconnect or reload the viewer sees the completed message.
- Knowledge retrieval itself fails (as opposed to returning nothing): the AI proceeds without knowledge grounding rather than failing the whole response, and the degradation is recorded.
- Two tenant staff members watch the same conversation during generation: both see consistent streaming state.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: For tenants with a configured, active AI agent, every new customer conversation MUST be AI-handled from its start: the system MUST automatically generate an AI response when a customer message arrives, and MUST stop generating responses once the conversation is escalated to or claimed by a human agent.
- **FR-002**: Response generation MUST use the conversation's tenant's active AI agent configuration — identity, tone, system prompt/instructions, and selected provider and model — as defined by the agent-configuration feature.
- **FR-003**: Response generation MUST incorporate a bounded window of the conversation's relevant recent history so that responses are contextual to the ongoing exchange.
- **FR-004**: Response generation MUST incorporate the knowledge passages retrieved for the customer message by the knowledge-retrieval pipeline when any are available, and MUST proceed without them (with the degradation recorded) when retrieval returns nothing or fails.
- **FR-005**: Prompt construction MUST be deterministic: the same agent configuration, conversation context, and retrieved knowledge MUST always produce the same prompt structure.
- **FR-006**: The engine MUST invoke the tenant's selected provider exclusively through the existing provider abstraction; supporting an additional provider MUST require no changes to the engine.
- **FR-007**: Every completed AI response MUST be stored as a message in the conversation history, attributed to the AI agent, retrievable thereafter exactly like any other message, and carrying the citations defined by the knowledge-retrieval feature when knowledge was used.
- **FR-008**: The system MUST support streaming delivery: response content MUST be observable progressively by conversation viewers as it is generated, and the final stored message MUST equal the fully streamed content.
- **FR-009**: Conversation viewers MUST see an in-progress ("thinking") indication from when a customer message is picked up for AI handling until response content begins to arrive or the attempt concludes.
- **FR-010**: Each AI response MUST carry confidence metadata recorded at generation time: an underlying numeric score (0–1) stored with the response and a derived category (high / medium / low). The category MUST be displayed to tenant staff as a badge on the AI response card; no confidence information may be exposed to customers.
- **FR-011**: AI responses MUST be visually distinguishable from customer and human-agent messages wherever tenant staff view conversations.
- **FR-012**: On provider failure, timeout, or an unusable response, the system MUST retry within bounded limits; the customer's message MUST never be silently dropped and generation MUST never retry indefinitely. For failures that interrupt a partially streamed response, the retry MUST first attempt to resume and complete the interrupted response; if resumption fails within the retry budget, the partial content MUST be removed and the fallback flow (FR-013) applies.
- **FR-013**: When retries are exhausted, the system MUST post a fallback message to the conversation and route the conversation for human attention through the existing escalation and routing flow.
- **FR-014**: All context assembly — agent configuration, conversation history, and retrieved knowledge — MUST be scoped to the conversation's tenant; no other tenant's data may influence or appear in any prompt or response.
- **FR-015**: Every generation attempt (successful or failed) MUST produce an inspectable record covering what was assembled, which provider and model were called, the outcome, latency, and usage, correlated to the conversation and message and integrated with the existing AI usage tracking.
- **FR-016**: At most one AI generation MUST be in flight per conversation at a time. When a customer message arrives during generation, the in-flight generation MUST be superseded: it is cancelled, its partial output is discarded (never posted), and a single new generation runs with context including all customer messages received so far — never interleaved or out-of-order answers.
- **FR-017**: Tenant staff MUST be able to request an AI-generated summary of a conversation's history; the summary MUST be visible to tenant staff only, derived solely from that conversation's tenant-scoped content, and its failure MUST NOT affect the rest of the conversation view.

### Key Entities

- **AI Response**: A message in the conversation history authored by the tenant's AI agent; carries content, attribution to the agent, chronological position, citations (when knowledge was used), and confidence metadata.
- **Generation Record**: The inspectable trace of one generation attempt — triggering message, assembled context summary, provider/model invoked, outcome (success, retried, failed, fallback), latency, and usage — correlated to the conversation.
- **Confidence Metadata**: A per-response assessment of answer confidence — a numeric score (0–1) stored with the response plus its derived band (high / medium / low), rendered to staff as a badge.
- **Fallback Message**: The customer-facing message posted when generation ultimately fails, accompanied by routing of the conversation into the human-attention flow.
- **Conversation Summary**: A staff-only, on-demand condensation of a conversation's history; ephemeral to the viewing context rather than part of the customer-visible transcript.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A customer asking a question covered by the tenant's published knowledge receives a correct, persona-consistent AI answer with no human involvement, and the answer remains in the conversation history on reload — verified end-to-end for every supported provider.
- **SC-002**: For typical questions, viewers see the AI begin responding (indicator plus first visible content) within 5 seconds of the customer message, and complete responses within 20 seconds.
- **SC-003**: 100% of completed AI responses are persisted in conversation history; zero streamed-but-lost or duplicated responses across reconnects and reloads.
- **SC-004**: In isolation testing across at least two tenants, zero instances of one tenant's configuration, history, or knowledge appearing in — or influencing — another tenant's prompts or responses.
- **SC-005**: Under a total provider outage, 100% of affected customer messages result in a fallback message and human routing within 60 seconds; no conversation is left in a permanently waiting state.
- **SC-006**: Tenant staff reviewing a conversation can identify which messages are AI-authored and each response's confidence level at a glance, with zero confidence information exposed on any customer-facing surface.
- **SC-007**: A requested conversation summary for a 50-message conversation is displayed within 10 seconds and allows an agent to take over without reading the full transcript.
- **SC-008**: Every AI response is traceable to a generation record showing provider, model, outcome, latency, and usage.

## Assumptions

- **Trigger semantics**: with a configured, active agent, conversations are AI-first by default (see Clarifications); unconfigured tenants follow the agent-configuration feature's (017) acknowledgment-then-choose behavior; escalated/human-claimed conversations are AI-silent.
- **Knowledge retrieval, citations, and their behavior** are those defined by the embeddings & RAG feature (020); this feature consumes the retrieval pipeline and citation model rather than redefining them. The context-aware retrieval query construction defined there is what feeds RAG context injection here.
- **Confidence in v1 is informational**: it is recorded and displayed to staff but does not automatically trigger escalation or block responses. The stored numeric score (see Clarifications) keeps threshold-driven automation (auto-escalate on low confidence) possible later without data changes, but such automation is out of scope here. The concrete method of deriving the score is a planning decision.
- **Fallback message content**: v1 uses a sensible platform default fallback text; per-tenant customization of fallback wording is deferred (the agent-configuration surface is the natural future home for it).
- **Human routing on failure** reuses the escalation/routing machinery from the human-handoff feature (014) rather than introducing a parallel mechanism.
- **Observation surface**: conversation viewing (streaming, thinking indicator, response cards, badges, summary) happens in the existing tenant dashboard conversation views; dedicated customer-channel widgets are governed by their own channel features.
- **Provider usage tracking** from the provider-abstraction feature (015) is extended, not duplicated, by this feature's generation records.
- **Summary generation** uses the same tenant agent configuration and provider selection as response generation; summaries are generated on demand and are not persisted as conversation messages.
