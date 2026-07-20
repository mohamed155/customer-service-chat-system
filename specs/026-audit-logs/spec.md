# Feature Specification: Audit Logs

**Feature Branch**: `026-audit-logs`

**Created**: 2026-07-19

**Status**: Draft

**Input**: User description: "Audit Logs — Track sensitive and important system actions. Scope: auth events, tenant changes, user role changes, prompt changes, AI provider changes, tool executions, billing changes. Backend: audit log service recording actor, action, target, tenant, metadata, timestamp; expose audit log APIs. Frontend: audit log table, filters, detail drawer. Acceptance: sensitive actions are logged; platform users can view platform audit logs; tenant admins can view tenant audit logs; audit logs cannot be edited by normal users."

## Clarifications

### Session 2026-07-19

- Q: Does the tenant audit view show all recorded audit entries or only the seven sensitive categories? → A: All recorded audit entries for the tenant; the seven scoped categories are guaranteed-covered, and filters let users narrow by category.
- Q: Which platform roles can view the platform-wide audit view? → A: All platform roles (Super Admin, Developer, Sales, Support, Finance), read-only.
- Q: Do tenant admins see platform-staff actions performed inside their tenant? → A: Fully visible — the entry appears in the tenant's audit view with the staff actor identified by name and marked as platform staff.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Tenant admin reviews their tenant's audit trail (Priority: P1)

A tenant Owner or Admin opens an "Audit Logs" page in the dashboard and sees a chronological table of sensitive actions that happened inside their tenant — who did what, to which resource, and when. They narrow the list with filters (date range, action category, actor) and click any row to open a detail drawer showing the full record, including contextual metadata (for example, which fields changed).

**Why this priority**: This is the core user-facing value of the feature — turning already-recorded audit data into something administrators can actually inspect. It answers the most common compliance and trust questions ("who changed this role?", "who edited the prompt?") and is independently shippable because sensitive actions are already being recorded today.

