# Feature Specification: Human Handoff & Routing

**Feature Branch**: `014-human-handoff-routing`

**Created**: 2026-07-14

**Status**: Draft

**Input**: User description: "Human Handoff & Routing — Support escalation from AI to human agents. Routing strategy: skill/tag-based routing with fallback to load-based routing. Scope: escalation queue, agent availability, agent skills, assignment rules, manual claim, auto-assignment, escalation reason. Backend: store agent skills, store agent availability, implement routing service, assign to best available agent, fall back to least-loaded available agent, place in queue if no agent is available. Frontend: escalation queue, agent assignment UI, agent availability toggle, escalation banner, routing reason display. Acceptance: AI can escalate a conversation, matching skilled agents are preferred, load-based fallback works, agents can manually claim queued conversations, tests cover routing logic."

## Clarifications

### Session 2026-07-14

- Q: How does escalation relate to the fixed conversation status set (open/pending/resolved/closed) from conversation-core? → A: Escalation is an orthogonal flag on the conversation; status is unaffected (an escalated conversation stays/becomes open). The inbox gains an "escalated" filter.
- Q: When an agent becomes available and the oldest queued conversation requires skills they lack while a younger entry matches their skills, which do they receive? → A: Skill-aware drain — the agent gets the oldest queued entry whose required skills they match; if they match none of the queued entries, they get the oldest entry outright (load fallback).
- Q: What does the AI do after escalating a conversation? → A: The AI stops responding the moment it escalates; customer messages while queued or assigned are handled by humans only, until/unless the escalation is closed out.
- Q: How does an agent learn they've been auto-assigned an escalation? → A: Real-time push — the agent is notified (browser/desktop notification plus immediate in-app indicator) the moment an escalation is assigned to them; live-update delivery for assignment notifications is in scope for this feature.
- Q: Does availability auto-revert when an agent's session ends? → A: Yes — presence-aware safeguard: availability auto-reverts to away when the agent has no active dashboard session; the toggle remains the manual control while signed in.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Escalation Is Routed to the Best Available Agent (Priority: P1)

A customer conversation being handled by the AI assistant reaches a point where a human is needed — the customer asks for a person, the AI cannot resolve the issue, or the topic requires human judgment. The AI escalates the conversation with a stated reason and, optionally, the skills the conversation requires (for example "billing" or "arabic"). The system immediately routes the conversation: it prefers an available agent whose skills match the requirement, falls back to the least-loaded available agent when no skilled agent is available, and places the conversation in the tenant's escalation queue when no agent is available at all. Whatever the outcome, the escalation is never lost and the decision is recorded.

**Why this priority**: This is the core promise of the feature — a customer who needs a human reliably reaches one (or a queue that leads to one). Every other story refines or supports this flow.

**Independent Test**: Using seeded agents with known skills, availability, and workloads, trigger escalations with and without required skills and verify each lands on the expected agent (skill match first, then least-loaded, then queue). Deliverable value: escalated customers are connected to the right human without manual dispatching.

**Acceptance Scenarios**:

1. **Given** an escalation requiring skill "billing" and an available agent who has that skill, **When** the escalation is triggered, **Then** the conversation is assigned to that agent and the routing decision records that it was a skill match.
2. **Given** an escalation requiring skill "billing", two available agents with that skill, and one of them carrying fewer active assigned conversations, **When** the escalation is triggered, **Then** the conversation is assigned to the less-loaded matching agent.
3. **Given** an escalation requiring a skill that no available agent has, and other agents available, **When** the escalation is triggered, **Then** the conversation is assigned to the least-loaded available agent and the routing decision records that it was a load-based fallback.
4. **Given** an escalation with no required skills, **When** it is triggered, **Then** the conversation is assigned to the least-loaded available agent.
5. **Given** an escalation when no agent in the tenant is available, **When** it is triggered, **Then** the conversation is placed in the tenant's escalation queue with its reason and required skills, and the customer-facing state makes clear a human will follow up.
6. **Given** any escalation, **When** routing completes, **Then** the escalation reason, routing outcome, and who/what/when are recorded in the audit trail, and the conversation is marked as escalated.
7. **Given** an escalation in tenant A, **When** routing runs, **Then** only tenant A's agents are ever considered, regardless of other tenants' agents being available.

---

