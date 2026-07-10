# Feature Specification: Dashboard Shell

**Feature Branch**: `009-dashboard-shell`

**Created**: 2026-07-11

**Status**: Draft

**Input**: User description: "Dashboard Shell — Create the main authenticated application layout. Scope: Sidebar, Header, Tenant switcher, User menu, Breadcrumbs, Responsive layout, Theme toggle, Page container, Loading states, Empty states. Frontend Requirements: create reusable components — App shell, Sidebar, Top navigation, Breadcrumb, Page header, User avatar menu, Tenant switcher, Theme switcher. Acceptance Criteria: Dashboard shell works after login. Platform users see platform navigation. Tenant users see tenant navigation. Tenant switcher only appears for platform users. Layout supports light and dark themes."

## Clarifications

### Session 2026-07-11

- Q: Where should the account menu (identity + sign-out) live in the shell? → A: A header avatar menu is the single identity surface — an avatar button at the far right of the header opens a dropdown with name, email, role, and sign-out; the sidebar footer user card (currently fixture data) is removed entirely.
- Q: How do platform and tenant navigation coexist for platform users? → A: The sidebar stays tenant-only; platform destinations are reached from a dedicated header control (adjacent to the tenant switcher), visible only to platform users with platform permissions.
- Q: Which role designation does the avatar menu show? → A: Context-aware — platform users see their platform role's display name; tenant users see their role in the currently active tenant (with the tenant name); a user with no active tenant and no platform role sees no role line.
- Q: What shows while identity/permissions are first resolving? → A: A shell skeleton — the frame (sidebar rail, header bar, content area) paints immediately with neutral placeholders where navigation, avatar, and switcher will appear; role-dependent controls fill in once entitlement is known. No full-screen loader.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Signed-in users land in a role-appropriate shell (Priority: P1)

After signing in, every user lands inside the application shell: a sidebar with navigation, a header with the page title and account controls, and a content area showing the current page. What the shell offers depends on who the user is. A tenant user sees the navigation for their workspace (the pages their role permits, as established by the existing permission rules). A platform user sees the tenant switcher and a dedicated header control (adjacent to the switcher) that reaches the platform destinations (tenant directory / platform administration); the sidebar itself stays tenant-only, showing the workspace navigation once they have switched into a tenant. Tenant users never see the tenant switcher or any platform navigation. The account menu in the shell shows the signed-in user's real name, email, and role — never placeholder data — and offers sign-out.

**Why this priority**: The shell is the frame every authenticated screen lives in. If the frame shows the wrong navigation for the user's role, leaks platform controls to tenant users, or displays placeholder identity data, every page inside it inherits the problem. This story is the load-bearing acceptance criterion of the feature.

**Independent Test**: Sign in as a tenant user and as a platform user; verify each sees exactly their navigation set, the switcher appears only for the platform user, and the account menu shows the real signed-in identity with a working sign-out.

**Acceptance Scenarios**:

1. **Given** a tenant user signs in, **When** the dashboard loads, **Then** the shell shows tenant navigation (filtered by their permissions), no tenant switcher, and no platform navigation entries.
2. **Given** a platform user signs in, **When** the dashboard loads, **Then** the header shows the tenant switcher and the platform navigation control, and both reach their destinations.
3. **Given** a platform user who has not yet switched into a tenant, **When** the dashboard loads, **Then** the sidebar contains no tenant-workspace entries until they select a tenant, after which the tenant navigation for that workspace appears; platform destinations remain reachable from the header throughout.
4. **Given** any signed-in user, **When** they open the account menu from the header avatar, **Then** it shows their actual display name, email, and context-aware role (platform role for staff; active-tenant role with tenant name for tenant users), and offers a sign-out action that ends the session and returns them to the sign-in screen.
5. **Given** a signed-in user, **When** any shell surface renders (sidebar, header, menus), **Then** no fixture or placeholder identity data is displayed anywhere in the shell — the former sidebar footer user card is gone and the header avatar menu is the only identity surface.

---

### User Story 2 - Users always know where they are (Priority: P2)

Every page inside the shell presents a consistent orientation layer: a breadcrumb trail showing the user's location (e.g., "Workspace / Conversations"), a page header with the page's title and optional description/actions, and a uniform content container so pages share the same margins, maximum width, and rhythm. Following a breadcrumb link navigates to that location.

**Why this priority**: Wayfinding is the everyday usability of the shell — it prevents "where am I?" moments and gives feature pages a consistent frame to build on — but the application is functional without it as long as Story 1 holds.

**Independent Test**: Navigate to each page and verify the breadcrumb trail matches the location, breadcrumb links navigate correctly, and every page renders inside the same header/container pattern.

**Acceptance Scenarios**:

