# Feature Specification: Platform Tenant Management

**Feature Branch**: `010-platform-tenant-management`

**Created**: 2026-07-11

**Status**: Draft

**Input**: User description: "Platform Tenant Management — Allow platform users to manage customer organizations. Scope: List tenants, Create tenant, View tenant, Edit tenant, Activate/deactivate tenant, Tenant status, Tenant metadata. Backend: GET /platform/tenants, POST /platform/tenants, GET /platform/tenants/:id, PATCH /platform/tenants/:id. Frontend: Tenant list page, Tenant detail page, Create tenant form, Edit tenant form, Status badge. Acceptance: Platform users can manage tenants. Tenant users cannot access platform tenant management. Tenant list supports search, pagination, and filtering. Audit logs are created for sensitive tenant changes."

## Clarifications

### Session 2026-07-11

- Q: Which platform roles can manage tenants (create, edit, activate/deactivate)? → A: Super Admin and Support Engineer hold the management capabilities; Developer, Sales, and Finance are view-only. All platform roles keep directory viewing.
- Q: What does "tenant metadata" cover beyond name, slug, and status? → A: Structured business fields — a plan/tier and a primary contact (name + email). Plan comes from a fixed starter set (Trial, Starter, Professional, Enterprise; default Trial); contact fields are optional, email format-validated. Editable by the management roles; changes audited like any edit.
- Q: Can a tenant's slug be changed after creation? → A: Yes — editable by management roles with the same validation as creation (live-tenant uniqueness, format); changes audited. Internal references use the tenant's immutable ID, so memberships, sessions, and audit history are unaffected.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Onboard a new customer organization (Priority: P1)

A platform administrator creates a new customer organization from the platform area: they provide the organization's display name and a URL-friendly identifier (slug), the system validates both (required name, correctly formatted and unused slug), and on success the tenant exists in Active status — immediately visible in the tenant directory, immediately available for platform staff to switch into, and ready for future member invitations. The creation is recorded in the audit trail with who did it and when.

**Why this priority**: Creating tenants is the core new capability — today customer organizations can only be seeded directly in the database. Without onboarding, nothing else in this feature has anything to manage. This story alone lets the business bring a new customer onto the platform.

**Independent Test**: As a platform administrator, submit the create form (and the equivalent direct request) with valid and invalid inputs; verify validation messages, that a valid submission produces an Active tenant findable in the directory and switchable-into, and that an audit record exists for the creation.

**Acceptance Scenarios**:

1. **Given** a signed-in platform administrator on the tenant list page, **When** they open the create form, enter a valid name and unused slug, and submit, **Then** the tenant is created in Active status and appears in the directory.
2. **Given** the create form, **When** the administrator submits a slug that is already in use by a live tenant or is incorrectly formatted, **Then** a clear validation message identifies the problem and nothing is created.
3. **Given** a newly created tenant, **When** a platform user switches into it, **Then** the switch succeeds and the tenant context works like any existing tenant.
4. **Given** a completed creation, **When** an operator inspects the audit trail, **Then** a record shows who created which tenant and when.
5. **Given** a signed-in tenant user (any role), **When** they attempt the create operation directly, **Then** the request is refused and no tenant is created.

---

### User Story 2 - Find and inspect customer organizations (Priority: P2)

A platform user works with the tenant directory as a real management surface: a list page showing each tenant's name, slug, and status (with a clear visual status badge), searchable by name or slug, filterable by status, and paginated for large customer bases. Selecting a tenant opens a detail page showing its full record — name, slug, current status, and record dates — with the management actions the viewer's role permits.

**Why this priority**: The directory is the daily working surface for support, sales, and finance staff ("which customer is this?", "is their workspace active?"). A directory endpoint already exists for the tenant switcher; this story turns it into a full page with filtering and a detail view.

**Independent Test**: Seed a mixed set of Active and Suspended tenants; verify search terms match name and slug, the status filter returns exactly the matching subset, pagination traverses the full set without duplicates or gaps, and the detail page shows the correct record for each tenant.

**Acceptance Scenarios**:

1. **Given** a signed-in platform user on the tenant list page, **When** the page loads, **Then** tenants are listed with name, slug, and a status badge, and results beyond one page are reachable via pagination.
2. **Given** the list page, **When** the user types a search term, **Then** only tenants whose name or slug matches are shown.
3. **Given** the list page, **When** the user filters by a status, **Then** only tenants with that status are shown, and search and filter combine correctly.
4. **Given** the list page, **When** the user selects a tenant, **Then** a detail page shows that tenant's name, slug, status, plan/tier, primary contact, and record dates.
5. **Given** a search or filter with no matches, **When** results render, **Then** a clear empty state explains there are no matching tenants (and offers creation where the viewer is permitted).
6. **Given** a signed-in tenant user, **When** they attempt to open the tenant list or a tenant detail page (by link or direct request), **Then** they are refused/redirected and no tenant data is exposed.

