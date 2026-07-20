# Feature Specification: Notifications

**Feature Branch**: `027-notifications`

**Created**: 2026-07-20

**Status**: Draft

**Input**: User description: "Notifications — notify users about important events (new escalation, assigned conversation, mention, failed AI response, tool approval required). Backend: notification model, notification service, in-app notifications, future-ready email notification design. Frontend: notification bell, notification list, unread count, mark as read. Acceptance: users receive in-app notifications; notifications are tenant-scoped; unread count works; notifications link to relevant pages."

## Clarifications

### Session 2026-07-20

- Q: When a fanned-out event (queued escalation, tool approval) is acted on by one recipient, what happens to the other recipients' notifications? → A: Auto-resolve for others — they are marked resolved and drop out of the unread count.
- Q: Should users be able to dismiss or delete their own notifications? → A: No — read-only list, cleared only by the retention rule.
- Q: An escalation routed to an agent also assigns the conversation to them — should they get two notifications? → A: No — the escalation notification is created and the assignment notification is suppressed.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See and act on my unread notifications (Priority: P1)

A staff member working in the dashboard sees a bell in the top bar with a count of
how many notifications they have not yet read. Opening the bell shows their most
recent notifications, newest first, each with a short description of what happened
and when. Selecting one takes them straight to the thing it is about (for example
the conversation that was assigned to them) and marks that notification read, so
the count goes down. They can also mark a single notification read without opening
it, or mark everything read at once.

**Why this priority**: This is the entire user-facing surface of the feature. Without
it, events may be recorded but no one is notified. Delivered alone — with even one
event type feeding it — it already replaces "keep refreshing the conversation list".

**Independent Test**: Seed one notification for a member, sign in as that member, and
confirm the bell shows a count of 1, the list shows the notification, selecting it
navigates to the linked page, and the count returns to 0.

**Acceptance Scenarios**:

1. **Given** a member with 3 unread notifications, **When** they load any dashboard
   page, **Then** the bell displays an unread count of 3.
2. **Given** the bell is open, **When** the member selects a notification, **Then** they
   are navigated to the page for the subject of that notification and the
   notification becomes read.
3. **Given** a member with unread notifications, **When** they choose "mark all as
   read", **Then** the unread count becomes 0 and every listed notification shows as
   read.
4. **Given** a member with no notifications, **When** they open the bell, **Then** they
   see an empty state and no unread count badge.
5. **Given** a member with more notifications than fit in the bell panel, **When** they
   open the full notification list, **Then** they can page through older
   notifications in reverse chronological order.
6. **Given** a notification whose subject has since been deleted or is no longer
   visible to the member, **When** they select it, **Then** they are told the target is
   unavailable instead of landing on an error page.

---

### User Story 2 - Get told when work lands on me (Priority: P1)

A staff member is notified when work becomes their responsibility: a conversation is
assigned to them, or an escalation from AI to a human is raised and needs to be
picked up by someone on their team.

**Why this priority**: These are the highest-volume, most time-sensitive events. An
unnoticed escalation is a customer waiting on nobody. This is the primary reason the
feature exists.

**Independent Test**: Assign a conversation to a member via the existing conversation
update flow and confirm a notification appears for exactly that member and no one
else; raise an escalation and confirm the intended recipients receive one.

**Acceptance Scenarios**:

1. **Given** a conversation assigned to member A, **When** a manager reassigns it to
   member B, **Then** member B receives an "assigned to you" notification and member A
   does not receive one.
2. **Given** an escalation is raised and routed to a specific available agent, **When**
   the routing completes, **Then** that agent receives exactly one notification — the
   "new escalation" one — and no separate "assigned to you" notification for the same
   action.
3. **Given** an escalation is raised but no agent is available and it enters the
   queue, **When** the escalation is queued, **Then** members who are able to claim
   queued escalations receive a "new escalation" notification.
4. **Given** several members were notified about a queued escalation, **When** one of
   them claims it, **Then** the remaining members' notifications become resolved and
   their unread counts decrease without any action on their part.
5. **Given** a member assigns a conversation to themselves, **When** the assignment
   saves, **Then** no notification is created (people are not notified about their own
   actions).
6. **Given** notifications exist in tenant X, **When** a member of tenant Y (including
   a user who belongs to both) views notifications while in tenant Y's context,
   **Then** none of tenant X's notifications are visible.

---

### User Story 3 - Get told when the AI needs attention (Priority: P2)

A staff member responsible for AI behaviour is notified when an AI reply attempt
fails, and when the AI has requested a tool action that cannot proceed without
explicit human approval.

