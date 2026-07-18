# Feature Specification: AI Tool Calling

**Feature Branch**: `022-ai-tool-calling`

**Created**: 2026-07-18

**Status**: Draft

**Input**: User description: "AI Tool Calling — Allow AI to execute approved business actions. Scope: tool registry, tool permissions, tool execution logs, tool approval rules, safe tool execution, tool result messages. Backend: define tool interface, register tenant-enabled tools, validate tool permissions, log all tool executions, prevent direct database access by LLMs. Frontend: AI tool execution timeline, tool approval UI, tool result display. Acceptance: AI can request approved tools; unsafe tools require human approval; tool executions are audited; tool failures are visible in the conversation timeline."

## Clarifications

### Session 2026-07-18

- Q: What happens to the AI generation while a tool request awaits human approval? → A: Two-phase (async) — the current generation ends after posting an interim holding message to the customer; when the request is decided (approved / denied / expired), a new generation runs with the outcome and posts the actual answer. Generations never block on human approval.
- Q: What kinds of tools does the v1 registry support? → A: Both built-in and tenant-defined external tools — platform developers ship built-in tools in the platform itself, and tenant admins can additionally register their own tools that point at their organization's external endpoints, which the platform calls on the AI's behalf. Tenant-defined tools belong to exactly one tenant and are never visible to or usable by any other tenant.
- Q: Can the AI chain multiple tool calls within one generation? → A: Bounded multi-step — within one generation the AI may make multiple sequential tool calls up to a platform-set maximum; reaching the limit forces the AI to answer with what it has. Each request in a chain is individually validated, recorded, and shown in the timeline. An approval-required request still ends the generation via the two-phase flow.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The AI answers using an approved tool (Priority: P1)