**Independent Test**: Sign in as a tenant Owner or Admin, perform a sensitive action (e.g., change a member's role), open the Audit Logs page, and confirm the action appears with correct actor, action, target, and timestamp; filter and open its detail drawer.

**Acceptance Scenarios**:

1. **Given** a tenant Admin is signed in, **When** they open the Audit Logs page, **Then** they see a paginated table of their tenant's audit entries, newest first, each showing actor, action, target, and timestamp.
2. **Given** the audit table is displayed, **When** the admin applies a date range, action category, or actor filter, **Then** the table shows only matching entries and indicates when no entries match.
3. **Given** the audit table is displayed, **When** the admin selects an entry, **Then** a detail drawer opens showing the complete record, including its metadata, in a readable form.
4. **Given** a tenant user without audit access (e.g., Agent or Viewer), **When** they attempt to open the Audit Logs page or request audit data directly, **Then** access is denied and no audit data is exposed.
5. **Given** two tenants exist, **When** an Admin of tenant A views audit logs, **Then** no entry belonging to tenant B is ever visible, regardless of filters used.

---

### User Story 2 - Platform user reviews platform-wide audit logs (Priority: P2)

A platform user (e.g., Super Admin or Support) opens a platform-level Audit Logs view that spans all tenants and also includes platform-level events that belong to no single tenant (e.g., tenant creation, platform sign-ins). They can filter by tenant to investigate a specific customer's history when handling a support or security inquiry.

**Why this priority**: Platform staff need cross-tenant visibility to investigate incidents and support requests, but this builds directly on the same viewing experience as User Story 1 and serves a much smaller audience, so it comes second.

**Independent Test**: Sign in as a platform user, open the platform Audit Logs view, confirm entries from multiple tenants and tenant-less platform events are visible, and filter down to a single tenant.

**Acceptance Scenarios**:

1. **Given** a platform user is signed in, **When** they open the platform Audit Logs view, **Then** they see audit entries across all tenants, including entries not associated with any tenant.
2. **Given** the platform audit view is displayed, **When** the platform user filters by a specific tenant, **Then** only that tenant's entries are shown.
3. **Given** a tenant user (any role), **When** they attempt to access the platform-wide audit view or its data, **Then** access is denied.

---

### User Story 3 - Every in-scope sensitive action leaves a complete record (Priority: P3)

An administrator can rely on the audit trail being complete for the defined scope: authentication events, tenant changes, user role changes, prompt changes, AI provider/configuration changes, tool executions, and billing changes all produce an audit record capturing actor, action, target, tenant, metadata, and timestamp. Categories that are recorded today keep working; the one verified gap — **tool executions**, where only tool *configuration* changes are recorded today — is brought up to the same standard. Billing is the sole category with nothing to record, because no billing actions exist in the product yet; the category is reserved and populates automatically once they do.

**Why this priority**: Coverage completeness is what makes the audit trail trustworthy, but most in-scope categories are already recorded today, so closing the one remaining gap is an incremental improvement rather than the foundation of the feature.

**Independent Test**: For each in-scope category, perform one representative action (e.g., publish a prompt version, change an AI provider credential, run a tool) and verify a correctly attributed audit entry appears in the audit view.

**Acceptance Scenarios**:

1. **Given** any in-scope sensitive action is performed by a user, **When** the action succeeds, **Then** an audit entry exists recording the actor, the action performed, the affected target, the owning tenant (when applicable), relevant metadata, and the time it occurred.
2. **Given** a sign-in attempt fails, **When** the attempt completes, **Then** the failed attempt is recorded as an audit entry.
3. **Given** an action is performed by the system itself rather than a person (e.g., an automated tool execution), **When** the audit entry is created, **Then** the actor is identified as the system/automation rather than a human user.

---

### Edge Cases

- An audit entry references a user or resource that has since been deleted or deactivated — the entry must still display, with the missing party labeled (e.g., "deleted user") rather than erroring.
- An entry's metadata is very large or deeply nested — the detail drawer must remain readable and the table must not degrade.
- Events that belong to no tenant (platform-level events) — they must appear in the platform view and never in any tenant's view.
- A filter combination matches nothing — the table shows a clear empty state, not an error.
- Very high entry volumes accumulated over time — listing stays responsive via pagination; users are never handed an unbounded list.
- A user attempts to modify or delete an audit entry through any exposed interface — no such capability exists for normal users, and direct attempts are rejected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST record an audit entry for every in-scope sensitive action, capturing: the actor (who), the action (what), the target (which resource, by type and identifier), the owning tenant (when the action is tenant-scoped), contextual metadata, and the timestamp (when).
- **FR-002**: The in-scope action categories MUST include: authentication events (sign-in success/failure, sign-out), tenant changes, user role/membership changes, prompt changes, AI provider and AI configuration changes, tool executions, and billing changes. Audit views MUST show all recorded audit entries (including operational categories already recorded, such as conversation, customer, and escalation events), not only these guaranteed categories; category filters let users narrow the list.
- **FR-003**: Audit entries MUST be immutable once written: no user-facing capability may edit or delete an entry, and immutability MUST be enforced at the data layer, not only in the interface.
- **FR-004**: Tenant Owners and Admins MUST be able to view a paginated, newest-first list of their own tenant's audit entries; tenant isolation MUST be enforced at the data-access layer so no cross-tenant entry can ever be returned.
- **FR-005**: All platform roles (Super Admin, Developer, Sales, Support, Finance) MUST be able to view a paginated, newest-first, read-only list of audit entries across all tenants, including platform-level entries associated with no tenant, and MUST be able to filter by tenant.
- **FR-006**: Users MUST be able to filter audit lists by date range, action category, and actor; applied filters MUST combine (AND semantics).
- **FR-007**: Users MUST be able to open any audit entry to see its full detail, including all recorded metadata, presented readably.
- **FR-008**: Access to audit data MUST be denied server-side for unauthorized roles (tenant Manager, Agent, Viewer for tenant logs; all tenant roles for platform logs); frontend checks alone are insufficient.
- **FR-009**: Actions performed by the system or automation (rather than a signed-in person) MUST be attributed to a clearly identified system actor.
- **FR-010**: Failed sensitive attempts that are security-relevant (at minimum, failed sign-ins) MUST be recorded, not only successful actions.
- **FR-011**: Audit entries whose actor or target has since been deleted MUST remain viewable, with the missing party clearly labeled.
- **FR-012**: Recording an audit entry MUST NOT corrupt or silently drop the record of the underlying action; if an entry cannot be written, the failure MUST be observable to operators.
- **FR-013**: Actions performed by platform staff inside a tenant MUST appear in that tenant's audit view, with the staff actor identified by name and visibly marked as platform staff.

### Key Entities

- **Audit Entry**: One immutable record of a sensitive action. Attributes: actor (user or system), action (namespaced name, e.g., category + verb), target (resource type + identifier), tenant (optional — absent for platform-level events), metadata (structured context such as changed fields or outcome), timestamp.
- **Actor**: The party that performed the action — a platform user, a tenant user, or the system itself.
- **Target**: The resource affected by the action (e.g., a user membership, a prompt, an AI credential, a tool, a billing setting).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of in-scope sensitive action categories produce audit entries with all six recorded facts (actor, action, target, tenant where applicable, metadata, timestamp), verified by performing one representative action per category.
- **SC-002**: A tenant admin can locate a specific known action (e.g., "who changed this member's role last week") in under 1 minute using filters alone.
- **SC-003**: Zero audit entries can be edited or deleted through any user-facing capability, by any tenant role, verified by attempting modification as each role.
- **SC-004**: Zero cross-tenant exposure: in testing across at least two tenants, no tenant user ever receives another tenant's audit entry.
- **SC-005**: The audit list page loads and responds to filter changes in under 2 seconds at realistic volumes (tens of thousands of entries per tenant).

## Assumptions

- The platform already records many in-scope actions (authentication, tenant changes, role changes, prompt changes, AI configuration/credential changes, tool *configuration* changes, conversation/customer/escalation/knowledge events) into an existing append-only audit store whose immutability is enforced at the data layer. This feature reuses that store and recording pattern; it does not redesign it.
- The main net-new work is: exposing read access (tenant-scoped and platform-scoped), building the viewing UI (table, filters, detail drawer), and closing the single verified recording gap (tool executions). Prompt changes and AI provider/configuration changes are already recorded today and need no new writers. Billing has no auditable actions yet, so its category is reserved rather than instrumented — nothing is built for it in this feature.
- Tenant-side audit viewing is restricted to Owner and Admin roles. Manager, Agent, and Viewer have no audit access. All platform staff roles receive read access to the platform audit view (read-only for everyone — no role can edit entries).
- "Cannot be edited by normal users" is interpreted strictly: no user role — tenant or platform — gets edit or delete capability through the product; entries are append-only for everyone.
- Audit entries are retained indefinitely in v1; no retention/purge policy or archival is in scope.
- Export (e.g., CSV download), real-time live-updating of the audit table, and alerting on audit events are out of scope for v1.
- Existing per-module audit action names remain valid; the viewing experience presents them grouped into the scope's human-readable categories.
