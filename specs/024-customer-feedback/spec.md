# Feature Specification: Customer Feedback

**Feature Branch**: `024-customer-feedback`

**Created**: 2026-07-19

**Status**: Draft

**Input**: User description: "Customer Feedback — Collect post-conversation feedback. Use 5-star rating with optional free-text comment. Rating from 1 to 5, optional comment, feedback per conversation, analytics-ready storage. Backend: store feedback, prevent duplicate feedback per conversation/customer session, associate feedback with channel, AI agent, and assigned human agent where available. Frontend: feedback component in widget, feedback display in conversation detail, satisfaction badge. Customers can leave a 5-star rating, optionally leave a comment, feedback is tenant-scoped, feedback can be used later in analytics."

## Clarifications

### Session 2026-07-19

- Q: Can a customer change their feedback after submitting it? → A: Immutable — one submission per conversation, no edits afterwards.
- Q: Where does the satisfaction badge appear? → A: Conversation detail view and conversation list rows.
- Q: How long after a conversation ends can feedback still be submitted? → A: While the customer session can still access the conversation (session lifetime); no separate expiry window.
- Q: Does any aggregate satisfaction metric ship in this feature? → A: One simple aggregate now — tenant-wide average rating + feedback count in the dashboard; fuller analytics remain a future feature.
- Q: What happens after the customer dismisses the feedback prompt? → A: Prompt collapses on dismissal; a passive "rate this conversation" entry point remains available for the session lifetime.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Customer rates a finished conversation (Priority: P1)

A customer who has just finished a support conversation in the website chat widget is invited to rate their experience. They pick a star rating from 1 to 5 and submit it in a single tap. The widget confirms the rating was received and thanks them.

**Why this priority**: This is the core value of the feature — without the ability to capture a rating, nothing else (comments, display, analytics) has any data to work with. It is independently shippable as an MVP.

**Independent Test**: Can be fully tested by ending a widget conversation, selecting a star rating, submitting, and verifying the rating is stored against that conversation for that tenant.

**Acceptance Scenarios**:

1. **Given** a widget conversation that has ended, **When** the customer selects 4 stars and submits, **Then** the feedback is saved with rating 4, linked to that conversation, and the widget shows a confirmation.
2. **Given** a widget conversation that has ended and already has feedback from this customer session, **When** the customer attempts to submit feedback again, **Then** no second feedback record is created for that conversation.
3. **Given** the feedback prompt is visible, **When** the customer dismisses it without rating, **Then** no feedback record is created and the conversation is otherwise unaffected.
4. **Given** a submission attempt with a rating outside 1–5, **Then** the submission is rejected and the customer is asked to pick a valid rating.

---

### User Story 2 - Customer adds an optional comment (Priority: P2)

After (or while) selecting a star rating, the customer can optionally write a short free-text comment explaining their rating, then submit rating and comment together.

**Why this priority**: Comments add qualitative context that makes low ratings actionable, but a rating alone is already valuable. Comments only make sense once ratings exist.

**Independent Test**: Can be tested by submitting a rating with a comment and verifying both are stored together; and by submitting a rating without a comment and verifying it succeeds.

**Acceptance Scenarios**:

1. **Given** the feedback form with a selected rating, **When** the customer types a comment and submits, **Then** both rating and comment are saved on the same feedback record.
2. **Given** the feedback form with a selected rating and an empty comment field, **When** the customer submits, **Then** the feedback is saved with no comment and submission succeeds.
3. **Given** a comment longer than the allowed maximum length, **When** the customer submits, **Then** the customer is informed of the limit and the submission is not silently truncated.

---

### User Story 3 - Team reviews feedback on a conversation (Priority: P2)

A tenant user (agent, manager, admin) opens a conversation in the dashboard's conversation detail view and sees the customer's rating and comment for that conversation, along with a satisfaction badge that makes the rating visible at a glance.

**Why this priority**: Feedback is only useful if the team can see it. This closes the loop for agents and managers reviewing individual conversations.

**Independent Test**: Can be tested by submitting feedback on a conversation via the widget, then opening that conversation in the dashboard and verifying the rating, comment, and badge are displayed.

**Acceptance Scenarios**:

1. **Given** a conversation with submitted feedback, **When** a tenant user opens its detail view, **Then** they see the star rating, the comment (if any), and when the feedback was given.
2. **Given** a conversation with feedback, **When** it appears in the conversation list or its detail view, **Then** the satisfaction badge reflects the rating value.
3. **Given** a conversation without feedback, **When** a tenant user opens its detail view, **Then** the feedback area indicates no feedback was given (no badge or an explicit "no rating" state).
4. **Given** a user from tenant A, **When** they access conversations, **Then** they can never see feedback belonging to another tenant.

---

### User Story 4 - Feedback is attributable for later analytics (Priority: P3)

Each stored feedback record carries enough context — tenant, conversation, channel, the AI agent involved, and the assigned human agent where one exists — that future analytics can aggregate satisfaction by any of these dimensions without reprocessing conversations.

**Why this priority**: Analytics dashboards are out of scope for this feature, but storing attribution now is what makes them possible later without backfilling.

**Independent Test**: Can be tested by submitting feedback on (a) an AI-only conversation and (b) a conversation that was handed off to a human agent, then verifying each record carries the correct channel, AI agent, and human-agent attribution.

**Acceptance Scenarios**:

1. **Given** feedback on an AI-only widget conversation, **Then** the stored record references the tenant, conversation, channel, and AI agent, with no human agent.
2. **Given** feedback on a conversation that was escalated and assigned to a human agent, **Then** the stored record additionally references that human agent.
3. **Given** stored feedback records, **Then** ratings, timestamps, and attribution can be queried per tenant without inspecting conversation content.

---

### User Story 5 - Manager sees tenant-wide satisfaction at a glance (Priority: P3)

A tenant user sees a single tenant-wide satisfaction summary in the dashboard: the average rating and the number of feedback submissions for their tenant.

**Why this priority**: A first, cheap signal of overall satisfaction that proves out the analytics-ready storage; deeper breakdowns stay with a future analytics feature.

**Independent Test**: Can be tested by submitting feedback on several conversations and verifying the displayed average and count match, and that another tenant's numbers are unaffected.

**Acceptance Scenarios**:

1. **Given** a tenant with feedback records, **When** a tenant user views the dashboard summary, **Then** they see the correct average rating and total feedback count for their tenant only.
2. **Given** a tenant with no feedback yet, **When** a tenant user views the summary, **Then** an explicit empty state is shown rather than a zero or misleading average.

---

### Edge Cases

- Customer closes the widget or browser before submitting: no feedback is recorded; if the same ended conversation is reopened within the same customer session, the passive "rate this conversation" entry point is available (no active re-prompt after a dismissal).
- Conversation was handled by multiple human agents over its lifetime: feedback is attributed to the agent assigned at the time the conversation ended.
- Feedback submitted for a conversation that does not belong to the customer's session: rejected.
- Feedback submitted twice concurrently (e.g., double-click or retry after a network timeout): exactly one feedback record exists afterwards, and the customer sees a success state rather than an error.
- Conversation is deleted or archived after feedback exists: feedback remains available for analytics (attribution survives as historical fact).
- Customer submits only a comment with no rating: rejected — a rating is required; a comment is not valid on its own.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST allow a customer to submit a star rating from 1 to 5 for a conversation they participated in, and MUST reject any value outside that range.
- **FR-002**: The system MUST allow an optional free-text comment to accompany a rating, up to a defined maximum length (assumed 2,000 characters), and MUST accept submissions with no comment.
- **FR-003**: The system MUST store at most one feedback record per conversation per customer session; duplicate submissions MUST NOT create additional records, including under concurrent/retried submissions.
- **FR-004**: Every feedback record MUST be tenant-scoped, and all feedback reads and writes MUST enforce tenant isolation.
- **FR-005**: Each feedback record MUST capture: the conversation, the channel the conversation occurred on, the AI agent involved (where applicable), the assigned human agent at conversation end (where one exists), the rating, the optional comment, and the submission time.
- **FR-006**: The widget MUST present a feedback prompt for an ended conversation at the customer's next interaction with the widget (opening it, or attempting to send a message), allowing the customer to rate, optionally comment, or dismiss without rating. On dismissal the prompt collapses and MUST NOT actively re-prompt, but a passive "rate this conversation" entry point MUST remain available while the session retains access to the conversation.
- **FR-007**: The widget MUST confirm successful submission to the customer and MUST NOT re-prompt for a conversation that already has feedback from that customer session.
- **FR-008**: The conversation detail view in the dashboard MUST display the rating, comment (if any), and submission time for conversations that have feedback, and an explicit no-feedback state otherwise.
- **FR-009**: A satisfaction badge MUST visually represent a conversation's rating in the conversation detail view and on conversation list rows; conversations without feedback show no badge.
- **FR-010**: Feedback submission MUST only be accepted from the customer session that owns the conversation; submissions for other conversations MUST be rejected.
- **FR-011**: Feedback records MUST be stored so that ratings and their attribution dimensions (tenant, channel, AI agent, human agent, time) are directly queryable for future analytics without parsing conversation content.
- **FR-012**: Feedback MUST be retained independently of later changes to the conversation (e.g., archival), preserving its analytics value.
- **FR-013**: Feedback MUST be immutable once submitted — no customer edits or resubmissions are accepted for a conversation that already has feedback.
- **FR-014**: Feedback MUST be accepted for an ended conversation for as long as the owning customer session retains access to that conversation; no separate expiry window applies, and loss of session access ends the ability to submit.
- **FR-015**: The dashboard MUST display one tenant-wide aggregate: the average rating and total feedback count for the current tenant. Per-agent, per-channel, and time-series breakdowns remain out of scope.