### User Story 2 - Agents Work the Escalation Queue and Claim Conversations (Priority: P2)

An agent opens the escalation queue and sees the tenant's queued escalations — each with the customer, channel, escalation reason, required skills, and how long it has been waiting, ordered oldest first. The agent claims a conversation, which assigns it to them and removes it from the queue. When an agent becomes available (or an agent's capacity frees up), the system also drains the queue automatically, routing waiting conversations using the same skill-then-load policy.

**Why this priority**: The queue is the safety net for P1's "no agent available" outcome; without claiming and auto-drain, queued customers would wait indefinitely.

**Independent Test**: Seed queued escalations with no available agents; verify the queue lists them correctly, that an agent can claim one (assignment recorded, entry removed), and that toggling an agent to available auto-assigns the oldest compatible queued conversation.

**Acceptance Scenarios**:

1. **Given** queued escalations exist, **When** an agent opens the escalation queue, **Then** they see only their tenant's queued conversations with reason, required skills, customer, channel, and waiting time, ordered longest-waiting first.
2. **Given** a queued escalation, **When** an agent claims it, **Then** the conversation is assigned to that agent, it leaves the queue for everyone, and the routing record shows it was manually claimed by that agent.
3. **Given** two agents claim the same queued conversation at nearly the same time, **When** both submit, **Then** exactly one claim succeeds and the other agent is told the conversation was already claimed, with the queue view refreshed.
4. **Given** conversations are waiting in the queue, **When** an agent becomes available, **Then** the system automatically routes to them the oldest queued conversation whose required skills they match — or the oldest entry outright if they match none — and the routing record shows it was auto-assigned from the queue.
5. **Given** an empty queue, **When** an agent opens it, **Then** a clear empty state confirms there is nothing waiting.

---

### User Story 3 - Agents Control Their Availability (Priority: P3)

An agent starting their shift toggles themselves to available, signalling they can receive escalations. When they step away or finish, they toggle to away, and the routing engine stops sending them new escalations — without affecting conversations already assigned to them.

**Why this priority**: Availability is the gate on all automatic routing; without an accurate signal, escalations would be pushed to absent agents. It is P3 only because P1/P2 can be tested with seeded availability.

**Independent Test**: Toggle an agent between available and away and verify escalations are routed to them only while available, and that going away neither unassigns their existing conversations nor blocks manual claiming.

**Acceptance Scenarios**:

1. **Given** an agent who is away, **When** they toggle to available, **Then** they immediately become eligible for automatic routing and queued conversations may auto-assign to them.
2. **Given** an available agent, **When** they toggle to away, **Then** no new escalations are automatically routed to them, while their currently assigned conversations remain assigned.
3. **Given** an away agent viewing the escalation queue, **When** they claim a conversation, **Then** the claim succeeds — manual claiming is allowed regardless of availability.
4. **Given** any agent, **When** they view the dashboard, **Then** their current availability state is always visible and the toggle is reachable from anywhere in the dashboard.
5. **Given** an available agent whose dashboard session ends (sign-out, expiry, or sustained disconnect) or whose membership is deactivated, **When** this is detected, **Then** their availability auto-reverts to away and they receive no new escalations; signing back in does not automatically restore available.

---

### User Story 4 - Managers Define Agent Skills (Priority: P4)

A tenant manager maintains the tenant's skill catalog (short tags such as "billing", "technical", "arabic") and assigns skills to each agent so that skill-based routing reflects the team's real expertise. Skills can be added to or removed from an agent at any time and take effect on the next routing decision.

**Why this priority**: Skill data powers P1's preferred path, but routing degrades gracefully (load-based) without it, so managing skills ranks below the routing flows themselves.

**Independent Test**: As a manager, create skills, assign them to agents, verify a subsequent escalation requiring those skills routes to the right agent; remove a skill and verify routing no longer prefers that agent.

**Acceptance Scenarios**:

1. **Given** a manager on the team management area, **When** they create a skill with a unique name, **Then** it appears in the tenant's skill catalog and can be assigned to agents.
2. **Given** an agent and the skill catalog, **When** a manager assigns or removes skills for that agent, **Then** the agent's skill set updates immediately and the change is recorded with who/when.
3. **Given** a skill assigned to agents, **When** a manager deletes it from the catalog, **Then** it is removed from all agents and pending queue entries stop requiring it, with the deletion recorded.
4. **Given** a member without team-management permission, **When** they attempt to change skills, **Then** the system refuses and the interface does not offer the controls.
5. **Given** two tenants, **When** each defines skills, **Then** each tenant sees and uses only its own skill catalog.