---

### User Story 3 - Maintain and control a customer organization (Priority: P3)

A platform administrator keeps tenant records correct and controls workspace availability. From the detail page they can edit the tenant's name, slug, plan/tier, and primary contact (with the same validation as creation), and activate or deactivate the workspace. Deactivating (suspending) a tenant immediately blocks that tenant's users — their very next interaction is refused — while platform staff can still find, inspect, and reactivate the tenant. Every edit and every status change is audited.

**Why this priority**: Corrections and offboarding/suspension matter for operations (billing disputes, contract endings, abuse response) but the platform delivers value with only creation and inspection in place.

**Independent Test**: Edit a tenant's name/slug and verify persistence and validation; deactivate a tenant and verify its users are refused on their next request while platform staff can still view and reactivate it; verify audit records for each change.

**Acceptance Scenarios**:

1. **Given** a platform administrator on a tenant's detail page, **When** they edit the name, slug, plan/tier, or primary contact with valid values and save, **Then** the record updates and the change appears everywhere the tenant is displayed.
2. **Given** the edit form, **When** a changed slug collides with another live tenant's slug, is malformed, or the contact email is not a valid address, **Then** a clear validation message is shown and nothing is saved.
3. **Given** an Active tenant with signed-in members, **When** the administrator deactivates it, **Then** each member's next interaction with the workspace is refused, without waiting for them to sign out.
4. **Given** a Suspended tenant, **When** a platform administrator reactivates it, **Then** tenant users regain access on their next interaction.
5. **Given** a Suspended tenant, **When** platform staff browse the directory or open its detail page, **Then** the tenant remains visible and inspectable with its Suspended badge.
6. **Given** any edit or status change, **When** an operator inspects the audit trail, **Then** a record shows who changed what, on which tenant, and when.
7. **Given** a signed-in tenant user (including an Owner), **When** they attempt an edit or status-change operation directly, **Then** the request is refused and nothing changes.

---

### Edge Cases

- Creating a tenant with the slug of a previously deleted (soft-deleted) tenant: allowed — slug uniqueness applies among live tenants only; the new tenant is a distinct organization.
- Two administrators edit the same tenant at once: the last save wins; both changes are audited so the sequence is reconstructable.
- Deactivating a tenant while a platform staff member is currently switched into it: the staff member's platform capabilities continue to work; tenant members are the ones refused.
- A tenant user deep-links to a platform tenant-management page while the app is loading: the page never renders (existing fail-closed route protection) and the equivalent direct request is refused.
- Search combined with a status filter and pagination: paging through filtered results stays consistent (no duplicates, no skipped tenants).
- A platform user whose role permits viewing but not managing opens a tenant detail page: management actions (edit, activate/deactivate, create) are not offered, and direct attempts are refused.
- Renaming or re-slugging a tenant that platform staff currently have selected as their active context: the displayed tenant name/slug refreshes on their next interaction; their access is unaffected.
- Rapid repeated status toggling: each transition is applied and audited in order; the final state matches the last action.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Platform users MUST be able to list customer organizations with each entry showing at least name, slug, and status; the list MUST support text search over name and slug, filtering by status, and pagination — combinable and consistent with the platform's existing list conventions.
- **FR-002**: Authorized platform users MUST be able to create a tenant by supplying a display name and a slug, plus optional business metadata: a plan/tier (from the fixed set Trial / Starter / Professional / Enterprise, defaulting to Trial) and a primary contact (name and email, both optional). The system MUST validate: name present and within length limits; slug in the established URL-safe format; slug unique among live (non-deleted) tenants; contact email format-valid when provided. New tenants start in Active status.
- **FR-003**: Platform users MUST be able to view a single tenant's full record: name, slug, status, plan/tier, primary contact, and record dates (created/last updated).
- **FR-004**: Authorized platform users MUST be able to edit a tenant's name, slug, plan/tier, and primary contact, subject to the same validation rules as creation. Validation failures MUST leave the record unchanged and explain the problem.
- **FR-005**: Authorized platform users MUST be able to change a tenant's status between Active and Suspended (activate/deactivate). A Suspended tenant's members MUST be refused on their next interaction with the workspace — no sign-out or wait required; reactivation restores access equally immediately.
- **FR-006**: Suspended tenants MUST remain visible and manageable to platform staff (directory, detail, reactivation); suspension affects tenant members only.
- **FR-007**: All tenant-management operations MUST be inaccessible to tenant users of every role: server-side requests refused with the platform's standard permission-denial behavior, and platform tenant-management pages/navigation never shown or reachable in the dashboard (consistent with the platform's fail-closed access model).
- **FR-008**: Within platform staff, management capabilities (create, edit, activate/deactivate) MUST be restricted to Super Admin and Support Engineer via a tenant-management permission in the platform's central role–permission model; Developer, Sales, and Finance (and all platform roles) retain directory viewing only.
- **FR-009**: Every tenant creation, edit, and status change MUST produce an audit record capturing who performed it, which tenant was affected, what kind of change it was, and when — appended to the platform's existing audit trail.
- **FR-010**: Status MUST be displayed with a consistent visual badge (Active / Suspended) everywhere tenants are listed or inspected, reusing the dashboard's established status presentation.
- **FR-011**: List, create, and edit interactions MUST handle failure modes gracefully: empty search/filter results show an explanatory empty state; validation errors are shown next to the offending input; permission refusals never reveal tenant data.

