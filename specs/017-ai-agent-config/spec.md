# Feature Specification: AI Agent Configuration

**Feature Branch**: `017-ai-agent-config`

**Created**: 2026-07-16

**Status**: Draft

**Input**: User description: "AI Agent Configuration — Allow each tenant to configure one AI agent in v1. V1 supports exactly one AI agent configuration per tenant; the data model must allow multiple named agents per tenant in the future without redesign. Scope: agent name, avatar, tone, system prompt, business rules, escalation rules, enabled channels, AI provider selection, model selection. Backend: create AI agent config model, enforce one active default agent per tenant in v1, keep schema extensible for future multiple agents. Frontend: AI agent settings page, prompt editor, tone selector, escalation settings, provider/model selector. Acceptance: tenant admin can configure the AI agent, only one active agent is available per tenant in v1, schema supports future multiple agents, changes are audited."

## Clarifications

### Session 2026-07-16

- Q: Which condition types must v1 escalation rules support? → A: Explicit "I want a human" request + topic/keyword triggers only; sentiment/frustration detection and failed-attempt thresholds are deferred to a later feature.
- Q: Should an unconfigured tenant's AI agent respond to customers at all? → A: Inactive until configured — the settings page presents editable defaults, but the tenant's own agent only starts responding after an admin saves the configuration. (Refined 2026-07-16, planning session): while unconfigured, an arriving customer message receives a one-time automatic acknowledgment reply, and conversation staff then choose per conversation to either hand it to the platform-provided AI (default persona over the platform AI layer) or escalate/assign it to a human.
- Q: Avatar scope for v1 — presets only, or also custom upload? → A: Both — preset gallery plus custom image upload ship in v1, with size/format limits enforced.
- Q: Which tenant roles can manage AI agent settings? → A: Owner and Admin only; Manager, Agent, and Viewer have no access to AI agent settings in v1.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A Tenant Admin Defines the Agent's Identity and Behavior (Priority: P1)

A tenant administrator opens the AI agent settings page and shapes how their tenant's AI agent presents itself and behaves: they give it a name customers will see, pick an avatar, choose a conversational tone from a curated set, and author the system prompt that governs how the agent answers. They save, and from the next AI-handled customer message onward, the agent responds under that identity and instruction set. Returning to the page later shows the saved configuration exactly as it was left.

**Why this priority**: The agent's identity and system prompt are the core of what "configuring an AI agent" means — without them the tenant has no way to make the AI its own. Every other setting refines this foundation.

**Independent Test**: Sign in as a tenant admin, fill in name, avatar, tone, and system prompt, save, reload the page, and verify the configuration persists; verify a subsequent AI-generated reply for that tenant is produced under the saved persona and instructions. Deliverable value: a tenant-branded AI agent.

**Acceptance Scenarios**:

1. **Given** a tenant whose AI agent has never been configured, **When** an admin opens the AI agent settings page, **Then** they see a single agent pre-filled with sensible defaults (a generic name, default avatar, neutral tone, starter prompt) ready to be edited — never an empty error state.
2. **Given** an admin on the settings page, **When** they set the agent's name, avatar, and tone and save, **Then** the configuration is persisted for their tenant and reflected on reload.
3. **Given** an admin editing the system prompt, **When** they enter instructions and save, **Then** subsequent AI responses for that tenant are generated under those instructions.
4. **Given** an admin saving a configuration with a missing name or an over-length system prompt, **When** they attempt to save, **Then** the save is rejected with a clear per-field message and no partial update occurs.
5. **Given** two tenants, **When** each configures its own agent, **Then** neither tenant can see or affect the other's agent configuration.

---

### User Story 2 - A Tenant Admin Selects the AI Provider and Model for the Agent (Priority: P2)

The tenant administrator chooses which AI provider and which model the agent uses to generate replies, picking from the providers and models the platform makes available to their tenant. After saving, the agent's replies are served by the selected provider/model.

**Why this priority**: Provider and model choice directly controls answer quality, cost, and compliance posture, and it is the tenant-facing surface of the platform's provider-independence promise. It depends on the agent existing (Story 1) but is independently valuable.

**Independent Test**: With at least two providers/models available to the tenant, select one, save, trigger an AI reply and verify it was served by the selected provider/model; switch the selection and verify the next reply follows the new choice.

**Acceptance Scenarios**:

1. **Given** the settings page, **When** the admin opens the provider/model selector, **Then** they see only providers and models actually available to their tenant.
2. **Given** a saved provider/model selection, **When** the agent next generates a reply, **Then** that provider and model serve the request.
3. **Given** the admin changes the selection, **When** they save, **Then** the change takes effect from the next AI request with no other intervention.
4. **Given** a previously selected provider or model that is no longer available to the tenant, **When** the admin views the settings page, **Then** the stale selection is clearly flagged and the admin is prompted to choose an available one.

