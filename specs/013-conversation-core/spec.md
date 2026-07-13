# Feature Specification: Conversation Core

**Feature Branch**: `013-conversation-core`

**Created**: 2026-07-13

**Status**: Draft

**Input**: User description: "Conversation Core — Create the core conversation and message system. Scope: conversations, messages, participants, channels, conversation status, assignment status, internal notes, message timeline. Backend: create conversation, list conversations, view conversation, add message, update conversation status, assign conversation. Frontend: conversation inbox, conversation detail page, message timeline, reply composer, conversation filters, status badges. Acceptance: tenant users can view their tenant conversations, messages are ordered correctly, conversation status can be updated, conversation data is isolated by tenant."

## Clarifications

### Session 2026-07-13

- Q: What happens to status when someone replies to a resolved or closed conversation? → A: A customer-facing reply automatically reopens the conversation to open; internal notes never change status.
- Q: What does the inbox show before any filter is applied? → A: Open conversations only; other statuses are reachable via the status filter.
- Q: Can a customer have multiple open conversations on the same channel at the same time? → A: Yes — each conversation is an independent thread; no single-open-conversation constraint per customer or channel.
- Q: Given no channel integrations yet, can team members log a message on behalf of the customer (e.g., transcribing a phone call)? → A: Yes — permitted members can manually log a customer-authored message, permanently marked as logged by that member for authorship integrity.
- Q: Can Agents assign conversations to other members, or only to themselves? → A: Agent-level roles and above can assign any active tenant member (including handing conversations to teammates); no extra restriction for Agents.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Work the Conversation Inbox (Priority: P1)

A tenant team member opens the Conversations area of the dashboard and sees an inbox listing their tenant's conversations. Each row shows who the conversation is with, which channel it is on, its current status and assignee, a preview of the latest message, and when it was last active. The member narrows the inbox with filters — by status, by assignment (mine / unassigned / anyone), and by channel — and uses clear status badges to triage at a glance. Members never see conversations belonging to any other tenant.