### Key Entities

- **Feedback**: A customer's post-conversation evaluation. Attributes: rating (integer 1–5), optional comment, submission time. Belongs to exactly one tenant and exactly one conversation; at most one per conversation per customer session.
- **Conversation** (existing): The subject of the feedback; supplies the channel, AI agent, and assigned-human-agent context captured on the feedback record.
- **Customer session** (existing, widget): The anonymous customer identity that authorizes submitting feedback for its own conversations and anchors duplicate prevention.
- **Channel / AI agent / Human agent** (existing): Attribution dimensions referenced by feedback for later analytics aggregation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A customer can go from "conversation ended" to "feedback submitted" in under 30 seconds with at most 3 interactions (select stars, optionally type comment, submit).
- **SC-002**: 100% of submitted feedback records carry tenant, conversation, and channel attribution; conversations with an assigned human agent at end carry that attribution in 100% of cases.
- **SC-003**: Zero duplicate feedback records exist per conversation/customer session, including under retried or concurrent submissions.
- **SC-004**: Zero feedback records are visible to, or writable by, users or sessions outside the owning tenant.
- **SC-005**: A tenant user viewing a conversation with feedback can identify the rating at a glance (badge) without opening any additional view.
- **SC-006**: Satisfaction data for any time period can be aggregated per tenant, channel, AI agent, or human agent from stored feedback alone, with no reprocessing of conversation transcripts.

## Assumptions

- The website chat widget (023) is the only customer-facing channel today, so the feedback prompt UI ships in the widget; the storage model is channel-aware so future channels (email, social, voice) can reuse it.
- "Conversation ended" (widget conversation reaching its resolved or closed state) is the trigger for showing the feedback prompt; feedback is not collected mid-conversation.
- The prompt appears at the customer's next widget interaction rather than instantly at conversation end. The platform emits no real-time "conversation closed" signal today, and adding one would require a new event type plus publishing from the conversations module — deferred to a future feature. A customer who leaves the widget open and idle while an agent ends the conversation sees the prompt when they next open the widget or attempt to send a message.
- One feedback record per conversation per customer session is the duplicate-prevention boundary, matching the widget's anonymous session model; a rating is required, a comment alone is not accepted.
- The maximum comment length is 2,000 characters — long enough for meaningful detail, short enough for review and analytics.
- When multiple human agents handled a conversation, attribution goes to the agent assigned when the conversation ended.
- Beyond the single tenant-wide average/count (FR-015), analytics dashboards, aggregate reports, and CSAT breakdowns are out of scope — this feature guarantees analytics-ready storage, per-conversation display, and that one aggregate.
- Feedback display in the dashboard follows existing tenant RBAC: any tenant user who can view a conversation can view its feedback.