### Key Entities

- **Tenant (customer organization)**: The managed record — display name (1–200 characters), slug (URL-safe, lowercase, unique among live tenants, case-insensitive), status (Active or Suspended), plan/tier (Trial / Starter / Professional / Enterprise), primary contact (optional name and format-validated email), record dates. Already the platform's core tenancy entity; this feature adds business metadata and lifecycle management on top.
- **Tenant Status**: Two-state lifecycle — Active (workspace usable by members) ↔ Suspended (members refused; platform staff retain visibility and control). Transitions are explicit administrator actions.
- **Audit Record**: Append-only entry for each sensitive tenant change (create, edit, status change): actor, affected tenant, action type, timestamp. Extends the existing audit trail.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A platform administrator can onboard a new customer organization (open form → tenant active in the directory) in under one minute, with no database or engineering involvement.
- **SC-002**: Directory search, status filtering, and pagination return exactly the matching tenants — zero missing or duplicated entries when paging through a directory of at least 500 tenants — with no user-perceivable slowdown versus other dashboard pages.
- **SC-003**: 100% of tenant-management operations attempted by tenant users (any role) are refused, and 0% of tenant-management UI is visible to them — verified across all ten platform/tenant roles.
- **SC-004**: 100% of creations, edits, and status changes produce a correct audit record (actor, tenant, action, time) — zero unaudited sensitive changes.
- **SC-005**: After deactivation, 100% of the suspended tenant's member interactions are refused starting with the very next request; after reactivation, access resumes on the next request.
- **SC-006**: Platform users with view-only roles can complete every inspection task (find, filter, open detail) while being offered zero management actions.

## Assumptions

- **"Deactivate" maps to the existing Suspended status**: the platform already has a two-value tenant status (Active/Suspended) and already refuses suspended tenants' members at the workspace boundary; this feature makes the transition an administrator action rather than a database edit. No third status value is introduced.
- **Tenant metadata scope** (clarified): the managed metadata is the descriptive record (name, slug, status, record dates) plus structured business fields — plan/tier and primary contact. The plan set (Trial, Starter, Professional, Enterprise) is a starter vocabulary owned by this feature until a billing feature takes it over; it is display/reporting metadata only and grants or restricts nothing. Internal notes and further business fields remain future scope.
- **Management vs. viewing split** (clarified): creating, editing, and activating/deactivating tenants is held by Super Admin and Support Engineer; Developer, Sales, and Finance are view-only. All platform roles retain directory viewing, which they already hold. New permissions for these capabilities follow the existing central-catalog pattern.
- **Deletion is out of scope**: no tenant deletion (soft or hard) is exposed in this feature; suspension is the offboarding control.
- **Existing directory contract is extended, not replaced**: the current platform tenant listing (search + pagination) gains a status filter and a management page; the tenant switcher keeps working unchanged.
- **Member management is out of scope**: inviting/removing tenant members and role assignment remain existing/future functionality; the detail page manages the organization record only.
- **Slug changes are permitted** (clarified) with live-tenant uniqueness enforced; internal references use the tenant's immutable identifier, so renaming/re-slugging does not break existing memberships, sessions, or audit history.
