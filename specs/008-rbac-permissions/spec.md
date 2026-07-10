# Feature Specification: RBAC & Permissions

**Feature Branch**: `008-rbac-permissions`

**Created**: 2026-07-10

**Status**: Draft

**Input**: User description: "Implement role-based access control. Platform roles: Super Admin, Developer, Support Engineer, Sales, Finance. Tenant roles: Owner, Admin, Manager, Support Agent, Viewer. Backend: define permissions, map roles to permissions, add permission-checking middleware/helpers, protect platform-only and tenant-only endpoints. Frontend: hide navigation items based on permissions, protect routes based on permissions, add permission utilities. Acceptance: users only see allowed pages, users cannot access unauthorized APIs, tests cover platform and tenant roles."

## Clarifications

### Session 2026-07-10

- Q: What can a tenant Owner do that an Admin cannot? → A: Admin has full workspace control (settings, members, all features); Owner exclusively holds billing, tenant deletion, and assigning/transferring the Owner role.
- Q: What is the tenant Manager role's permission scope? → A: All functional areas including integrations and member management; only workspace settings and billing are off-limits.
- Q: When a user's role is changed or revoked mid-session, how quickly must it take effect? → A: Immediately everywhere — both API enforcement and open UI sessions reflect the change without waiting for a session refresh.
- Q: When platform staff switch into a tenant, what can each role do there? → A: Environment-dependent. In non-production environments (dev, qa, stg) every platform role gets full tenant access. In production, least privilege applies: Super Admin full; Support Engineer works support areas (conversations, customers, knowledge base) without settings; Developer read-only/diagnostic; Sales & Finance read-only account-level info.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Unauthorized API access is rejected (Priority: P1)

Any signed-in user who calls an operation their role does not permit is refused, regardless of how the request is made (the app, a direct API call, or a crafted client). A tenant Viewer cannot change settings, a Support Agent cannot manage integrations, a tenant user cannot reach platform administration operations, and a platform Sales user cannot perform actions reserved for the Super Admin. The refusal is a clear "not allowed" response that names no restricted data.

**Why this priority**: Server-side enforcement is the only real security boundary — the constitution forbids relying on frontend checks alone. Every other story in this feature is presentation on top of this guarantee. If only this story ships, the platform is already protected.

**Independent Test**: Can be fully tested by issuing API requests as users holding each of the ten roles against a matrix of protected operations and asserting allow/deny matches the role–permission mapping, with no frontend involved.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant Viewer, **When** they attempt an operation that modifies tenant data (e.g., updating workspace settings), **Then** the request is rejected as forbidden and no data is changed.
2. **Given** a signed-in tenant user with no platform role, **When** they call any platform-only operation (e.g., listing all tenants), **Then** the request is rejected as forbidden.
3. **Given** a signed-in platform Finance user, **When** they call a platform operation their role permits (e.g., viewing billing-related data), **Then** the request succeeds.
4. **Given** a signed-in user whose request lacks any recognized role for the requested scope, **When** they call a protected operation, **Then** the request is denied — access is denied by default, never granted by omission.
5. **Given** an unauthenticated caller, **When** they call any protected operation, **Then** the response indicates authentication is required, distinguishable from a permission refusal.

---

### User Story 2 - Users only see the pages and navigation their role allows (Priority: P2)

When a user signs in to the dashboard, the navigation shows only the areas their role permits. A Support Agent sees their working areas (conversations, customers, knowledge base) but not workspace settings; a Viewer sees read-only areas; an Owner sees everything in their tenant. If a user types or follows a link to a page their role does not allow, they are redirected to a safe page instead of seeing the restricted content — even briefly.

**Why this priority**: This is the day-to-day user experience of RBAC — a clean interface that never offers actions the user cannot take, preventing confusion and dead-end error screens. It depends on the permission model from Story 1 but delivers its own testable value.

**Independent Test**: Can be tested by signing in as each tenant role and verifying the set of visible navigation items matches the role matrix, then deep-linking to a disallowed page and confirming redirection with no restricted content shown.

**Acceptance Scenarios**:

1. **Given** a signed-in Support Agent, **When** the dashboard loads, **Then** navigation shows only the pages their role permits and no entry for workspace settings.
2. **Given** a signed-in Viewer, **When** they navigate directly to the settings page by URL, **Then** they are redirected to a page they are allowed to view and no settings content is displayed.
3. **Given** a signed-in Owner, **When** the dashboard loads, **Then** all tenant pages are available in navigation.
4. **Given** a user whose role permits a page, **When** they navigate to it, **Then** the page loads normally with no additional friction.
5. **Given** a signed-in user, **When** a page contains actions their role does not permit (e.g., an edit button for a Viewer), **Then** those actions are not offered.

---

### User Story 3 - Platform staff get role-appropriate access inside a tenant (Priority: P3)

Platform staff who switch into a tenant context (via the existing tenant switcher) receive capabilities appropriate to their platform role and the environment. In non-production environments (dev, qa, stg) all platform roles have full tenant access so internal work is frictionless. In production, least privilege applies: a Super Admin can do anything within the tenant; a Support Engineer can view and work support-related areas; Sales and Finance see the account-level information relevant to their jobs; a Developer can view diagnostic information. In production, none of them silently gain tenant-owner powers they don't need.

**Why this priority**: Platform-to-tenant access already exists (feature 006); this story bounds it with least-privilege rules. It matters for enterprise trust and auditability but the platform is functional without it being refined, as long as Stories 1–2 hold.

**Independent Test**: Can be tested by switching into a tenant as each platform role, in a production-configured and a non-production-configured environment, and verifying which tenant pages are visible and which tenant operations succeed or are refused per the role matrix.

**Acceptance Scenarios**:

1. **Given** a platform Super Admin switched into a tenant, **When** they perform any tenant operation, **Then** it succeeds in every environment.
2. **Given** a production environment and a platform Support Engineer switched into a tenant, **When** they view conversations, **Then** access is granted; **When** they attempt to change workspace settings, **Then** access is refused.
3. **Given** a production environment and a platform Sales user switched into a tenant, **When** they attempt to modify tenant data, **Then** the request is refused.
4. **Given** a non-production environment (dev, qa, or stg) and any platform role switched into a tenant, **When** they perform any tenant operation, **Then** it succeeds.

---

### Edge Cases

- A signed-in user with neither a platform role nor any tenant membership: every protected operation is refused and the dashboard shows a safe landing state, not an error loop.
- A user's role is changed or revoked while they are signed in: the very next API request is evaluated against the new role, and their open dashboard session updates its visible navigation/pages accordingly — a revoked user must not retain access for the remainder of their session.
- A stored role value that is unrecognized (legacy or corrupted data): treated as no role — access denied by default, and the anomaly is observable to operators.
- A user deep-links into a restricted page while the app is still loading their permission set: the restricted content never renders, even transiently, before the check completes.
- A platform user who has *not* switched into a tenant attempts a tenant-scoped operation: refused, consistent with the existing tenant-context contract.
- A user belongs to multiple tenants with different roles: permissions are always evaluated against the role in the currently selected tenant only.
- A new endpoint is added without an explicit permission declaration: it must not be silently reachable — the default posture is deny.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST define a single canonical catalog of named permissions covering all protected capabilities (viewing and managing each functional area of the tenant dashboard, and each platform administration capability).
- **FR-002**: System MUST define a fixed mapping from each of the five platform roles (Super Admin, Developer, Support Engineer, Sales, Finance) and five tenant roles (Owner, Admin, Manager, Support Agent, Viewer) to a set of permissions. The mapping is maintained centrally in one place; the same mapping drives both API enforcement and UI visibility.
- **FR-002a**: Within a tenant, Admin holds full workspace control (settings, member management, all functional areas). Owner holds everything Admin holds plus three exclusive capabilities: billing, tenant deletion, and assigning or transferring the Owner role.
- **FR-002b**: Within a tenant, Manager holds all functional areas — conversations, customers, AI agent, knowledge base, integrations, analytics — including member management, but MUST NOT access workspace settings or billing.
- **FR-003**: Every protected operation MUST declare the permission it requires, and the system MUST refuse requests from users lacking that permission. Operations without a declared requirement MUST be refused by default (fail closed).
- **FR-004**: Platform-only operations MUST be refused for any user without the required platform role, including tenant users of every role.
- **FR-005**: Tenant-scoped operations MUST be evaluated against the user's role in the currently active tenant. Platform staff acting inside a tenant receive the tenant capabilities defined for their platform role in the role–permission mapping.
- **FR-005a**: Platform-staff capabilities inside a tenant MUST be environment-aware. In non-production environments (dev, qa, stg), every platform role receives full tenant access. In production, least privilege applies: Super Admin full access; Support Engineer may work support areas (conversations, customers, knowledge base) but not change settings; Developer read-only/diagnostic access; Sales and Finance read-only account-level access. Tenant-user permissions are identical in every environment.
- **FR-006**: Permission refusals MUST return the platform's standard error shape, distinguishable from "not signed in", and MUST NOT reveal the existence or content of restricted data.
- **FR-007**: The dashboard MUST show each user only the navigation items for pages their effective permissions allow.
- **FR-008**: The dashboard MUST prevent navigation to pages the user's permissions do not allow, redirecting to an allowed page, and MUST NOT render restricted content even momentarily while permissions are being resolved.
- **FR-009**: The dashboard MUST provide a reusable way for any screen to ask "does the current user hold permission X?" so per-page actions (buttons, forms) can be shown or hidden consistently, without duplicating role logic per feature.
- **FR-010**: The user's effective permission set MUST originate from the server-maintained mapping (the single source of truth); the frontend consumes it and MUST NOT maintain an independent role-to-permission mapping that could drift.
- **FR-011**: Changes to a user's role MUST take effect immediately: server-side enforcement MUST evaluate the user's current role on every request (never a stale cached role), and open UI sessions MUST reflect the new permission set without requiring the user to sign out or wait for a session refresh.
- **FR-012**: Automated tests MUST cover the allow/deny outcome for every role (all five platform roles and all five tenant roles) against representative protected operations in both scopes, and the visibility of navigation/routes per tenant role.