A customer asks a question the AI cannot answer from knowledge and conversation history alone — it requires live business information or a business action (for example, looking up the status of the customer's order). The AI requests one of the tools that has been registered on the platform and enabled for the tenant. The system verifies the tool is permitted, executes it on the AI's behalf in a controlled way, and hands the result back to the AI, which uses it to compose its reply. The tool activity is recorded and becomes part of the conversation's inspectable history.

**Why this priority**: This is the feature's core loop — request → validate → execute → result → response. Without it, the AI can only talk about the business; with it, the AI can act for the business. Every other story (approvals, auditing, administration) qualifies or observes this loop.

**Independent Test**: Enable a tool for a tenant that returns a distinctive piece of business data, ask the AI a question that requires that data in an AI-handled conversation, and verify the AI's reply reflects the tool's result, the tool activity appears in the conversation's execution timeline, and an execution record exists with the request, outcome, and timing.

**Acceptance Scenarios**:

1. **Given** a tool that is registered on the platform and enabled for the tenant, **When** the AI determines the tool is needed to answer a customer message, **Then** the system validates the request, executes the tool, and returns the result to the AI, which incorporates it into its response.
2. **Given** answering requires more than one step (e.g., look up an order, then check its shipping status), **When** the AI chains sequential tool calls within one generation, **Then** each call is individually validated, executed, and recorded, and the chain is capped at the platform-set maximum — on reaching it, the AI answers with the results it has.
3. **Given** the AI requests a tool that is not registered or not enabled for the tenant, **When** the request is validated, **Then** execution is refused, the refusal is recorded, and the AI continues and responds without the tool — the conversation never silently stalls.
4. **Given** the AI requests a tool with inputs that do not match the tool's defined parameters, **When** the request is validated, **Then** execution is refused before any action is taken and the refusal is recorded.
5. **Given** a tool execution completes, **When** tenant staff view the conversation, **Then** the tool activity (which tool, when, outcome) is visible in the conversation's execution timeline in its correct chronological position.
6. **Given** the AI needs business information, **When** it obtains that information, **Then** it does so exclusively through registered tools — the AI has no path to business data or business actions other than approved tools.
7. **Given** two tenants with different enabled tools, **When** the AI serves a conversation for one tenant, **Then** only that tenant's enabled tools are available to it, and every execution operates only on that tenant's data.

---

### User Story 2 - A sensitive tool waits for human approval (Priority: P2)

The AI determines that answering the customer requires a tool classified as requiring approval — one with real-world side effects, such as issuing a refund. Instead of executing immediately, the system records the pending request, the AI posts an interim holding message to the customer (so the customer is never left in silence), and the current generation ends. The pending request is surfaced to tenant staff, who see which tool the AI wants to run, with what inputs, and in which conversation. When a staff member approves, the tool executes and a new generation runs with the result to post the actual answer. If the request is denied — or nobody decides within a bounded time — the tool never executes, and a new generation informs the AI of the outcome so it can respond to the customer as best it can without it.

**Why this priority**: Approval is the safety property that makes side-effecting tools acceptable at all. Without it, only harmless read-only tools could ever be enabled; with it, tenants can safely delegate consequential actions. It directly implements the acceptance criterion "unsafe tools require human approval."

**Independent Test**: Enable an approval-required tool for a tenant, drive the AI to request it, and verify: execution does not happen before a decision, the pending request is visible and actionable to tenant staff, approving causes exactly one execution whose result reaches the AI, and denying (or letting the request expire) causes zero executions with the AI responding gracefully.

**Acceptance Scenarios**:

1. **Given** a tool classified as approval-required, **When** the AI requests it, **Then** the tool does not execute, an interim holding message is posted to the customer, the requesting generation ends, and a pending approval request is surfaced to tenant staff showing the tool, its inputs, and the conversation context.
2. **Given** a pending approval request, **When** an authorized staff member approves it, **Then** the tool executes exactly once, a new generation runs with the result and posts the answer to the customer, and the approval decision (who, when) is permanently recorded.
3. **Given** a pending approval request, **When** an authorized staff member denies it, **Then** the tool never executes, the denial is recorded, and a new generation informs the AI of the denial so it responds to the customer without the tool result.
4. **Given** a pending approval request, **When** no decision is made within the bounded approval window, **Then** the request expires, is treated as declined, is recorded as expired, the tool never executes, and the follow-up generation runs as it would for a denial.
5. **Given** two staff members act on the same pending request at the same time, **When** both submit decisions, **Then** exactly one decision takes effect and at most one execution occurs.
6. **Given** a conversation is escalated to or claimed by a human while an approval request is pending, **Then** the pending request is cancelled without executing and is recorded as cancelled.

---

### User Story 3 - Staff inspect what the AI did and see failures (Priority: P3)

A tenant agent or supervisor reviewing an AI-handled conversation can see everything the AI did with tools: each request, whether it was auto-approved or human-approved (and by whom), what it ran with, whether it succeeded or failed, and how long it took. When a tool fails — an error, a timeout, a refusal — the failure is plainly visible in the conversation timeline, clearly distinguished from success, and the AI's reply degrades gracefully rather than the conversation stalling. Customers never see raw tool internals; they only ever see the AI's messages.

**Why this priority**: Tool calls are the least predictable part of AI behavior; without an inspectable execution trail, debugging agent actions in production is guesswork and trust in delegated actions collapses. It directly implements the acceptance criteria "tool executions are audited" and "tool failures are visible in the conversation timeline."

**Independent Test**: Run one successful and one failing tool execution in a conversation, then open the conversation as tenant staff and verify both appear in the execution timeline with tool name, outcome, timing, and approval details, the failure is visually distinct, and none of these internals are visible from the customer's view of the conversation.

**Acceptance Scenarios**:

1. **Given** tool executions occurred in a conversation, **When** tenant staff view the conversation, **Then** each execution appears in the timeline with the tool name, its inputs, the outcome, its duration, and — where approval was involved — who decided and when.
2. **Given** a tool execution fails or times out, **When** staff view the conversation, **Then** the failure is visible in the timeline, clearly distinguished from a successful execution, with an indication of what went wrong.
3. **Given** a tool execution fails, **When** the AI continues, **Then** it responds to the customer as best it can without the result (or the conversation follows the existing escalation flow) — the customer is never left without a reply because a tool failed.
4. **Given** a customer views the conversation, **When** tool activity has occurred, **Then** the customer sees only the AI's messages — never tool names, inputs, raw results, or failure details.
5. **Given** any tool request reached any terminal state (executed, refused, denied, expired, cancelled, failed), **When** an authorized reviewer audits the tenant's tool activity, **Then** a permanent record of that request and its outcome exists and is inspectable.

---

### User Story 4 - A tenant admin controls and extends the AI's tool set (Priority: P4)

A tenant administrator views the catalog of built-in tools available on the platform and decides which ones their AI agent may use. For each tool they can enable or disable it, and for enabled tools they can require human approval even if the platform classifies the tool as safe. They cannot loosen platform safety rules: a built-in tool the platform classifies as approval-required can never be made auto-approved by a tenant.

Beyond the built-in catalog, the admin can register their own tools — capabilities backed by their organization's external endpoints (for example, their order-management system). For each tenant-defined tool they describe what it does and what inputs it takes, point it at their endpoint with any credentials the endpoint requires, and classify it as auto-approved or approval-required (new tools default to approval-required). The platform calls the endpoint on the AI's behalf; the AI itself never sees endpoint locations or credentials. A tenant-defined tool exists only for its own tenant.

**Why this priority**: Per-tenant control is what makes the registry trustworthy for businesses, and tenant-defined tools are what make it useful beyond what the platform ships — but the feature already demonstrates its core value with built-in tools enabled on tenants' behalf, so self-service administration and extension land after the execution, approval, and observability loops.

**Independent Test**: As a tenant admin, disable a previously enabled tool and verify the AI can no longer use it; mark a safe built-in tool as approval-required and verify its next use pauses for approval; register a tenant-defined tool against a test endpoint and verify the AI can use it, the endpoint receives the call, and no other tenant can see or use the tool. Verify each configuration change is recorded.

**Acceptance Scenarios**:

1. **Given** the platform tool catalog, **When** a tenant admin views their tool settings, **Then** they see the built-in tools available, their own tenant-defined tools, which are enabled, and each tool's safety classification.
2. **Given** an enabled tool, **When** the admin disables it, **Then** subsequent AI requests for it are refused, while past execution records remain intact.
3. **Given** a built-in tool the platform classifies as safe, **When** the admin marks it as requiring approval, **Then** subsequent requests for it follow the human approval flow.
4. **Given** a built-in tool the platform classifies as approval-required, **When** the admin configures it, **Then** no option exists to exempt it from approval — tenants can only tighten, never loosen, platform safety rules.
5. **Given** an admin registers a tenant-defined tool (description, inputs, endpoint, credentials, classification), **When** the AI next serves that tenant, **Then** the tool is available to it under the same validation, approval, execution, and audit rules as built-in tools.
6. **Given** a tenant-defined tool, **When** any user of another tenant views tool settings or the AI serves another tenant's conversation, **Then** the tool is neither visible nor usable there.
7. **Given** a tenant-defined tool's endpoint credentials, **When** anyone views tool settings, execution records, or the conversation timeline, **Then** the credentials are never displayed, and they are never shared with the AI.
8. **Given** any change to a tenant's tool configuration (including registering, editing, or removing tenant-defined tools), **When** the change is saved, **Then** who changed what and when is permanently recorded.

---

### Edge Cases

- AI requests a tool that exists but was disabled mid-conversation → the validation at request time governs; the request is refused and recorded, and the AI continues without it.
- A tool execution exceeds its bounded execution time → it is treated as failed, recorded as timed out, and the failure is surfaced to the AI and the timeline; side-effecting tools are not automatically retried.
- A new customer message supersedes an AI generation that is still in-flight when it requests an approval-required tool (before its interim holding message is posted) → the not-yet-surfaced request is cancelled without executing and recorded as cancelled; the new generation may issue fresh requests. Once a generation has posted its interim message and ended, its pending request is no longer tied to any in-flight generation and survives supersede.
- The AI requests the same side-effecting tool twice with the same inputs in one generation → each request is a distinct approval/execution with its own record; staff can see and decline duplicates.
- The AI keeps requesting tools without converging on an answer → the platform-set per-generation maximum cuts the chain off; the cutoff is recorded, and the AI answers with what it has.
- A tool result contains sensitive business data → the full result is visible to authorized tenant staff in the timeline; the customer sees only what the AI chooses to say in its reply.
- A customer sends a new message while a tool approval is pending → the pending request remains pending (the interim holding message already ended its generation); the new message triggers a normal generation that may answer independently, and the eventual approval outcome still triggers its follow-up generation with then-current conversation context.
- A staff member without sufficient permissions attempts to approve → the decision is refused and recorded; only authorized roles may decide.
- The platform catalog removes or deprecates a built-in tool that tenants have enabled, or a tenant admin removes a tenant-defined tool → existing execution records remain intact and inspectable; new requests for the tool are refused.
- A tenant-defined tool's external endpoint is unreachable, responds too slowly, or returns a malformed response → the execution is treated as failed (or timed out) under the same rules as any tool failure; the endpoint's raw error details are visible to tenant staff in the timeline but never to the customer.
- A tenant-defined tool's endpoint credentials become invalid (rotated or revoked externally) → executions fail and are recorded as failures; the admin can update the credentials in tool settings without recreating the tool.

## Requirements *(mandatory)*

### Functional Requirements

**Tool registry & permissions**

- **FR-001**: The platform MUST maintain a registry of tools, where each tool is defined with a name, a human-readable description of what it does, its expected inputs, and a safety classification (auto-approved or approval-required). The registry MUST support two tool sources: built-in tools shipped by the platform, and tenant-defined tools registered by tenant admins against their organization's external endpoints.
- **FR-002**: The AI MUST be able to use a tool only if it is registered AND enabled for the conversation's tenant; all other requests MUST be refused and recorded. Tenant-defined tools belong to exactly one tenant and MUST never be visible to or invokable from any other tenant.
- **FR-003**: Tenants MUST be able to enable or disable individual tools and to require approval for built-in tools the platform classifies as safe; tenants MUST NOT be able to exempt a platform-classified approval-required built-in tool from approval. For their own tenant-defined tools, tenant admins set the safety classification, which MUST default to approval-required on creation.
- **FR-003a**: Tenant admins MUST be able to register, edit, and remove tenant-defined tools, specifying the tool's description, expected inputs, target endpoint, and any credentials the endpoint requires. Credentials MUST be stored securely, MUST never be displayed after entry, MUST never appear in execution records or the timeline, and MUST never be exposed to the AI.
- **FR-003b**: The platform MUST call a tenant-defined tool's endpoint on the AI's behalf — the AI never learns endpoint locations and has no direct network access; an endpoint's response (or its failure) is returned to the AI as the tool result under the same rules as built-in tools.
- **FR-004**: The set of tools offered to the AI for a conversation MUST reflect the tenant's current configuration at the time the AI generation begins.

**Request validation & safe execution**

- **FR-005**: Every tool request MUST be validated before execution — the tool is registered, enabled for the tenant, permitted by the tenant's policy, and the inputs match the tool's defined parameters; any validation failure MUST prevent execution and be recorded.
- **FR-006**: The AI MUST have no means of reading or changing business data other than registered tools; every tool execution MUST operate only within the requesting tenant's data and context.
- **FR-007**: Each tool execution MUST complete within a bounded time; executions that exceed it MUST be treated as failed and recorded as timed out.
- **FR-008**: An approved or auto-approved tool request MUST execute at most once; side-effecting tools MUST NOT be automatically retried after failure.
- **FR-008a**: Within a single generation the AI MAY make multiple sequential tool requests up to a platform-set maximum per generation; each request in the chain is individually validated, executed, and recorded. On reaching the maximum, no further requests are accepted for that generation and the AI MUST answer with the results it has. A request for an approval-required tool ends the chain via the two-phase approval flow (FR-011a).
- **FR-009**: Tool execution results MUST be returned to the AI so it can use them in composing its response; failures MUST be returned as failures so the AI can respond gracefully without the result.
- **FR-010**: A tool failure, refusal, denial, or expiry MUST never leave the customer without a reply — the AI continues without the tool result, or the conversation follows the existing escalation flow.

**Approval rules**

- **FR-011**: A request for an approval-required tool MUST NOT execute until an authorized tenant staff member approves it; the pending request MUST be surfaced to staff with the tool, its inputs, and the conversation context.
- **FR-011a**: Approval waits MUST be asynchronous (two-phase): the requesting generation posts an interim holding message to the customer and ends; the approval outcome (approved, denied, or expired) later triggers a new generation that uses the outcome to post the actual answer. A generation MUST never remain in-flight waiting for a human approval decision.
- **FR-012**: Authorized staff MUST be able to approve or deny a pending request from within the conversation view; each decision MUST record who decided and when.
- **FR-013**: Pending approval requests MUST expire after a bounded window; an expired request is treated as declined and MUST never execute afterwards.
- **FR-014**: Concurrent decisions on the same pending request MUST resolve to exactly one effective decision and at most one execution.
- **FR-015**: When a conversation is escalated to or claimed by a human, pending tool requests in that conversation MUST be cancelled without executing and recorded as cancelled. Likewise, if the generation that issued a request is cancelled or superseded while still in-flight, its not-yet-decided requests MUST be cancelled; a pending approval whose requesting generation completed (interim message posted) survives new customer messages until decided or expired.

**Execution logs & auditing**

- **FR-016**: Every tool request MUST produce a permanent record covering its full lifecycle — requested, validated or refused, approved/denied/expired/cancelled (with decider where applicable), executed, and its outcome (success or failure), with timestamps and duration.
- **FR-017**: Execution records MUST be retained regardless of later changes to tool or tenant configuration, and MUST be inspectable by authorized tenant staff and platform operators.

**Conversation timeline & result display**

- **FR-018**: Tool activity MUST appear in the conversation's staff-facing execution timeline in chronological position — each request with its tool name, inputs, approval state, outcome, and duration.
- **FR-019**: Failed executions MUST be visible in the timeline and clearly distinguished from successful ones, with an indication of what went wrong.
- **FR-020**: Customers MUST never see tool internals (tool names, inputs, raw results, failure details); they see only the AI's messages.

### Key Entities

- **Tool Definition**: A registry entry describing a capability the AI may invoke — name, description, expected inputs, safety classification (auto-approved or approval-required), and source: built-in (shipped by the platform, available to all tenants) or tenant-defined (registered by a tenant admin, owned by exactly one tenant, backed by that tenant's external endpoint and securely stored credentials).
- **Tenant Tool Policy**: A tenant's configuration for a tool — enabled or disabled, and whether approval is required beyond the tool's base classification. Belongs to exactly one tenant.
- **Tool Request**: A single instance of the AI asking to run a tool in a conversation — the tool, the inputs, the requesting generation, and its lifecycle state (pending validation, awaiting approval, executing, succeeded, failed, refused, denied, expired, cancelled).
- **Approval Decision**: The human ruling on an approval-required request — approve or deny, who decided, when; or the terminal expiry/cancellation of an undecided request.
- **Tool Execution Record**: The permanent audit trail of a request's lifecycle — timestamps, decider, outcome, duration, and failure details where applicable. Append-only.
- **Tool Timeline Event**: The staff-facing representation of tool activity within a conversation's execution timeline, positioned chronologically among messages.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of tool requests — regardless of outcome — have a complete, inspectable lifecycle record; no execution ever occurs without one.
- **SC-002**: In verification across all tested scenarios, zero executions occur for tools that are unregistered, disabled for the tenant, denied, expired, or cancelled — and zero approval-required executions occur without a recorded human approval.
- **SC-003**: Zero instances of a tool execution reading or affecting another tenant's data across all tested scenarios.
- **SC-004**: Tenant staff reviewing a conversation can determine what tools the AI used, with what outcome, and who approved what, within 30 seconds of opening the conversation and without leaving it.
- **SC-005**: Staff can act on a pending approval request from the conversation view in under 3 interactions (locate, review, decide).
- **SC-006**: In 100% of conversations where a tool fails, is refused, is denied, or expires, the customer still receives a reply or the conversation enters the human escalation flow — no conversation stalls indefinitely because of a tool.
- **SC-007**: Customers are never exposed to tool internals in any tested scenario — customer-facing views contain only conversation messages.
- **SC-008**: Zero instances, across all tested scenarios, of tenant-defined tool credentials appearing in any interface, execution record, timeline entry, or AI-visible content after entry.

## Assumptions

- This feature builds on the AI Conversation Engine (021): tool use happens inside its generation loop, and its existing rules for supersede, escalation handover, and fallback continue to apply. Approval waits are the one addition to its lifecycle: they follow the two-phase flow (interim message → decision → follow-up generation) defined in this spec, so no generation ever blocks on a human.
- The v1 deliverable is the tool-calling framework — registry, permissions, approval, execution, auditing, and timeline — supporting both tool sources: built-in tools (with at least one auto-approved and one approval-required built-in tool exercising the framework) and tenant-defined external tools. A richer catalog of concrete built-in business tools is later work that plugs into this registry.
- For built-in tools, safety classification is set at the platform level and tenants can only tighten (require approval, disable), never loosen; classifying built-in tools correctly is a platform-operator responsibility. For tenant-defined tools, the owning tenant's admin sets the classification (defaulting to approval-required) and bears responsibility for what their own endpoints do.
- Calling a tenant-defined tool sends conversation-derived inputs to the tenant's own external endpoint; this is the tenant acting on its own data via its own systems, and endpoint reliability is the tenant's responsibility — the platform's responsibility is bounded execution, failure visibility, and credential confidentiality.
- Approval authority follows existing tenant roles: tenant users who handle conversations (Agent role and above) may decide approval requests; Viewers may not. Platform operators acting within a tenant context follow existing tenant-switch rules.
- The bounded approval window has a platform default and is treated as declined on expiry; the specific duration is a planning decision. Likewise, the platform-set maximum of tool calls per generation has a platform default whose specific value is a planning decision.
- Approval requests surface through the existing staff realtime channel and conversation view; no separate notification system is introduced by this feature.
- Read-only tool failures may be retried within the generation's existing bounded retry budget; side-effecting tools are never automatically retried.
- Sensitive data returned by tools is displayed to authorized tenant staff as-is in v1; automatic redaction or masking of tool results is out of scope.
- Configuration changes to tenant tool policy are audited through the platform's existing audit trail conventions.
