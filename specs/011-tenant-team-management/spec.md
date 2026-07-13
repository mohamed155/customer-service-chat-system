# Feature Specification: Tenant Team Management

**Feature Branch**: `011-tenant-team-management`

**Created**: 2026-07-12

**Status**: Draft

**Input**: User description: "Tenant Team Management. Goal: Allow tenant admins to manage their own team. Scope: List tenant users, Invite user, Change role, Disable user, View membership. Backend: implement tenant-scoped team APIs. Frontend: Team list page, Invite user dialog, Role selector, User status badge. Acceptance: Tenant admins can manage users in their tenant. Tenant users cannot manage users in other tenants. Role changes are audited. Viewer users cannot modify team members."

## Clarifications

### Session 2026-07-12

- Q: How are invitations delivered to the invited person? → A: Both mechanisms — the acceptance link is always shown and copyable in the app, and the system additionally sends the invitation email automatically when email delivery is configured; missing email delivery never blocks invitation creation (graceful degradation).
- Q: Can a managing role act on members senior to themselves (e.g., Manager re-roles an Admin)? → A: Strict hierarchy — a member may only manage (invite, re-role, disable) members whose role is below their own, and may only assign roles at or below their own rank; Owner-specific rules still apply.
- Q: Is the acceptance link a bearer token, or bound to the invited email? → A: Email-bound with verification — acceptance succeeds only for an account whose email matches the invited address, and membership activates only after the invited address is verified as controlled by the accepter.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - View the team roster (Priority: P1)

A tenant member opens the Team page and sees everyone who belongs to their workspace: each person's name, email, role, membership status (active, invited, or disabled), and when they joined. The list shows only people in the viewer's own tenant — never members of any other tenant.

**Why this priority**: The roster is the foundation every other team action builds on — you cannot invite, re-role, or disable someone you cannot see. On its own it already delivers value: admins can answer "who has access to our workspace and with what role?", which is a core accountability question for any business customer.

**Independent Test**: Sign in as a member of a tenant that has several memberships in different states, open the Team page, and verify the list matches that tenant's membership records exactly — and contains nothing from any other tenant.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant Admin whose tenant has active, invited, and disabled members, **When** they open the Team page, **Then** all of those members are listed with name, email, role, and a status indicator that distinguishes active, invited, and disabled.
2. **Given** two tenants each with their own members, **When** a member of tenant A views the Team page, **Then** no member of tenant B appears, and any direct attempt to request tenant B's roster is refused.
3. **Given** a signed-in Support Agent or Viewer (roles without roster visibility in the established access-control model), **When** they use the dashboard, **Then** no Team page is offered to them, and a direct attempt to open it or request the roster is refused.
4. **Given** a tenant with more members than fit on one page, **When** an admin browses the roster, **Then** they can page through the full list and find a specific person by name or email.

---

### User Story 2 - Invite a new team member (Priority: P2)

A tenant Admin invites a colleague by entering their email address and choosing the role they should hold. The invitation appears in the roster as a pending member, and the invited person can use their invitation to join the workspace with exactly the role that was assigned. The admin can revoke an invitation that has not been accepted.

**Why this priority**: Growing the team is the first thing an admin needs after seeing the roster; without invitations a tenant is frozen at its initial membership. It depends on Story 1's page but delivers standalone value the moment it ships.