### Key Entities

- **Permission**: A named, human-readable capability (e.g., "view conversations", "manage workspace settings", "administer tenants"). The atomic unit of access control.
- **Role**: A named bundle of permissions. Two disjoint families: platform roles (Super Admin, Developer, Support Engineer, Sales, Finance) held on the user account, and tenant roles (Owner, Admin, Manager, Support Agent, Viewer) held per tenant membership.
- **Role–Permission Mapping**: The central matrix assigning each role its permission set. Fixed per release (not editable by end users in this feature).
- **Effective Permission Set**: The permissions a specific user holds right now, derived from their platform role and/or their role in the currently active tenant; what both API enforcement and the dashboard consult.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of protected operations refuse callers whose role lacks the required permission, verified by an automated matrix covering all ten roles — zero unauthorized successes.
- **SC-002**: For every tenant role, the set of visible navigation items exactly matches the role matrix — zero navigation entries lead to a page the user cannot use.
- **SC-003**: A user who deep-links to a disallowed page is redirected to an allowed page without any restricted content being displayed, 100% of the time.
- **SC-004**: Permission checks add no user-perceivable delay: page loads and API interactions feel the same as before the feature.
- **SC-005**: Every role in both families is exercised by automated tests for both an allowed and a denied operation, so a regression in any role's access is caught before release.

## Assumptions

- **Role name alignment**: "Support Engineer" is the display name for the existing platform `support` role, and "Support Agent" is the display name for the existing tenant `agent` role; no new role values are introduced and no data migration of role values is needed.
- **Static role catalog**: The ten roles and their permission sets are fixed by the product. Custom roles, per-tenant permission overrides, and a role-administration UI are out of scope for this feature.
- **Role assignment is out of scope**: How users acquire roles (invitations, membership management) is existing/future functionality; this feature only *evaluates* roles already assigned.
- **Scope of protected surface**: The permission catalog covers the current dashboard areas (overview, conversations, customers, AI agent, knowledge base, integrations, analytics, settings) and current platform operations (tenant directory, tenant switching, platform administration). New areas added later declare their permissions as they are built.
- **Environment awareness**: The system can distinguish production from non-production (dev, qa, stg) via deployment configuration; the platform-staff-in-tenant rules in FR-005a key off that setting. No per-request environment detection is implied.
- **Session model**: The existing authentication and tenant-context mechanisms (feature 006/007) are reused. Role changes take effect immediately (see FR-011): the server evaluates the current role per request, and open UI sessions update without re-login.
- **Audit posture**: Sensitive-operation auditing already exists per the constitution; this feature does not add new audit requirements beyond keeping existing switch/role audits intact.
