# Feature Specification: Customer Profiles

**Feature Branch**: `012-customer-profiles`

**Created**: 2026-07-13

**Status**: Draft

**Input**: User description: "Customers — Create the customer profile system. Scope: customer records, customer identifiers per channel, contact information, customer metadata, customer conversation history. Backend: list, create, view, update, search customers. Frontend: customer list page, customer profile page, customer metadata view, conversation history section. Acceptance: customers are tenant-scoped, customers can be searched, customer profiles show basic conversation history, tests verify tenant isolation."

## Clarifications

### Session 2026-07-13

- Q: How should the customer profile's conversation history section be sourced, given no conversations feature exists yet? → A: This feature defines a minimal, tenant-scoped conversation summary record (channel, status, last-activity time, linked customer); the profile reads it, tests seed it, and future messaging features extend it.
- Q: Which channels should customer identifiers support in this feature? → A: A fixed initial set: email, phone, web chat, WhatsApp, and Telegram; new channels are added later as explicit extensions.
- Q: Should customer create/update operations be recorded in the platform's audit log? → A: Yes — every customer creation and modification writes a who/what/when entry to the existing append-only audit log, including which fields changed.
- Q: Which tenant roles should be able to create and update customers? → A: Agent and above (Owner, Admin, Manager, Agent); Viewer is read-only.
- Q: What is the maximum number of metadata attributes per customer? → A: 50.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Find Customers in a Tenant Directory (Priority: P1)

A tenant team member (agent, manager, admin, or owner) opens the Customers area of the dashboard and sees a paginated directory of the customers belonging to their tenant. They narrow the directory by typing a search term (name, email, phone number, or channel identifier) and quickly locate the customer they need. Team members never see customers belonging to any other tenant.

**Why this priority**: The directory is the entry point for every other customer interaction and is where the core acceptance criteria (tenant scoping, searchability) are proven. Without it, no other customer-facing capability is reachable.

**Independent Test**: Seed two tenants with distinct customers, sign in as a member of each tenant, and confirm the list shows only that tenant's customers and that searching by name, email, phone, or channel identifier returns the matching customers only.

**Acceptance Scenarios**:

1. **Given** a tenant with existing customers, **When** a tenant member opens the customer list page, **Then** they see a paginated list of that tenant's customers with name, primary contact details, and channels at a glance.
2. **Given** two tenants each with their own customers, **When** a member of tenant A views or searches the customer list, **Then** no customer from tenant B ever appears in results, counts, or pagination.
3. **Given** a customer named "Sara Ali" with email "sara@example.com", **When** a tenant member searches for "sara", "sara@example.com", or her phone number, **Then** the customer appears in the results.
4. **Given** a search term that matches no customers, **When** the search runs, **Then** the list shows a clear empty state with the option to clear the search or create a customer.

---

### User Story 2 - View a Customer Profile (Priority: P2)

A tenant team member opens a customer from the directory and lands on the customer's profile page. The profile shows the customer's contact information, the identifiers the customer uses on each communication channel, custom metadata attributes, and a basic history of the customer's conversations so the team member has full context before responding or taking action.

**Why this priority**: The profile is the "single view of the customer" that gives support teams context. It depends on the directory (P1) to be reachable, and it is where metadata and conversation history become visible.

**Independent Test**: Open a seeded customer's profile and verify contact information, per-channel identifiers, metadata attributes, and the conversation history section render correctly, including empty states when the customer has no metadata or no conversations.

**Acceptance Scenarios**:

1. **Given** a customer with contact details, channel identifiers, and metadata, **When** a tenant member opens the customer's profile, **Then** all of these are displayed in clearly separated sections along with when the record was created and last updated.
2. **Given** a customer who has past conversations, **When** their profile is viewed, **Then** a conversation history section lists those conversations with at least channel, status, and last-activity time, ordered most recent first.
3. **Given** a customer with no conversations or no metadata, **When** their profile is viewed, **Then** the corresponding sections show informative empty states rather than errors or blank areas.
4. **Given** a customer ID that belongs to another tenant, **When** a tenant member attempts to open that profile directly (e.g., via URL), **Then** the system responds as if the customer does not exist.

---

### User Story 3 - Create and Update Customer Records (Priority: P3)