1. **Given** a user on any dashboard page, **When** the page renders, **Then** a breadcrumb trail reflects its position in the navigation hierarchy and the final crumb names the current page.
2. **Given** a breadcrumb trail with ancestor entries, **When** the user activates an ancestor crumb, **Then** they navigate to that location.
3. **Given** any two dashboard pages, **When** they render, **Then** both use the same page header and content container pattern (title placement, spacing, maximum width).

---

### User Story 3 - The shell works on any screen size (Priority: P3)

The shell adapts to the viewport. On desktop the sidebar is persistent and collapsible to an icon rail. On narrow screens (tablets and phones) the sidebar becomes an overlay drawer that opens from the header and closes after navigating or when dismissed; the header condenses to keep the essential controls reachable; content never overflows horizontally.

**Why this priority**: Support staff frequently check dashboards from tablets or phones. Valuable, but the desktop experience (Stories 1–2) is the primary workflow.

**Independent Test**: Exercise the shell at desktop, tablet, and phone widths and verify sidebar behavior (persistent/collapsible vs. drawer), header condensation, and absence of horizontal overflow.

**Acceptance Scenarios**:

1. **Given** a desktop-width viewport, **When** the user toggles the sidebar, **Then** it collapses to an icon rail and expands back, and the preference applies for the rest of the session.
2. **Given** a narrow viewport, **When** the shell renders, **Then** the sidebar is hidden by default and opens as an overlay drawer from a header control.
3. **Given** an open drawer on a narrow viewport, **When** the user picks a navigation item or dismisses the drawer, **Then** the drawer closes and the chosen page shows.
4. **Given** any supported viewport width, **When** any shell page renders, **Then** no horizontal scrolling of the overall layout occurs.

---

### User Story 4 - Waiting and emptiness look intentional (Priority: P4)

While the shell resolves the signed-in user or a page loads its data, the user sees a purposeful loading presentation instead of a blank screen or half-rendered chrome. When a page or list has nothing to show, a consistent empty state explains what would appear there and, where applicable, offers the next step. Both presentations look and behave the same across all pages.

**Why this priority**: Polish and perceived quality. The shell is usable without standardized waiting/empty presentations, but inconsistent blanks erode trust in the product.

**Independent Test**: Load the dashboard on a throttled connection and visit a page with no data; verify a consistent loading presentation appears during resolution and a consistent empty state appears for no-data views.

**Acceptance Scenarios**:

1. **Given** the shell is still resolving the signed-in user's identity and permissions, **When** the first screen paints, **Then** the shell frame renders immediately with neutral skeleton placeholders where navigation, avatar, and switcher will appear — no full-screen loader, no blank screen, and no navigation or content the user might not be entitled to.
2. **Given** a page whose content is loading, **When** the user waits, **Then** a loading presentation consistent with the rest of the dashboard is shown in the content area.
3. **Given** a page or list with no data, **When** it renders, **Then** a consistent empty state describes what belongs there and offers a next action where one exists.

---

### Edge Cases

- A signed-in user with no platform role and no tenant memberships: the shell renders a safe landing state with the account menu (so they can sign out) and no navigation entries — not an error screen or redirect loop.
- The user's identity/permission data is still loading when the shell first paints: navigation and role-dependent controls (tenant switcher, platform navigation control) must not flash into view and then disappear — they appear only once entitlement is known.
- A platform user's tenant switcher lists many tenants or tenants with very long names: the switcher remains usable (searchable, scrollable) and long names truncate without breaking the header layout.
- Sign-out fails or the session has already expired server-side: the user is still returned to the sign-in screen with local state cleared.
- The user deep-links to a page while on a narrow viewport: the drawer stays closed and the page renders; opening the drawer shows the current page highlighted.
- Theme is set to follow the system while the operating system switches appearance mid-session: the shell follows without a reload, and every shell surface (sidebar, header, menus, breadcrumbs, loading/empty states) renders legibly in both light and dark.
- A page exists outside the primary navigation (e.g., a detail page): the breadcrumb still shows a sensible trail rooted at its parent section.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The dashboard MUST present a single authenticated shell — sidebar navigation, header, and content area — that frames every post-sign-in page.
- **FR-002**: The shell MUST derive all identity displays (name, email, role) from the signed-in user's actual session data; placeholder or fixture identity data MUST NOT appear anywhere in the shell.
- **FR-003**: Navigation MUST be role-appropriate: the sidebar shows only tenant-workspace pages the user's permissions allow; platform users additionally get a dedicated header control (adjacent to the tenant switcher) reaching the platform pages their platform permissions allow. Tenant users MUST never see the platform control.
- **FR-004**: A platform user's tenant-workspace navigation MUST appear only while they have an active tenant selected, and MUST reflect their staff capabilities in that tenant; platform destinations stay reachable from the header regardless of tenant selection.
- **FR-005**: The tenant switcher MUST be visible only to platform users, and MUST never render for tenant users.
- **FR-006**: The shell MUST provide an account menu opened from an avatar control in the header — the shell's single identity surface — showing the user's name, email, and context-aware role (platform role for staff; active-tenant role plus tenant name for tenant users; no role line when neither applies), with a sign-out action that ends the session, clears local state, and returns to the sign-in screen — including when the session is already invalid. The sidebar footer user card is removed.
- **FR-007**: Every page MUST render a breadcrumb trail reflecting its position in the navigation hierarchy; ancestor crumbs MUST navigate to their location.
- **FR-008**: Every page MUST use a shared page header (title, optional description and actions) and a shared content container that enforces consistent margins, maximum width, and spacing.
- **FR-009**: The shell MUST adapt to viewport width: persistent, collapsible-to-rail sidebar on wide viewports; hidden-by-default overlay drawer on narrow viewports that closes on navigation or dismissal.
- **FR-010**: The overall shell layout MUST never scroll horizontally at any supported viewport width.
- **FR-011**: The shell MUST retain the existing theme control (light / dark / follow-system, persisted across sessions), and every shell surface introduced or changed by this feature MUST render correctly in both light and dark themes, including live system-theme changes.
- **FR-012**: While identity and permission data is being resolved, the shell frame MUST paint immediately with neutral skeleton placeholders in place of navigation, avatar, and switcher, and MUST NOT render role-dependent navigation or controls that could appear and then vanish; the placeholders resolve into real controls once entitlement is known.
- **FR-013**: The shell MUST offer a single consistent loading presentation and a single consistent empty-state presentation, reused by all pages, with the empty state describing the missing content and offering a next action where one exists.
- **FR-014**: All shell building blocks (shell frame, sidebar, top navigation, breadcrumb, page header, account menu, tenant switcher, theme control, loading state, empty state) MUST be reusable components consumed by pages rather than re-implemented per page.
- **FR-015**: A signed-in user with no roles or memberships MUST receive a safe shell state with account access and sign-out, never an error loop.