---

### User Story 3 - A Tenant Admin Sets Business Rules and Escalation Rules (Priority: P2)

The tenant administrator records business rules — standing constraints the agent must honor (e.g., "never promise refunds", "always mention business hours") — and escalation rules that define when the agent must hand a conversation to a human (customer explicitly asks for a human, or the conversation touches topics/keywords the agent must not handle). Once saved, the agent observes the business rules in its answers and escalates according to the configured rules.

**Why this priority**: Rules are what make the agent safe to put in front of customers; they build on the base configuration but constitute an independently testable behavior layer.

**Independent Test**: Configure a business rule and an escalation rule, then drive a conversation that triggers each and verify the agent's answer respects the business rule and the matching conversation is escalated to a human with the configured reason.

**Acceptance Scenarios**:

1. **Given** the settings page, **When** the admin adds, edits, reorders, or removes business rules and saves, **Then** the rule set is persisted and applied to subsequent agent responses.
2. **Given** configured escalation rules, **When** a conversation meets a rule's condition, **Then** the conversation is escalated through the platform's existing human-handoff flow and the routing reason reflects which rule fired.
3. **Given** no escalation rules configured, **When** conversations proceed, **Then** the platform's default escalation behavior (customer explicitly requests a human) still applies — rules extend, never disable, the baseline safety valve.
4. **Given** an escalation rule referencing routing attributes (such as a required skill) that later cease to exist, **When** the admin views escalation settings, **Then** the broken reference is surfaced for correction rather than silently ignored.

---

### User Story 4 - A Tenant Admin Controls Which Channels the Agent Serves (Priority: P3)

The tenant administrator enables or disables the channels on which the AI agent responds. On an enabled channel the agent answers automatically; on a disabled channel incoming conversations flow to humans without AI participation.

**Why this priority**: Channel control matters for staged rollout ("AI on web chat only for now") but the platform has few channels in v1, so it refines rather than defines the feature.

**Independent Test**: Disable the agent on a channel, start a conversation on that channel, and verify no AI reply is produced and the conversation routes to humans; re-enable and verify AI replies resume.

**Acceptance Scenarios**:

1. **Given** the settings page, **When** the admin toggles a channel off and saves, **Then** new conversations on that channel receive no AI responses and are handled by humans.
2. **Given** a disabled channel is re-enabled, **When** a new conversation arrives on it, **Then** the agent responds there again.
3. **Given** all channels are disabled, **When** the admin saves, **Then** the save succeeds and the settings page makes plainly visible that the agent is effectively inactive.

---

### User Story 5 - Configuration Changes Are Audited and Access-Controlled (Priority: P3)

Every change to the AI agent configuration is recorded — who changed it, what changed, and when — consistent with the platform's audit trail for sensitive operations. Only tenant members authorized to manage AI settings can view or modify the configuration.

**Why this priority**: Auditing and access control are mandated for AI configuration changes; they are cross-cutting guarantees over the other stories rather than standalone user value.

**Independent Test**: Change the configuration as an admin and verify an audit record captures actor, change, and time; attempt to view/modify it as an unauthorized member and verify refusal.

**Acceptance Scenarios**:

1. **Given** any saved change to the agent configuration, **When** the audit trail is inspected, **Then** it shows who made the change, which fields changed, and when.
2. **Given** a tenant member without AI-settings permission, **When** they attempt to open or modify the agent configuration, **Then** access is refused and no configuration data is exposed.
3. **Given** a platform user operating in a tenant's context via the tenant switcher, **When** they modify the configuration, **Then** the audit record identifies them (not a tenant member) as the actor.

---

### User Story 6 - Conversations Arriving Before the Agent Is Configured (Priority: P2)

A customer writes to a tenant that has not yet configured its AI agent. The customer immediately receives an automatic acknowledgment so they are not met with silence. In the dashboard, the conversation is flagged as awaiting an AI-handling decision, and a staff member with conversation-management permission chooses: let the platform-provided AI (default persona over the platform AI layer) take the conversation, or escalate and assign it to a human through the existing handoff flow.

**Why this priority**: Every tenant passes through the unconfigured window; without this flow, early customer messages either go unanswered or force premature agent configuration. It is independent of the settings page itself.

**Independent Test**: With no agent configured, send a customer message and verify the automatic acknowledgment appears once; choose "platform AI" on one conversation and verify subsequent replies come from the platform default persona; choose "human" on another and verify it lands in the escalation queue.

**Acceptance Scenarios**:

1. **Given** an unconfigured tenant, **When** a customer sends the first message of a conversation, **Then** exactly one automatic acknowledgment reply is added and the conversation is marked awaiting an AI-handling decision.
2. **Given** a conversation awaiting the decision, **When** staff choose platform-provided AI, **Then** subsequent customer messages are answered by the platform default persona through the platform AI layer, and the choice is audited.
3. **Given** a conversation awaiting the decision, **When** staff choose human handling, **Then** the conversation enters the existing escalation/assignment flow with a routing reason indicating no AI agent was configured.
4. **Given** the platform AI layer is not resolvable for the tenant, **When** staff view the decision, **Then** the platform-AI option is unavailable with the reason shown, and human escalation remains selectable.
5. **Given** the tenant later saves its own agent configuration, **When** new customer messages arrive in any conversation, **Then** the configured agent handles them, superseding earlier per-conversation choices.

---

### Edge Cases

- What happens when a tenant has never configured an agent and a customer message arrives? The conversation receives a one-time automatic acknowledgment, then waits for staff to choose platform-provided AI or human escalation (FR-004a/b). The tenant's own agent participates only after the first configuration save, and from then on it supersedes any per-conversation fallback choice (FR-004c).
- What happens when staff pick "platform-provided AI" but the platform AI layer is not configured or its credential is missing? The option is unavailable (disabled with the reason shown); if it becomes unresolvable later, affected conversations fall back to awaiting a decision and human escalation remains available.
- What happens when two admins edit the configuration concurrently? The later save must not silently merge or corrupt; the second saver is told the configuration changed underneath them and must re-apply.
- What happens when the system prompt or a rule contains attempted prompt-injection content (e.g., "ignore prior instructions")? The configuration is stored as authored — it is the tenant's own agent — but it can never widen the agent's capabilities beyond the platform's tool-mediated boundaries.
- What happens when the selected model is removed from the tenant's available set after being saved? Existing traffic falls back to the tenant's AI-layer default configuration, and the settings page flags the stale selection.
- What happens to the avatar if an upload fails or the image is invalid? The previous (or default) avatar remains; the save of other fields is not lost.
- What happens in the future multi-agent world to today's single agent? It becomes the tenant's default named agent — no data rework is needed, which is why the underlying design must support multiple named agents per tenant from day one.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST maintain an AI agent configuration per tenant comprising: agent name, avatar, tone, system prompt, business rules, escalation rules, enabled channels, and AI provider/model selection.
- **FR-002**: In v1 the system MUST expose exactly one active agent configuration per tenant — the tenant's default agent — and MUST prevent creation of a second active agent for the same tenant.
- **FR-003**: The underlying data design MUST support multiple named agents per tenant in the future (per-tenant unique agent names, a designated default agent) such that enabling multi-agent later requires no redesign or data migration of existing configurations.
- **FR-004**: A tenant whose agent has never been explicitly configured has no tenant agent: opening the settings page MUST present platform defaults as an editable starting point, and the tenant's own agent becomes active only upon the first successful save.
- **FR-004a**: While the tenant's agent is unconfigured, the first customer message of a conversation MUST receive a one-time automatic acknowledgment reply (fixed platform text, clearly not from a human), and the conversation MUST be marked as awaiting an AI-handling decision.
- **FR-004b**: For a conversation awaiting that decision, staff with conversation-management permission MUST be able to choose either (a) platform-provided AI — subsequent customer messages in that conversation are answered by the platform default persona through the platform AI layer, available only when that layer resolves — or (b) human handling — the conversation is escalated and assigned through the existing handoff flow. The choice is per conversation and audited.
- **FR-004c**: Once the tenant's agent is configured, it supersedes the fallback: new customer messages follow the configured agent regardless of any earlier per-conversation fallback choice.
- **FR-005**: The system MUST validate configuration on save: agent name is required and length-bounded; the system prompt is length-bounded; tone MUST be one of the platform's offered tones; invalid saves MUST fail atomically with per-field feedback.
- **FR-006**: The avatar MUST be selectable from a platform-provided set or uploaded as an image, with size and format limits enforced; a failed avatar change MUST NOT discard other saved fields.
- **FR-007**: The provider/model selector MUST offer only providers and models available to the tenant, and the saved selection MUST determine which provider/model serves the agent's replies from the next request onward.
- **FR-008**: If a saved provider/model selection becomes unavailable, the system MUST fall back to the tenant's AI-layer default configuration for live traffic and MUST surface the stale selection to admins on the settings page.
- **FR-009**: Business rules MUST be an ordered, editable list of natural-language constraints that are deterministically incorporated into the agent's instructions for every response.
- **FR-010**: Escalation rules MUST support exactly two condition types in v1 — explicit customer request for a human, and topic/keyword triggers — and matching conversations MUST escalate through the platform's existing human-handoff flow with the firing rule recorded as the routing reason. Sentiment/frustration detection and failed-attempt thresholds are out of scope for v1.
- **FR-011**: Escalation rules MUST extend, and MUST NOT be able to disable, the platform's baseline escalation behavior.
- **FR-012**: Admins MUST be able to enable or disable the agent per channel; the agent MUST NOT respond on disabled channels, and conversations there MUST flow to humans.
- **FR-013**: Only tenant members with the Owner or Admin role (and platform users acting in tenant context) MUST be able to view or modify the agent configuration; Manager, Agent, and Viewer roles have no access. Enforcement MUST NOT rely on the frontend alone.
- **FR-014**: Every change to the agent configuration MUST be audited with actor, changed fields, and timestamp, including changes made by platform users in tenant context.
- **FR-015**: Agent configurations MUST be tenant-isolated: no read or write path may expose one tenant's configuration to another.
- **FR-016**: Saved changes MUST take effect for the next AI-generated response without redeploy or manual intervention; in-flight responses MAY complete under the prior configuration.
- **FR-017**: Concurrent edits MUST NOT silently overwrite each other; a save against a configuration modified since it was loaded MUST be rejected with a clear message.
- **FR-018**: The agent's system prompt and rules MUST NOT be able to grant the agent capabilities beyond the platform's tool-mediated boundaries, regardless of their content.