---

### User Story 5 - Everyone Sees Why a Conversation Landed Where It Did (Priority: P5)

A team member opening an escalated conversation sees an escalation banner: the conversation was escalated, when, and the reason given. Alongside the assignment, they see the routing explanation — matched on skills, assigned by load fallback, claimed manually, or auto-assigned from the queue. Members can also reassign an escalated conversation to another agent manually when routing got it wrong.

**Why this priority**: Transparency and manual correction make the routing system trustworthy and debuggable, but the routing itself (P1–P4) must exist first.

**Independent Test**: Escalate conversations through each routing path and verify the banner and routing reason render correctly on the conversation detail page for each outcome, and that a permitted member can manually reassign.

**Acceptance Scenarios**:

1. **Given** an escalated conversation, **When** any tenant member opens it, **Then** an escalation banner shows that it was escalated, when, and the escalation reason.
2. **Given** an escalated conversation that was auto-routed, **When** a member views its assignment, **Then** the routing reason is displayed in plain language (e.g., "matched skills: billing", "least-loaded fallback", "claimed by [agent]", "auto-assigned from queue").
3. **Given** an escalated conversation, **When** a permitted member manually reassigns it to another active agent, **Then** the assignment updates, the routing reason reflects a manual reassignment, and the change is audited.
4. **Given** a conversation that was never escalated, **When** it is viewed, **Then** no escalation banner or routing reason appears.

---

### Edge Cases

- What happens when the only skill-matching agents are heavily loaded while non-matching agents are idle? Skill match takes precedence: the least-loaded *matching* available agent is chosen even if a non-matching agent has less load; load only decides among equals (ties within skill matches, or the fallback pool).
- What happens when an escalation requires multiple skills and no available agent has all of them? Agents matching more of the required skills are preferred over agents matching fewer; if no available agent matches any required skill, load-based fallback applies.
- What happens when an already-escalated conversation is escalated again? The escalation is refused as a duplicate while the conversation is queued or escalated-and-assigned; the existing escalation record stands.
- What happens when the assigned agent of an escalated conversation is deactivated or removed from the tenant? The conversation is treated as needing reassignment (per conversation-core rules) and re-enters routing as a queued escalation with its original reason and skills.
- What happens when an agent toggles away between being selected by routing and the assignment completing? The assignment either completes or the conversation re-routes; it must never end up unassigned and unqueued.
- What happens when the AI escalates a conversation in a tenant that has no agents at all (e.g., a brand-new tenant)? The conversation queues, and the queue view for managers makes the unstaffed backlog visible.
- What happens when a queued conversation's customer resolves the issue or the conversation is closed before any agent takes it? Closing or resolving the conversation removes it from the queue and closes out the escalation record.
- What happens when auto-drain would assign several queued conversations to one newly available agent at once? Queue drain respects load-balancing: it assigns one conversation at a time, re-evaluating load, so a single agent is not flooded the moment they come online.

## Requirements *(mandatory)*

### Functional Requirements

**Escalation**

- **FR-001**: The system MUST provide an escalation capability that marks a conversation as escalated to humans, carrying a required escalation reason (free text) and an optional set of required skills drawn from the tenant's skill catalog. The escalated mark is orthogonal to conversation status: it does not change the fixed status set (open, pending, resolved, closed), and an escalated conversation is open while awaiting or receiving human handling.
- **FR-001a**: The conversation inbox MUST offer an "escalated" filter so members can narrow the inbox to escalated conversations, combinable with the existing status, assignment, and channel filters.
- **FR-002**: The escalation capability MUST be invocable by the tenant's AI assistant (or automated process acting on its behalf) for conversations in its own tenant only; escalating a conversation that is already queued or escalated-and-assigned MUST be refused as a duplicate.
- **FR-002a**: From the moment a conversation is escalated, the AI MUST NOT respond on it — customer messages arriving while it is queued or assigned are handled by humans only. AI participation may resume only if the escalation is closed out (the conversation is resolved or closed) and a new conversation or interaction begins under the AI's normal rules.
- **FR-003**: Every escalation MUST immediately produce exactly one of two outcomes: assignment to an agent, or placement in the tenant's escalation queue. An escalation MUST never be silently dropped.
- **FR-004**: Each escalation MUST be recorded with the conversation, reason, required skills, time, routing outcome, and (once assigned) the receiving agent, and MUST produce append-only audit records for the escalation and every resulting assignment, consistent with the platform's existing audit trail.