A tenant team member with sufficient permissions creates a customer record manually — entering the customer's name, contact information, channel identifiers, and optional metadata — and later edits the record to correct or enrich it. Team members without edit permissions can view but not modify customers.

**Why this priority**: Manual creation and editing complete the management lifecycle. It is ranked after browse/view because in the long run most customer records will be created automatically by inbound channel activity; manual management is the fallback and correction path.

**Independent Test**: As a permitted tenant member, create a customer with contact info, an email channel identifier, and one metadata attribute; verify it appears in the list and profile; edit the record and verify changes persist. As a view-only member, verify create/edit actions are unavailable and rejected.

**Acceptance Scenarios**:

1. **Given** a permitted tenant member on the customer list page, **When** they create a customer with a name and at least one contact detail or channel identifier, **Then** the customer is saved to their tenant and appears in the list and search results immediately.
2. **Given** an existing customer, **When** a permitted member updates contact information, channel identifiers, or metadata, **Then** the profile reflects the changes and the last-updated time is refreshed.
3. **Given** a create or update attempt with an invalid email or phone format, **When** it is submitted, **Then** the system rejects it with a field-level, human-readable message and no partial data is saved.
4. **Given** a channel identifier (e.g., an email address on the email channel) already assigned to another customer in the same tenant, **When** a member tries to assign it to a second customer, **Then** the system rejects the change and explains the conflict, identifying the existing customer where the viewer is permitted to see it.
5. **Given** a tenant member whose role does not permit customer editing, **When** they view the customer area, **Then** create and edit controls are not offered, and any direct modification attempt is refused by the system.

---

### Edge Cases

- What happens when two tenants each have a customer with the same email or phone number? Both must coexist independently; identifier uniqueness applies only within a tenant.
- What happens when a search term contains special characters or is very long? The search must return safely (matching literally or returning no results) without errors.
- How does the system handle a profile request for a deleted or never-existing customer ID? It responds as "not found" — indistinguishable from a cross-tenant access attempt.
- What happens when a customer has a very large number of conversations? The history section shows the most recent ones and indicates more exist, without degrading profile load time.
- What happens when two team members edit the same customer at nearly the same time? The last successful save wins, and no update may corrupt the record or mix fields from both edits.
- What happens when a customer record has many metadata attributes? The metadata view remains readable up to the 50-attribute limit, and an attempt to add a 51st attribute is rejected with a clear message.
- What happens when a platform user (e.g., support) is operating in a tenant's context? They see and manage that tenant's customers exactly as scoped tenant data — never an aggregated cross-tenant customer view.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST store customer records that are owned by exactly one tenant; every read and write of customer data MUST be scoped to the acting user's current tenant context at the data-access layer, not only in the user interface.
- **FR-002**: A customer record MUST support a display name, contact information (at least email and phone number), and record timestamps (created, last updated).
- **FR-003**: A customer record MUST support zero or more channel identifiers, each combining a communication channel with the customer's identifier on that channel; the supported channel set for this feature is fixed: email, phone, web chat, WhatsApp, and Telegram (new channels are added later as explicit extensions). A given channel-plus-identifier pair MUST be unique within a tenant.
- **FR-004**: A customer record MUST support custom metadata as named attributes with values (e.g., "plan: enterprise", "region: EMEA") that tenant teams can add, edit, and remove, up to a limit of 50 attributes per customer.
- **FR-005**: Tenant members MUST be able to view a paginated list of their tenant's customers showing at minimum name, primary contact details, and channels.
- **FR-006**: Tenant members MUST be able to search customers by name, email, phone number, or channel identifier, with matching that tolerates partial input (e.g., a name fragment); search results MUST respect tenant scope and pagination.
- **FR-007**: Permitted tenant members MUST be able to create a customer record by providing at least a name plus one contact detail or channel identifier.
- **FR-008**: Permitted tenant members MUST be able to update a customer's contact information, channel identifiers, and metadata; the system MUST record when the record was last changed.
- **FR-009**: Tenant members MUST be able to view a single customer's full profile: contact information, channel identifiers, metadata, and conversation history.
- **FR-010**: The customer profile MUST include a conversation history section listing the customer's conversations with at least channel, status, and last-activity time, most recent first, limited to a recent subset with an indication when more exist; when the customer has no conversations the section MUST show an empty state.
- **FR-011**: Any attempt to access or modify a customer belonging to a different tenant MUST be refused with a response indistinguishable from the customer not existing.
- **FR-012**: Customer viewing MUST be available to all tenant roles; customer creation and modification MUST be restricted to a customer-management permission held by Agent-level roles and above (Owner, Admin, Manager, Agent) — Viewer is read-only — enforced by the system (not only hidden in the interface).
- **FR-013**: Create and update operations MUST validate contact information formats (email, phone) and reject invalid input with field-level, human-readable messages without saving partial data.
- **FR-014**: The system MUST reject assigning a channel identifier already held by another customer in the same tenant, with a message that explains the conflict.
- **FR-015**: Automated tests MUST verify tenant isolation for every customer operation (list, search, view, create, update), demonstrating that one tenant's data is never readable or writable from another tenant's context.
- **FR-016**: The system MUST maintain a minimal, tenant-scoped conversation summary record — channel, status, last-activity time, and owning customer — as the data source for the profile's conversation history section; automated tests MUST be able to seed these records, and future messaging features are expected to extend this record rather than replace the profile integration.
- **FR-017**: Every customer creation and modification MUST produce an append-only audit record capturing who performed the action, what changed (including which fields), and when it occurred, using the platform's existing audit trail.