**Why this priority**: Lower volume than assignment and escalation, but these events
silently stall conversations if unnoticed. Valuable, but the feature is already
useful without them.

**Independent Test**: Force an AI generation failure and a tool request that requires
approval, then confirm the appropriate recipients receive notifications that link to
the affected conversation.

**Acceptance Scenarios**:

1. **Given** an AI reply attempt for a conversation fails, **When** the failure is
   recorded, **Then** the designated recipients receive a "failed AI response"
   notification linking to that conversation.
2. **Given** the AI requests a tool action that requires approval, **When** the request
   enters the awaiting-approval state, **Then** members permitted to decide tool
   approvals receive a "tool approval required" notification linking to the pending
   request.
3. **Given** a tool approval notification was sent to several members, **When** one of
   them decides the request, **Then** the others' notifications become resolved, drop
   out of their unread counts, and remain visible in their lists linking to the
   already-decided request.
4. **Given** repeated AI failures on the same conversation within a short window,
   **When** the failures occur, **Then** recipients are not flooded with a separate
   notification for every attempt.

---

### Edge Cases

- A member is deactivated or removed from the tenant while unread notifications
  exist for them — their notifications must stop being reachable, and their unread
  count must not resurface if they are re-added later without new events.
- The same underlying event is processed more than once (retry, replay) — the member
  must not receive duplicate notifications for it.
- A member belongs to multiple tenants — the unread count shown reflects only the
  tenant they are currently working in.
- An event has no valid recipient (for example an escalation with no eligible
  claimer) — the event must not fail, and the absence of recipients must be
  observable rather than silent.
- A notification's linked page requires a permission the recipient has since lost —
  they must be told the target is unavailable rather than shown a permission error.
- Very old notifications accumulate — the list must remain responsive and older
  entries must be aged out on a defined schedule.
- A member has the dashboard open when an event fires — the unread count must update
  without requiring a manual page reload.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST record a notification for each recipient of a supported
  event, capturing the event type, the recipient, the tenant, a human-readable
  title and body, a link target, the read state, and the time it was created.
- **FR-002**: System MUST scope every notification to exactly one tenant and MUST
  NOT expose a notification to any user outside that tenant, regardless of the
  user's other tenant memberships.
- **FR-003**: System MUST support these event types: new escalation, conversation
  assigned, AI response failed, and tool approval required.
- **FR-004**: System MUST allow a new event type to be introduced by defining its
  recipient rule, display wording, and link target, without changing how
  notifications are stored, listed, counted, or marked read.
- **FR-005**: Users MUST be able to retrieve their own notifications for the current
  tenant, newest first, with pagination for older entries.
- **FR-006**: Users MUST be able to retrieve the count of their unread notifications
  for the current tenant.
- **FR-007**: Users MUST be able to mark a single notification as read and to mark
  all of their unread notifications as read.
- **FR-007a**: Users MUST NOT be able to dismiss, archive, or delete notifications;
  the list is cleared only by the retention rule in FR-016.
- **FR-008**: Every notification MUST carry a navigable target identifying the entity
  it concerns (conversation, escalation, or tool request), sufficient for the
  dashboard to route the user to the corresponding page.
- **FR-009**: System MUST NOT notify a user about an event that user themselves
  caused.
- **FR-009a**: When a single action produces more than one qualifying event for the
  same recipient and subject, System MUST create only the most specific
  notification. Specifically, a conversation assignment caused by escalation routing
  MUST produce the escalation notification only.
- **FR-010**: System MUST NOT create duplicate notifications when the same source
  event is processed more than once.
- **FR-011**: System MUST determine recipients per event type from tenant membership,
  role, and assignment state, and MUST evaluate recipients at the time the event
  occurs.
- **FR-011a**: For events representing claimable work delivered to multiple
  recipients, System MUST automatically resolve the remaining recipients'
  notifications once the underlying work is claimed or decided, so that resolved
  notifications no longer count as unread.
- **FR-011b**: A resolved notification MUST remain visible in the recipient's list,
  distinguishable from unread and read entries, and MUST still navigate to its
  subject.
- **FR-012**: Users MUST NOT be able to read, mark, or otherwise act on notifications
  belonging to another user, including other members of their own tenant.
- **FR-012a**: Access to one's own notifications MUST NOT require any role-based
  permission beyond active membership in the current tenant — every tenant role,
  including Viewer, has a working notification inbox.
- **FR-013**: The dashboard MUST surface the unread count persistently in the
  application shell on every authenticated page.
- **FR-014**: The unread count MUST update without a manual page reload when a new
  notification arrives for the signed-in member in their current tenant.