**Routing policy**

- **FR-005**: When an escalation specifies required skills, routing MUST prefer available agents matching those skills: agents matching more of the required skills rank above agents matching fewer, and among equally matching agents the one with the lowest current load wins.
- **FR-006**: When no available agent matches any required skill — or the escalation specifies no skills — routing MUST assign the available agent with the lowest current load (load-based fallback).
- **FR-007**: An agent's load MUST be measured as the number of open or pending conversations currently assigned to them within the tenant.
- **FR-008**: When no agent in the tenant is available, routing MUST place the conversation in the tenant's escalation queue, ordered by escalation time (oldest first).
- **FR-009**: Routing MUST only ever consider active members of the conversation's own tenant who hold a conversation-management-capable role (Owner, Admin, Manager, Agent); Viewers and members of other tenants MUST never receive routed conversations.
- **FR-010**: Every automatic or manual routing outcome MUST record a routing reason from a fixed set — skill match (with the matched skills), load fallback, manual claim, auto-assigned from queue, manual reassignment — retrievable for display.
- **FR-011**: Concurrent routing and claiming MUST be safe: a conversation MUST end up assigned to exactly one agent, and competing claims on the same queued conversation MUST result in one success and clear refusals for the rest.

**Escalation queue**

- **FR-012**: Tenant members with conversation-management permission MUST be able to view the tenant's escalation queue, showing each waiting conversation's customer, channel, escalation reason, required skills, and time waiting, ordered longest-waiting first.
- **FR-013**: Agents MUST be able to manually claim any queued conversation regardless of their availability state; a successful claim assigns the conversation to them, removes it from the queue, and records a manual-claim routing reason.
- **FR-014**: When an agent becomes available or their load decreases, the system MUST automatically assign queued conversations to eligible agents using skill-aware drain: the agent receives the oldest queued entry whose required skills they match; if they match none of the queued entries' required skills (or no entries specify skills), they receive the oldest entry outright. Drain proceeds one conversation at a time per agent so no single agent is flooded.
- **FR-015**: A queued conversation that is resolved or closed before assignment MUST leave the queue, with its escalation record closed out accordingly.

**Agent availability**

- **FR-016**: Each tenant member with an agent-capable role MUST have an availability state — available or away — scoped per tenant, defaulting to away, and changeable only by that member themselves via a persistent, always-reachable toggle in the dashboard.
- **FR-017**: Availability changes MUST take effect on the next routing decision; toggling to away MUST NOT unassign the agent's existing conversations, and deactivated or removed members MUST be treated as away.
- **FR-017a**: Availability MUST be presence-aware: when an available agent no longer has an active dashboard session (sign-out, session end, or sustained disconnect), their availability MUST auto-revert to away so routing never targets absent agents. The manual toggle remains the control while signed in; signing back in does not automatically restore available.

**Agent skills**

- **FR-018**: Tenant members with team-management permission MUST be able to maintain a tenant-scoped skill catalog (create, rename, delete skills with unique names per tenant) and assign or remove any catalog skills for each agent; all changes MUST be audited.
- **FR-019**: Deleting a skill MUST remove it from all agents and from the required-skill sets of pending queue entries; escalation records already written retain the skill for history.
- **FR-020**: Skill and availability data MUST be tenant-isolated: no tenant can view or use another tenant's skills, availability states, or queue.

**Conversation experience**