### Key Entities

- **AI Agent Configuration**: A named agent belonging to exactly one tenant; carries identity (name, avatar), behavior (tone, system prompt), governance (business rules, escalation rules), reach (enabled channels), and serving choice (provider/model selection); has an active/default designation and change history. In v1 each tenant has exactly one, always the default; the shape must accommodate several per tenant later.
- **Business Rule**: An ordered natural-language constraint owned by an agent configuration, incorporated into every response the agent produces.
- **Escalation Rule**: A condition owned by an agent configuration that, when met by a conversation, triggers handoff to a human via the existing escalation flow; references routing attributes (e.g., required skill) from the handoff feature.
- **Channel Enablement**: The per-channel on/off state of an agent configuration determining where the agent participates.
- **Audit Record**: The existing append-only who/what/when trail, extended to cover agent-configuration changes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant admin starting from an unconfigured agent can complete a full configuration (identity, tone, prompt, provider/model) in under 5 minutes without documentation.
- **SC-002**: 100% of saved configuration changes are reflected in the very next AI-generated response for that tenant.
- **SC-003**: 100% of configuration changes appear in the audit trail with actor, changed fields, and timestamp.
- **SC-004**: Zero instances of one tenant reading or affecting another tenant's agent configuration.
- **SC-005**: In every conversation that meets a configured escalation condition, the handoff occurs and the recorded routing reason names the rule that fired.
- **SC-006**: Attempting to create a second active agent for a tenant fails 100% of the time in v1, while the same data design demonstrably accommodates multiple named agents without rework.

## Assumptions

- "Tenant admin" means tenant members with the Owner or Admin role; Manager, Agent, and Viewer roles have no access to AI agent settings in v1.
- The AI provider abstraction (feature 015) supplies the set of providers/models available to a tenant, credential handling, and usage tracking; this feature only selects among what that layer offers and does not manage API keys or vendor credentials.
- Escalation handoff mechanics (queueing, routing, agent availability) come from the human-handoff feature (014); this feature only defines *when* the agent escalates, not *how* the handoff is executed.
- Web chat is the only customer-facing channel in v1; the channel-enablement model is built so future channels (email, social messaging, etc.) appear as additional toggles without redesign.
- Tone is chosen from a curated platform-defined set (e.g., professional, friendly, casual, formal, empathetic) rather than free text; the exact list is a design-time decision.
- Configuration changes go live immediately on save; draft/publish workflows and prompt version history beyond the audit trail are out of scope for v1.
- Testing/previewing the agent in a sandbox conversation before going live is out of scope for v1.
- Business rules are conveyed to the AI through deterministic prompt composition; rule *enforcement* is best-effort model behavior plus the platform's escalation safety net, not a hard guarantee — consistent with how system prompts work.
- Uploaded avatar images are stored platform-side and served through an authenticated endpoint, limited to standard web image formats (PNG, JPEG, WebP) and a cap of 256 KB. The storage mechanism itself is an implementation decision, not a product constraint — the platform has no object-storage integration today, so v1 keeps these small images in the database behind a reference that can migrate to object storage later without any change to the user-facing behavior or the API.