- **FR-015**: System MUST define the notification model so that additional delivery
  channels — email first — can be added by introducing a delivery mechanism only,
  without changing how events are recorded or how recipients are resolved. Actual
  email delivery is out of scope for this feature.
- **FR-016**: System MUST retain notifications for a defined period and age out
  entries older than that period.
- **FR-017**: Creating notifications MUST NOT block or fail the originating action;
  an event whose notification cannot be recorded MUST still complete and MUST be
  observable as a failure.
- **FR-018**: System MUST record notification volume and failures in a form that
  operators can inspect.

### Key Entities

- **Notification**: One record of "this recipient should know about this event".
  Belongs to exactly one tenant and one recipient. Carries an event type, a title
  and body suitable for display, a reference to the subject entity, its state, and
  creation time. State is one of **unread** (counts toward the badge), **read** (the
  recipient opened or marked it), or **resolved** (the underlying work was handled by
  someone else — see FR-011a). Only unread notifications count toward the badge.
- **Notification Recipient**: The tenant member a notification is addressed to.
  Notifications are addressed per member, not per role or per team — a role-targeted
  event fans out to one notification per qualifying member.
- **Notification Event Type**: The classification of what happened (new escalation,
  conversation assigned, AI response failed, tool approval required). Drives
  recipient resolution, display wording, and link target.
- **Delivery Channel**: How a notification reaches a recipient. In-app is the only
  channel in this feature; the model must accommodate email as a second channel
  later.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of qualifying events produce a notification for every intended
  recipient and for no one else, verified across all supported event types.
- **SC-002**: A newly created notification is reflected in the recipient's unread
  count within 5 seconds without a page reload.
- **SC-003**: Zero cross-tenant leakage: no user can retrieve or act on a
  notification belonging to a tenant they are not currently working in, or to
  another user, under any tested request.
- **SC-004**: Opening the notification panel displays results in under 1 second for a
  member holding 1,000 notifications.
- **SC-005**: 100% of notifications navigate to a page about their subject entity, or
  to a clear "no longer available" state — never to an error page.
- **SC-006**: A member can go from noticing the bell to viewing the underlying work
  item in 2 interactions.
- **SC-007**: Repeated processing of the same source event produces exactly one
  notification per recipient.
- **SC-008**: Adding email delivery later requires no change to event recording or
  recipient resolution, demonstrated by the model review at plan time.
- **SC-009**: When claimable work is taken by one recipient, every other recipient's
  unread count reflects the resolution within 5 seconds, with no action on their part.
- **SC-010**: A single action never produces more than one notification for the same
  recipient about the same subject.

## Assumptions

- **Recipient rules** (defaults chosen where the description did not specify):
  - *Conversation assigned* → the new assignee only.
  - *New escalation, routed* → the agent it was routed to.
  - *New escalation, queued with no assignee* → tenant members who can claim queued
    escalations (holders of `conversations.manage`).
  - *Tool approval required* → tenant members permitted to decide tool approvals
    (holders of `conversations.manage`).
  - *AI response failed* → tenant Owners and Admins, plus the conversation's current
    assignee if there is one.
- **Mentions are out of scope for this feature.** The original request listed
  "mention" as a trigger, but the platform has internal notes without any @mention
  authoring capability, so the trigger has no source event to fire from. Building
  @mention authoring is a separate feature that will add its own notification type
  on top of FR-004's extension point.
- Notifications are for tenant users only. Platform users acting inside a tenant
  context are out of scope for this feature.
- Email delivery is designed for but not implemented: no messages are sent, no
  per-user delivery preferences or digest scheduling exist yet. The existing email
  sending abstraction used for team invitations is the intended future transport.
- There are no per-user notification preferences, mute controls, or per-event opt-out
  in this feature; every qualifying recipient is notified.
- Notification text is English-only; localization is out of scope.
- Notifications are informational records, not the audit trail — the existing audit
  log remains the system of record for who did what.
- Retention default is 90 days, consistent with a working notification inbox rather
  than a compliance record. Aged-out notifications are permanently removed, not
  archived — the audit log remains the durable record of what happened.
- Recipient rules for claimable work (queued escalation, tool approval) resolve to
  holders of `conversations.manage`, which is the permission that already guards both
  claiming escalations and deciding tool requests.
- The dashboard's existing realtime event stream and tenant-context switching are
  reused rather than replaced.
- Read state is per notification, not per subject entity — reading a conversation
  does not implicitly clear notifications about it.
- AI failure notifications are suppressed for repeated failures on the same
  conversation within a 15-minute window.