- **FR-021**: An escalated conversation MUST display an escalation banner on its detail view showing that and when it was escalated and the escalation reason, visible to all tenant members who can view the conversation; conversations never escalated show no banner.
- **FR-022**: The assignment display of an escalated conversation MUST include the human-readable routing reason for the current assignment.
- **FR-023**: Permitted members MUST be able to manually reassign an escalated conversation to any active agent-capable member, consistent with existing assignment rules; the routing reason updates to manual reassignment and the change is audited.
- **FR-025**: When an escalated conversation is assigned to an agent by routing (auto-assignment, queue drain) or by another member's manual reassignment, the receiving agent MUST be notified in real time: a browser/desktop notification where the agent has granted permission, and always an immediate in-app indicator in the dashboard — without requiring a manual page refresh. Notifications MUST identify the conversation and its escalation reason and MUST never be delivered to members of other tenants.

**Verification**

- **FR-024**: The routing policy (skill preference, most-skills ranking, load tie-breaking, load fallback, queueing, queue drain order, claim contention) MUST be covered by automated tests that exercise each branch of the decision logic.

### Key Entities

- **Skill**: A tenant-defined tag naming an area of expertise (e.g., "billing", "arabic"). Unique by name within a tenant; exists independently of any agent.
- **Agent Skill Assignment**: The link between a tenant member and a skill, indicating that agent can handle conversations requiring it.
- **Agent Availability**: A per-member, per-tenant state (available / away) gating automatic routing eligibility; defaults to away.
- **Escalation**: The record that a conversation was handed off from AI to humans — carries the conversation, reason, required skills, escalation time, current status (queued, assigned, closed), and routing history.
- **Escalation Queue Entry**: A waiting escalation with no assigned agent, ordered by escalation time within its tenant; removed on claim, auto-assignment, or conversation closure.
- **Routing Decision**: The recorded explanation of how an escalated conversation reached its current assignee — skill match (with matched skills), load fallback, manual claim, auto-assigned from queue, or manual reassignment — with who/when.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of escalations end in exactly one of the defined outcomes (assigned or queued) — none are lost — verifiable by reconciling escalation records against assignments and queue contents.
- **SC-002**: When at least one available agent matches an escalation's required skills, the conversation is assigned to a matching agent 100% of the time.
- **SC-003**: Escalated conversations are assigned or queued within 5 seconds of escalation, and customers whose conversation is escalated see the handoff acknowledged without a gap in the conversation.
- **SC-004**: An agent can find and claim a queued conversation in 2 interactions or fewer from the escalation queue view.
- **SC-005**: For every escalated-and-assigned conversation, a team member viewing it can state why it was routed there (reason and routing explanation are displayed) without consulting anyone.
- **SC-006**: Zero escalations are ever routed to, queued for, or visible to members of another tenant.
- **SC-007**: Automated tests cover every routing branch (skill match, multi-skill ranking, load tie-break, load fallback, queueing, auto-drain, claim contention) and pass in the standard quality gates.
- **SC-008**: An agent with an active dashboard session is notified of an escalation assigned to them within 5 seconds of the assignment, without refreshing the page.

## Assumptions

- The AI assistant subsystem that will invoke escalation is delivered by a separate feature; this feature exposes the escalation capability and its rules, and it is exercised directly (as the AI would) for testing until that subsystem exists.
- "Best available agent" is fully defined by the fixed policy in this spec (most matched skills, then lowest load, then load-based fallback, then queue). Tenant-configurable assignment rules (custom rule builders, priorities, business hours) are out of scope for this version.
- Agent load counts open and pending conversations assigned to the agent in the tenant; there is no hard per-agent capacity cap in this version — routing balances by relative load only.
- Availability is a self-service toggle with two states (available / away), backed by a presence-aware safeguard that auto-reverts absent agents to away (FR-017a); schedules and additional statuses (busy, break) are out of scope.
- The escalation queue is ordered strictly by waiting time (oldest first); customer- or topic-based priority tiers are out of scope for this version.
- Escalation reasons are free text supplied by the caller (the AI); a curated reason taxonomy can be layered on later without changing these rules.
- Skills management lives with the existing tenant team management area and reuses its permission model (team-management permission: Owner/Admin/Manager); claiming and receiving escalations follows the existing conversation-management permission (Agent-level roles and above).
- This feature builds on conversation-core (013): assignment, status, tenant isolation, and audit behavior established there apply unchanged; escalation adds to them rather than replacing them.
- Real-time (live-push) delivery is in scope for assignment notifications to agents (FR-025). The escalation queue and availability displays may use the same live-update capability where convenient, but otherwise follow the dashboard's existing data-refresh patterns, as long as routing decisions themselves act on current data.