### Key Entities

- **Customer**: A person the tenant's business communicates with. Belongs to exactly one tenant. Has a display name, contact information, timestamps, and relationships to channel identifiers, metadata attributes, and conversations.
- **Channel Identifier**: The customer's identity on one communication channel (channel type + identifier value, e.g., email + address). Supported channels in this feature: email, phone, web chat, WhatsApp, Telegram. Each identifier belongs to one customer; unique per channel within a tenant so inbound activity can be matched to the right customer.
- **Customer Metadata Attribute**: A named value attached to a customer (name + value pairs) used by tenant teams to enrich profiles with business-specific context.
- **Conversation Summary**: A minimal, tenant-scoped record of an exchange between the customer and the tenant on a channel — channel, status, last-activity time, owning customer. Introduced by this feature as the data source for the profile's history section and as the extension point for future messaging features. Conversation content and management remain outside this feature's scope.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant member can locate a specific customer via search in under 10 seconds from opening the customer list page.
- **SC-002**: Customer list and search results are displayed in under 1 second for tenants with up to 10,000 customers.
- **SC-003**: 100% of tenant-isolation tests pass: across every customer operation, zero customer records are ever exposed to or modifiable from another tenant's context.
- **SC-004**: A permitted tenant member can create a complete customer record (name, contact detail, channel identifier, one metadata attribute) in under 1 minute.
- **SC-005**: Opening any customer profile presents contact information, channel identifiers, metadata, and conversation history in a single view without further navigation.
- **SC-006**: 100% of invalid create/update submissions (bad email/phone format, duplicate channel identifier) are rejected with a message that identifies the offending field.

## Assumptions

- This feature owns a minimal conversation summary record (see FR-016) so the profile's history section is backed by real, tenant-isolated data and is fully testable via seeded records. No user-facing way to create conversations ships in this feature; when a customer has no summaries the section shows an empty state. Future messaging features extend this record with content and management capabilities.
- Customer deletion is out of scope for this feature; the lifecycle covered here is create, view, update, list, and search. (Records follow the platform's existing soft-delete conventions so deletion can be added later.)
- Merging duplicate customers and automatic customer creation from inbound channel messages are out of scope; they are expected follow-on features enabled by the per-channel identifier uniqueness rule.
- Role permissions follow the platform's existing tenant role model and permission system: all tenant roles can view customers; a customer-management permission held by Owner, Admin, Manager, and Agent gates create and update, with Viewer read-only.
- Metadata attributes are free-form names and values defined by each tenant; no tenant-wide attribute schema or type system is required in this feature. The 50-attribute per-customer limit protects readability and performance.
- Search is a straightforward directory search (partial matching on name, email, phone, channel identifier); relevance ranking, fuzzy matching, and cross-entity search are out of scope.
- Platform users operating in a tenant's context (via the tenant switcher) interact with customers under that tenant's scope, subject to the same permission checks; there is no cross-tenant customer view anywhere in the product.