### Key Entities

- **Navigation Item**: A labeled destination with an icon, target location, required permission, and surface (sidebar group for tenant-workspace pages; header platform control for platform pages). Visibility is entitlement-driven.
- **Breadcrumb Trail**: The ordered list of locations from the section root to the current page; each entry has a label and, for ancestors, a navigable target.
- **User Identity Summary**: The signed-in user's display name, email, and context-aware role designation (platform role for staff; active-tenant role otherwise) as shown in the account menu — sourced from session data, never stored separately by the shell.
- **Shell Layout State**: The user's current presentation preferences and viewport-driven mode — sidebar expanded/collapsed (session-scoped), drawer open/closed (narrow viewports), theme choice (persisted).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of shell surfaces show real session identity — zero occurrences of placeholder names, emails, roles, or companies after sign-in.
- **SC-002**: For every role, the visible navigation exactly matches that role's permitted pages, and the tenant switcher renders for 100% of platform users and 0% of tenant users.
- **SC-003**: On every dashboard page, the breadcrumb trail correctly names the current location and every ancestor crumb navigates to its target.
- **SC-004**: At all supported viewport widths (down to small phones), the shell exhibits no horizontal overflow and all navigation remains reachable within two interactions.
- **SC-005**: During identity resolution and page loads, users never see role-dependent controls flash and disappear — verified across sign-in, refresh, and tenant-switch flows.
- **SC-006**: Every shell surface passes visual review in both light and dark themes with no illegible or unstyled elements.
- **SC-007**: All pages consume the shared header, container, loading, and empty-state patterns — zero per-page reimplementations of these patterns in the dashboard.

## Assumptions

- **Existing foundations are reused, not rebuilt**: authentication and session handling (feature 007), tenant context and switching (feature 006), permission-driven visibility (feature 008), and the established visual system with its light/dark/system theme control (feature 003) are the base this shell consolidates. Where a capability already exists (e.g., theme cycling, permission-filtered tenant navigation, platform-only switcher visibility), this feature preserves and completes it rather than reinventing it.
- **Platform navigation scope**: the platform area currently contains the platform administration pages (tenant directory/overview), reached from the header control. The control must accommodate additional platform destinations later without rework; only currently existing pages get entries now.
- **Account menu contents**: identity summary (name, email, role) plus sign-out. Additional entries (profile, preferences) are future features; the menu must accommodate them without redesign.
- **Breadcrumb depth**: current pages sit one level below their section, so trails are typically two levels (section / page); the mechanism supports deeper trails for future detail pages.
- **Header search, notifications, and the "New" action** remain visual-only placeholders per the prior visual-system decision; wiring them up is out of scope here.
- **Supported viewports**: evergreen browsers from small phones (~360px wide) through desktop; no native-app or offline requirements.
- **Sidebar collapse preference** remains session-scoped (not persisted) and theme choice remains persisted, consistent with the established state rules.