**Independent Test**: As a tenant Admin, invite a new email address with the Support Agent role, verify a pending entry appears in the roster, complete the invitation as the invited person, and confirm they can sign in as a Support Agent of that tenant and of no other tenant.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant Admin on the Team page, **When** they invite a new email address and select a role, **Then** the invitation is created, recorded in the audit trail, and shown in the roster as an invited (pending) member with the chosen role.
2. **Given** a pending invitation, **When** the invited person accepts it before it expires, **Then** they become an active member of that tenant with the invited role, and the roster reflects the change.
3. **Given** an email address that already belongs to an active member of the tenant, **When** an admin tries to invite it, **Then** the invitation is refused with a clear explanation and no duplicate membership is created.
4. **Given** a pending invitation, **When** an admin revokes it, **Then** the invitation can no longer be accepted and the revocation is recorded in the audit trail.
5. **Given** an invitation that has passed its validity window, **When** the invited person tries to accept it, **Then** acceptance is refused with a clear message, and an admin can issue a fresh invitation.
6. **Given** a signed-in Viewer or Support Agent, **When** they attempt to create an invitation (through the app or by calling the operation directly), **Then** the attempt is refused and no invitation is created.
7. **Given** a valid acceptance link, **When** it is opened by someone signed in with (or registering) an email other than the invited address, **Then** acceptance is refused and no membership is created — the link is bound to the invited email, not a bearer token.
8. **Given** an accepter presenting a valid acceptance credential whose account email matches the invited address, **When** they accept, **Then** the membership activates — and the credential is thereby consumed, so it cannot activate a second membership (single use establishes that the invited address's confirmation was redeemed exactly once).

---

### User Story 3 - Change a member's role (Priority: P3)

A tenant Admin changes a team member's role — for example promoting a Support Agent to Manager or reducing an Admin to Viewer. The change takes effect immediately, is visible in the roster, and is recorded in the audit trail with who made the change, whose role changed, and the before/after roles.

**Why this priority**: Role changes are how a team adapts over time. They are less frequent than viewing and inviting, but the audit requirement makes them a named acceptance criterion of this feature.

**Independent Test**: As a tenant Admin, change another member's role, verify the roster shows the new role, the affected user's very next action is evaluated under the new role, and the audit trail contains a complete who/what/when record of the change.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant Admin, **When** they change another member's role, **Then** the roster shows the new role and an audit record captures actor, affected member, previous role, new role, and time.
2. **Given** a member whose role was just changed while they were signed in, **When** that member performs their next action, **Then** the action is evaluated under the new role, and their visible navigation reflects it without requiring sign-out.
3. **Given** a signed-in Admin (not Owner), **When** they attempt to assign the Owner role or change the Owner's role, **Then** the attempt is refused — only the Owner can assign or transfer ownership.
4. **Given** the tenant's only Owner, **When** anyone attempts to change that Owner to a lesser role, **Then** the attempt is refused so the tenant is never left without an Owner.
5. **Given** any signed-in member, **When** they attempt to change their own role, **Then** the attempt is refused.
6. **Given** a signed-in Viewer, **When** they attempt a role change by calling the operation directly, **Then** the request is refused and no change or audit entry for a change occurs.
7. **Given** a signed-in Manager, **When** they attempt to change or disable an Admin, or to promote anyone to Admin, **Then** the attempt is refused — a member may only manage members below their own rank and assign roles at or below it.

---

### User Story 4 - Disable and re-enable a team member (Priority: P4)

A tenant Admin disables a member who should no longer have access — a departed employee or a compromised account. The member immediately loses access to the tenant, remains visible in the roster marked as disabled, and can be re-enabled later with their previous role intact.

**Why this priority**: Removing access is essential for security hygiene, but it is needed less often than the day-to-day flows above. Disabling (rather than deleting) preserves history and keeps the action reversible.

**Independent Test**: As a tenant Admin, disable an active member, verify their very next request to the tenant is refused, confirm the roster shows them as disabled with an audit record, then re-enable them and verify access is restored with the same role.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant Admin, **When** they disable an active member, **Then** the member's status shows disabled in the roster and the action is recorded in the audit trail.
2. **Given** a member who was just disabled while signed in, **When** they make their next request to the tenant, **Then** it is refused — access ends immediately, not at their next sign-in.
3. **Given** a disabled member, **When** an admin re-enables them, **Then** they regain access with the role they held before being disabled, and the re-enablement is audited.
4. **Given** any signed-in member, **When** they attempt to disable themselves, **Then** the attempt is refused.
5. **Given** the tenant's only Owner, **When** anyone attempts to disable them, **Then** the attempt is refused.
6. **Given** a member who is disabled in tenant A but active in tenant B, **When** they use the product, **Then** tenant B access is unaffected — disabling is scoped to a single tenant's membership.

---

### Edge Cases

- A user who belongs to several tenants opens the Team page: they see and act on only the tenant they are currently working in; switching tenants switches the roster.
- An admin invites an email address that already has a pending invitation for this tenant: the duplicate is refused; the admin may revoke the pending invitation and issue a new one instead.
- An admin invites a person who already has an account from another tenant: accepting adds a membership in this tenant with the invited role; their other memberships are untouched.
- The inviter's own access is revoked or downgraded after they sent an invitation: the invitation remains valid unless explicitly revoked.
- Concurrent conflicting changes (two admins edit the same member at once): the outcome is one consistent final state, and both actions appear in the audit trail.
- A manage-team request names a member who belongs to a different tenant: refused as not found or forbidden — the response must not confirm the member's existence elsewhere.
- A disabled member tries to accept a new invitation to the same tenant: refused; the path back is re-enablement, not a second membership.
- The roster is requested for a tenant the caller does not belong to (crafted request): refused before any data is read.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST let tenant members whose role includes roster visibility (Owner, Admin, Manager — per the established access-control model) view their tenant's team roster, listing each member's name, email, role, membership status (active, invited, disabled), and join date, with the ability to page through large teams and search by name or email. Roles without roster visibility (Support Agent, Viewer) are refused server-side and see no Team page.
- **FR-002**: All team management data and operations MUST be scoped to the caller's current tenant; requests that reference another tenant's roster, members, or invitations MUST be refused without revealing whether the referenced records exist.
- **FR-003**: Team management actions (invite, revoke invitation, change role, disable, re-enable) MUST be permitted only to tenant members whose role includes member management (Owner, Admin, Manager per the established role model); Support Agent and Viewer roles MUST be refused, and refusal MUST be enforced server-side, not only hidden in the interface.
- **FR-004**: Members with management rights MUST be able to invite a person by email address with an assigned tenant role; the invitation MUST appear in the roster as a pending member until accepted, revoked, or expired.
- **FR-005**: An invitation MUST be acceptable only within its validity window and only once; accepting MUST create an active membership with exactly the invited role — for a new person by completing account creation, for an existing account holder by adding the membership to their account.
- **FR-005a**: Acceptance MUST be bound to the invited identity: each invitation carries a single-use, unguessable acceptance credential, and the accepting account's email address MUST match the invited address (a new registrant's email is fixed to the invited address and not editable). Control of the invited address is established by this combination — when email delivery is configured, the credential reaches the invitee only through that mailbox, so presenting it constitutes the confirmation sent to that address; when email delivery is not configured, the admin hand-delivers the acceptance link and the email-match rule still applies. A valid link presented by an account with a different email MUST be refused — the link alone never grants access.
- **FR-006**: The system MUST refuse an invitation to an email address that is already an active or disabled member of the tenant, or that already has a pending invitation for the tenant, with a message stating why.
- **FR-007**: Members with management rights MUST be able to revoke a pending invitation; a revoked invitation MUST no longer be acceptable.
- **FR-008**: Members with management rights MUST be able to change another member's role among the tenant role set, subject to: only the Owner may assign or transfer the Owner role or change an Owner's role; no member may change their own role; the tenant's last remaining Owner may never be demoted.
- **FR-009**: Members with management rights MUST be able to disable an active member and re-enable a disabled member, subject to: no member may disable themselves; the tenant's last remaining Owner may never be disabled; re-enablement restores the role held at the time of disabling.
- **FR-010**: A role change or disablement MUST take effect immediately: the affected member's very next request is evaluated under the new role or refused, and their open session's visible navigation updates accordingly (consistent with the platform's existing mid-session revocation behavior).
- **FR-011**: Every team management action — invitation created, invitation revoked, invitation accepted, role changed, member disabled, member re-enabled — MUST be recorded in the tenant's audit trail with actor, affected member or invitee, action, prior and new values where applicable, and timestamp.
- **FR-012**: Disabling MUST be scoped to the single tenant membership: the person's account and their memberships in other tenants remain unaffected.
- **FR-013**: The Team page MUST present each member's status as a clearly distinguishable badge (active, invited, disabled) and offer management actions only to users whose role permits them.
- **FR-014**: The invite flow MUST be presented as a dialog from the Team page where the admin enters an email address and selects a role from the tenant role set, with validation feedback for invalid or duplicate email addresses before submission completes. The role selector MUST offer only roles the inviter is permitted to assign under FR-016.
- **FR-015**: When an invitation is created, its acceptance link MUST always be presented to the inviter for copying; in addition, if email delivery is configured for the environment, the system MUST automatically send the invitation to the invited address. Unavailable or failed email delivery MUST NOT block invitation creation — the copyable link remains the guaranteed delivery path, and the inviter can see whether the email was sent.
- **FR-016**: Management actions MUST respect the tenant role hierarchy (Owner above Admin above Manager above Support Agent above Viewer): a member may invite, re-role, or disable only members whose current role is below their own, and may assign only roles at or below their own rank. Attempts outside these bounds MUST be refused server-side. The Owner-specific rules in FR-008 and FR-009 apply in addition.

### Key Entities

- **Team Member (membership)**: A person's participation in one tenant — identity (name, email), tenant role, status (active or disabled), and join date. One person may hold memberships in several tenants, each independent.
- **Invitation**: A pending offer for a specific email address to join a specific tenant with a specific role. Has an issuer, an issue time, a validity window, and a lifecycle: pending → accepted, revoked, or expired.
- **Tenant Role**: One of the fixed tenant role set — Owner, Admin, Manager, Support Agent, Viewer — as defined by the existing access-control model; this feature assigns and changes them but does not alter what each role permits.
- **Audit Record**: An append-only who/what/when entry describing a team management action, including before/after values for role changes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant admin can find any member and complete an invitation, role change, or disablement in under 1 minute from opening the Team page.
- **SC-002**: 100% of team management actions (invites, revocations, acceptances, role changes, disables, re-enables) appear in the audit trail with actor, target, action, and time; role changes always include before and after roles.
- **SC-003**: Zero cross-tenant exposure: in testing across at least two tenants, every attempt to view or modify another tenant's members — through the app or crafted requests — is refused, with no data returned.
- **SC-004**: 100% of modification attempts by Viewer and Support Agent roles are refused server-side, even when the request bypasses the interface.
- **SC-005**: A disabled member's access ends by their next interaction — no disabled member successfully performs a tenant action after disablement in any test run.
- **SC-006**: An invited person can go from receiving an invitation to working inside the tenant in under 5 minutes without assistance.
- **SC-007**: The team roster remains browsable and searchable without degradation for tenants with up to 500 members.

## Assumptions

- **Role model is inherited, not redefined**: The tenant role set (Owner, Admin, Manager, Support Agent, Viewer) and what each role may do come from the existing RBAC feature (008). Per that model, member management **and** roster visibility belong to Owner, Admin, and Manager; this spec's "tenant admins" acceptance criterion therefore covers all three managing roles. Support Agent and Viewer have no Team page access — they can neither view nor modify the roster (the original acceptance criterion "Viewer users cannot modify team members" is thereby satisfied a fortiori). Widening roster visibility would be a change to the 008 permission matrix, out of scope here.
- **Invitation delivery**: Dual-path per clarification — the acceptance link is always shown and copyable in the invite flow, and the system also emails the invitation automatically when email delivery is configured. Environments without configured email delivery remain fully functional via the copyable link.
- **Invitation validity**: Invitations expire after 7 days by default; expired invitations cannot be accepted and are replaced by issuing a new invitation.
- **Disable, not delete**: Removing someone permanently (deleting the membership record) is out of scope; disabling is the supported way to end access, preserving history and auditability.
- **Existing sign-in applies**: Invited people authenticate through the platform's existing sign-in; a brand-new invitee completes account creation as part of accepting the invitation. Per clarification, acceptance is email-bound: the account email must match the invited address and be verified before the membership activates.
- **Platform-side administration is separate**: Platform staff managing tenants from the platform area (feature 010) is unaffected; this feature is the tenant-facing, self-service surface only.
- **Ownership transfer**: Assigning the Owner role to another member is permitted only for the current Owner and results in a role change recorded like any other; whether the previous Owner is simultaneously demoted is the Owner's explicit choice via a second role change.