**Why this priority**: The inbox is the entry point for all conversation work and is where the core acceptance criteria (tenant users can view their tenant's conversations; tenant isolation) are proven. Every other story is reached from it.

**Independent Test**: Seed two tenants with distinct conversations in varied statuses, channels, and assignments; sign in as a member of each tenant and confirm the inbox lists only that tenant's conversations, ordered by most recent activity, and that each filter narrows the list correctly.

**Acceptance Scenarios**:

1. **Given** a tenant with existing conversations, **When** a tenant member opens the conversation inbox, **Then** they see a paginated list of that tenant's open conversations by default showing customer, channel, status badge, assignee, latest-message preview, and last-activity time, ordered most recently active first.
2. **Given** two tenants each with their own conversations, **When** a member of tenant A views or filters the inbox, **Then** no conversation from tenant B ever appears in results, counts, or pagination.
3. **Given** conversations in several statuses, **When** the member filters by a status (e.g., open), **Then** only conversations in that status are listed, and each row's badge visibly reflects its status.
4. **Given** conversations assigned to various team members, **When** the member filters by "assigned to me" or "unassigned", **Then** only the matching conversations are listed.
5. **Given** a filter combination that matches nothing, **When** it is applied, **Then** the inbox shows a clear empty state with the option to reset filters.

---

### User Story 2 - Read a Conversation Timeline (Priority: P2)

A tenant team member opens a conversation from the inbox and lands on its detail page. They see who the participants are (the customer and any team members involved), the channel, the current status and assignee, and a message timeline that presents every message in the order it was sent — customer messages, team replies, and internal notes — with sender and timestamp on each entry. Internal notes are visually distinct so it is always obvious they were never visible to the customer.

**Why this priority**: The timeline is the "single view of the exchange" that gives responders context, and it is where the message-ordering acceptance criterion is proven. It depends on the inbox (P1) to be reachable.

**Independent Test**: Seed a conversation with an interleaved mix of customer messages, team replies, and internal notes; open its detail page and verify all entries render in chronological order with correct sender attribution, and that notes are visibly distinguished from customer-facing messages.

**Acceptance Scenarios**:

1. **Given** a conversation with messages from the customer and from team members, **When** a tenant member opens the conversation, **Then** the timeline shows every message in chronological order with the sender's identity and the time sent.
2. **Given** messages sent at nearly the same moment, **When** the timeline is displayed (or re-displayed), **Then** their relative order is stable and never changes between views.
3. **Given** a conversation containing internal notes, **When** the timeline is viewed, **Then** notes appear in their chronological position but are clearly visually distinguished from customer-facing messages.
4. **Given** a conversation with a long history, **When** the detail page opens, **Then** the most recent portion of the timeline is shown first and earlier messages can be loaded on demand without losing order.
5. **Given** a conversation ID that belongs to another tenant, **When** a tenant member attempts to open it directly (e.g., via URL), **Then** the system responds as if the conversation does not exist.

---

### User Story 3 - Reply and Leave Internal Notes (Priority: P3)

A tenant team member with sufficient permissions uses the composer on the conversation detail page to write a reply to the customer, or switches the composer to note mode to leave an internal note for teammates. Sent entries appear immediately at the end of the timeline. Members without reply permission can read the conversation but cannot compose.

**Why this priority**: Composing is how teams act on conversations, but it requires the inbox and timeline to exist first. Ranked after read paths because the acceptance criteria emphasize viewing, ordering, and isolation.

**Independent Test**: As a permitted member, send a reply and an internal note in a seeded conversation and verify both appear at the end of the timeline with correct type, sender, and timestamp, and that the conversation's last-activity time and inbox preview update. As a view-only member, verify the composer is unavailable and direct submission attempts are refused.

**Acceptance Scenarios**:

1. **Given** a permitted member viewing a conversation, **When** they submit a reply, **Then** the message is appended to the timeline with their identity and timestamp, and the conversation's last-activity time and inbox preview reflect it.
2. **Given** a permitted member viewing a conversation, **When** they switch the composer to internal-note mode and submit, **Then** the note is appended to the timeline, marked as internal, and recorded as never being part of what the customer sees.
3. **Given** an empty or whitespace-only submission, **When** it is sent, **Then** the system rejects it with a clear message and nothing is added to the timeline.
4. **Given** a member whose role does not permit composing, **When** they view a conversation, **Then** the composer is not offered, and any direct submission attempt is refused by the system.
5. **Given** a resolved or closed conversation, **When** a permitted member sends a customer-facing reply, **Then** the conversation automatically returns to open status; **When** they instead add an internal note, **Then** the status is unchanged.
6. **Given** a permitted member transcribing an exchange (e.g., a phone call), **When** they switch the composer to log-customer-message mode and submit, **Then** the entry is appended to the timeline as a customer-authored message that also permanently records which member logged it and when.

---

### User Story 4 - Manage Status and Assignment (Priority: P4)

A tenant team member with sufficient permissions changes a conversation's status (for example from open to pending while waiting on the customer, to resolved when the issue is handled, or back to open if it flares up again) and assigns the conversation to themselves or a teammate — or returns it to the unassigned pool. Status and assignee are visible on both the inbox and the detail page, and every change is recorded with who made it and when.

**Why this priority**: Status and assignment turn the inbox into a working queue and prove the "conversation status can be updated" acceptance criterion. They act on conversations that already exist and are readable (P1–P2).

**Independent Test**: As a permitted member, move a seeded conversation through open → pending → resolved → open, assign it to a teammate, then unassign it; verify each change is reflected in the detail page and inbox (including filters and badges) and that a who/when record exists for each change. As a view-only member, verify these controls are unavailable and refused.

**Acceptance Scenarios**:

1. **Given** an open conversation, **When** a permitted member changes its status, **Then** the new status is saved, its badge updates everywhere the conversation is shown, and the change is recorded with who made it and when.
2. **Given** a resolved or closed conversation, **When** a permitted member reopens it, **Then** its status returns to open and it reappears under open-status filters.
3. **Given** an unassigned conversation, **When** a permitted member assigns it to themselves or an active teammate, **Then** the assignee is saved and shown, and the conversation appears under that member's "assigned to me" filter.
4. **Given** an assigned conversation, **When** a permitted member unassigns it, **Then** it returns to the unassigned pool and appears under the "unassigned" filter.
5. **Given** an assignment attempt targeting someone who is not an active member of the tenant, **When** it is submitted, **Then** the system refuses it with a clear message and the previous assignee is unchanged.
6. **Given** a member whose role does not permit these actions, **When** they view a conversation, **Then** status and assignment controls are not offered, and any direct attempt is refused by the system.

---

### User Story 5 - Start a New Conversation (Priority: P5)

A tenant team member with sufficient permissions starts a new conversation with an existing customer — choosing the customer and the channel, and writing the first message. The conversation appears in the inbox as open and shows in the customer's profile history.

**Why this priority**: Outbound conversation creation completes the lifecycle but is the least critical path — most conversations will eventually originate from inbound channel activity, which is out of scope here. It depends on customers (feature 012) and on all prior stories.

**Independent Test**: As a permitted member, create a conversation for a seeded customer on a supported channel with a first message; verify it appears in the inbox as open and unassigned, its timeline shows the first message, and the customer's profile history includes it.

**Acceptance Scenarios**:

1. **Given** a permitted member and an existing customer, **When** the member starts a conversation by selecting the customer and a channel and sending a first message, **Then** the conversation is created in their tenant with open status, the message is the first timeline entry, and it appears in the inbox.
2. **Given** a conversation creation attempt without a customer, without a channel, or with an empty first message, **When** it is submitted, **Then** the system rejects it with field-level messages and nothing is created.
3. **Given** a newly created conversation, **When** the customer's profile is viewed, **Then** the conversation appears in the profile's conversation history with its channel, status, and last-activity time.

---

### Edge Cases

- What happens when two tenants have conversations with visually identical content? Each exists independently; nothing about one tenant's conversations is ever observable from the other, including via direct links, counts, or filters.
- What happens when a conversation's assignee is later deactivated or removed from the tenant? The conversation keeps a readable record of the past assignment but is treated as needing reassignment; it must not disappear from the inbox and new assignments to that person are refused.
- What happens when two members change status or assignment at nearly the same time? The last successful change wins cleanly; the record never ends up in a mixed or invalid state, and both changes appear in the who/when history.
- What happens when a message body is extremely long or contains unusual characters (emoji, right-to-left text, markup-like text)? It is stored and displayed safely and legibly without breaking the timeline layout or being interpreted as markup.
- What happens when a member opens a conversation that has a conversation record but no messages yet? The timeline shows an informative empty state and the composer (for permitted members) still works.
- What happens when the inbox is opened for a brand-new tenant with no conversations? A clear empty state explains there are no conversations yet.
- What happens to conversations when their customer record changes (e.g., renamed)? Conversations always display the customer's current name; the link between conversation and customer is never broken by profile edits.
- What happens when a member loads an inbox page while conversations are being updated by teammates? Pagination stays coherent — no duplicates or gaps that would hide a conversation from the person paging through.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST store conversations that are owned by exactly one tenant; every read and write of conversation and message data MUST be scoped to the acting user's current tenant context at the data-access layer, not only in the user interface.
- **FR-002**: A conversation MUST record the customer it is with, the channel it takes place on (from the platform's supported channel set: email, phone, web chat, WhatsApp, Telegram), its status, its assignee (or unassigned), and its created and last-activity times.
- **FR-003**: A conversation MUST have exactly one status at a time, from a fixed set: open, pending, resolved, and closed; new conversations start as open.
- **FR-004**: A conversation MUST track its participants: the customer and every team member who has sent a message or note in it, so viewers can see who has been involved.
- **FR-005**: A conversation MUST support messages of two kinds — customer-facing messages (from the customer or from team members) and internal notes (from team members only); internal notes MUST be permanently marked as internal and never presented as part of what the customer sees.
- **FR-006**: Every message MUST record its conversation, its sender (customer or specific team member), its body text, its kind (customer-facing or internal note), and the time it was sent; customer-authored messages logged manually by a team member MUST additionally record which member logged them. Message records MUST be immutable once created (no editing or deleting in this feature).
- **FR-007**: The message timeline MUST present messages in chronological send order with a stable, deterministic order for messages with identical times; the same conversation MUST always present the same order.
- **FR-008**: Tenant members MUST be able to view a paginated inbox of their tenant's conversations, ordered by most recent activity first, showing at minimum customer, channel, status, assignee, latest-message preview, and last-activity time; the default (unfiltered) inbox view MUST show open conversations only, with other statuses reachable via the status filter.
- **FR-009**: Tenant members MUST be able to filter the inbox by status, by assignment (assigned to me, unassigned, any specific member), and by channel, individually or in combination; filters MUST respect tenant scope and pagination.
- **FR-010**: Tenant members MUST be able to open a single conversation and view its details: customer, channel, status, assignee, participants, and full message timeline, with older portions of long timelines loadable on demand.
- **FR-011**: Permitted tenant members MUST be able to add a customer-facing reply, an internal note, or a manually logged customer-authored message (e.g., a phone-call transcription) to a conversation; the entry MUST appear in the timeline and MUST update the conversation's last-activity time. Logged customer messages count as customer-facing activity (including the auto-reopen rule in FR-012). Empty or whitespace-only bodies MUST be rejected, and message bodies MUST be limited to 10,000 characters with a clear message when exceeded.
- **FR-012**: Permitted tenant members MUST be able to change a conversation's status to any value in the fixed set, including reopening resolved or closed conversations. Adding a customer-facing reply to a resolved or closed conversation MUST automatically return it to open status (recorded like any status change); internal notes MUST never change status.
- **FR-013**: Permitted tenant members MUST be able to assign a conversation to any active member of the tenant (including themselves) or make it unassigned; assignment to anyone who is not an active tenant member MUST be refused.
- **FR-014**: Permitted tenant members MUST be able to create a new conversation by selecting an existing customer of their tenant, a channel, and a first message body; the conversation starts open and unassigned with that message as its first timeline entry. A customer MAY have any number of concurrent conversations, including multiple open conversations on the same channel — each is an independent thread and creation is never refused because other conversations exist.
- **FR-015**: Conversation viewing MUST be available to all tenant roles; replying, internal notes, status changes, assignment, and conversation creation MUST be restricted to a conversation-management permission held by Agent-level roles and above (Owner, Admin, Manager, Agent) — Viewer is read-only — enforced by the system (not only hidden in the interface).
- **FR-016**: Any attempt to access or modify a conversation or message belonging to a different tenant MUST be refused with a response indistinguishable from the conversation not existing.
- **FR-017**: Every conversation creation, status change, and assignment change MUST produce an append-only audit record capturing who performed the action, what changed, and when, using the platform's existing audit trail.
- **FR-018**: The customer profile's conversation history (introduced in the customer-profiles feature) MUST reflect real conversations from this system — channel, status, and last-activity time per conversation — replacing seeded summary data as its source without changing what the profile shows.
- **FR-019**: Automated tests MUST verify tenant isolation for every conversation operation (list, filter, view, create, add message, change status, assign), demonstrating that one tenant's conversations and messages are never readable or writable from another tenant's context, and MUST verify that message ordering is correct and stable.

### Key Entities

- **Conversation**: A single ongoing exchange between a tenant and one of its customers on one channel. Belongs to exactly one tenant; has a status, an optional assignee, participants, timestamps (created, last activity), and an ordered set of messages. Extends the minimal conversation summary introduced by the customer-profiles feature.
- **Message**: One immutable entry in a conversation's timeline: sender (the customer or a specific team member), body text, kind (customer-facing or internal note), and send time. Customer-authored messages logged manually by a team member also permanently record the logging member. Ordered chronologically within its conversation.
- **Internal Note**: A message kind authored by team members for teammates only. Occupies its chronological place in the timeline but is permanently marked as never visible to the customer.
- **Participant**: The association between a conversation and a person involved in it — the customer plus each team member who has contributed a message or note.
- **Assignment**: The link between a conversation and the single active tenant member currently responsible for it; a conversation may instead be unassigned. Changes are recorded with who made them and when.
- **Channel**: The communication medium a conversation takes place on, from the platform's fixed supported set (email, phone, web chat, WhatsApp, Telegram); referenced, not redefined, by this feature.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant member can go from opening the inbox to reading a specific conversation's timeline in under 15 seconds using filters and previews.
- **SC-002**: The inbox and its filters respond in under 1 second for tenants with up to 10,000 conversations; opening a conversation presents the recent timeline in under 1 second for conversations with up to 1,000 messages.
- **SC-003**: 100% of tenant-isolation tests pass: across every conversation operation, zero conversations or messages are ever exposed to or modifiable from another tenant's context.
- **SC-004**: 100% of timeline renderings present messages in correct chronological order with stable ordering across repeated views, including seeded same-instant messages.
- **SC-005**: A permitted member can send a reply, leave an internal note, change a status, and assign a conversation, each in a single action from the conversation detail page, with the result visible immediately.
- **SC-006**: 100% of permission checks hold: every compose, status, assignment, and creation attempt by a view-only member is refused, and no internal note is ever presented as customer-visible content.
- **SC-007**: Every conversation creation, status change, and assignment change is traceable to who did it and when via the audit trail.

## Assumptions

- No live channel delivery ships in this feature: replies composed in the dashboard are recorded in the conversation timeline as the system of record, but actual outbound sending (emails, WhatsApp messages, etc.) and inbound message ingestion from channels are out of scope, arriving with future channel-integration features. Conversation data therefore originates from manual creation, replies, manually logged customer messages (transcriptions marked with the logging member), and seeded test data.
- Real-time updates (live inbox refresh, typing indicators, unread counts) are out of scope; members see new data on navigation or refresh. The timeline and inbox designs must not preclude adding live updates later.
- The status set is fixed at open, pending, resolved, and closed, with any-to-any transitions permitted for permitted members (including reopening). The only automatic transition is the reopen-on-customer-facing-reply rule (FR-012); per-tenant custom statuses and time-based automatic transitions (e.g., auto-close after inactivity) are out of scope.
- A conversation has at most one assignee at a time; team-based or multi-assignee routing, workload balancing, and auto-assignment rules are out of scope.
- Message editing and deletion are out of scope; messages are immutable once sent. Attachments and rich formatting are out of scope — message bodies are plain text (up to 10,000 characters), displayed safely.
- Permissions follow the platform's existing tenant role model, mirroring the customer-profiles feature: all tenant roles can view conversations; a conversation-management permission held by Owner, Admin, Manager, and Agent gates composing, status changes, assignment, and creation, with Viewer read-only.
- Channels reuse the fixed set established by the customer-profiles feature (email, phone, web chat, WhatsApp, Telegram); this feature adds no new channels.
- This feature builds on the minimal conversation summary record introduced by the customer-profiles feature (012): it extends that record into full conversations rather than creating a parallel system, so the customer profile's history section continues to work unchanged.
- Conversations reference existing customers only; creating a customer inline while starting a conversation is out of scope (the member creates the customer first via the customers feature).
- Platform users operating in a tenant's context (via the tenant switcher) interact with conversations under that tenant's scope, subject to the same permission checks; there is no cross-tenant conversation view anywhere in the product.
- Search within conversations (by content or customer) is out of scope for this feature; the inbox is navigated via filters and ordering. AI agent participation in conversations arrives with later AI features.
