# Feature Specification: Integrations Foundation

**Feature Branch**: `028-integrations-foundation`
**Created**: 2026-07-22
**Status**: Draft

**Input**: User description: "Integrations Foundation — Create the foundation for external integrations. Scope: integration catalog, integration connection model, secrets storage, webhook handling, integration status, integration logs. Backend: store integration configs securely, add webhook endpoint foundation, add integration health status. Frontend: integrations list, integration detail page, connect/disconnect actions, status badges. Acceptance: tenant admins can view available integrations, connect/disconnect integrations, secrets are not exposed to frontend, integration events are logged."

## Clarifications

### Session 2026-07-22

- Q: After a tenant admin disconnects an integration, what happens when they reconnect it later? → A: Reactivate the same connection record (one record per tenant+integration, forever); secrets must be re-entered on reconnect; the event log stays continuous across connect/disconnect cycles.
- Q: What should the initial integration catalog contain? → A: One connectable generic inbound-webhook integration plus non-connectable "coming soon" placeholder entries (e.g., Slack, Microsoft Teams, CRM).
- Q: How long should accepted webhook delivery payloads be retained? → A: 90 days, same as the integration event log.
- Q: What happens to an existing connection when the platform retires its catalog entry? → A: The connection keeps working and stays updatable/disconnectable; availability blocks new connections only (so a retired entry cannot be reconnected once disconnected).
- Q: The spec restricted integrations to Owner/Admin, but the platform's existing permission matrix already grants integrations access more broadly — which wins? → A: The existing matrix wins: Owner, Admin, and Manager can view and manage; Viewer (and platform staff Developer in tenant context) can view read-only; Agent has no access.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Browse the integration catalog and see connection state (Priority: P1)

A tenant user with integrations view access (Owner, Admin, Manager, or Viewer) opens an "Integrations" page in the dashboard and sees the catalog of integrations the platform offers. Each catalog entry shows its name, short description, category, and a status badge indicating whether the tenant has it connected, disconnected, or in an error state. Selecting an entry opens a detail page with a fuller description, the connection's current status and configuration summary, and the history of recent integration events.

**Why this priority**: Visibility is the prerequisite for everything else — without a catalog and per-tenant status view there is nothing to connect, monitor, or debug. It is independently valuable as a read-only surface even before connections can be made.

**Independent Test**: Seed the catalog with at least one integration, sign in as a tenant Owner or Admin, and verify the list page shows the catalog with correct status badges and the detail page shows the entry's information. Sign in as an Agent and verify the integrations area is not accessible; sign in as a Viewer and verify the area is visible but read-only.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant user with integrations view access and a non-empty integration catalog, **When** they open the integrations page, **Then** they see every available catalog entry with name, description, category, and a status badge reflecting their tenant's connection state.
2. **Given** a tenant that has never connected anything, **When** a user with view access views the list, **Then** every connectable entry shows a "not connected" state and no connection details, and "coming soon" entries are visibly marked as not yet connectable.
3. **Given** a signed-in tenant user with the Agent role, **When** they attempt to open the integrations area, **Then** access is denied and no catalog or connection data is returned.
4. **Given** a signed-in tenant user with the Viewer role, **When** they open the integrations area, **Then** they can see the catalog, statuses, and event logs but are offered no connect, update, or disconnect actions, and any such attempt is rejected.
5. **Given** two tenants where only tenant A has connected an integration, **When** an admin of tenant B views the same catalog entry, **Then** tenant B sees it as not connected and sees none of tenant A's configuration or events.

---

### User Story 2 - Connect and disconnect an integration (Priority: P2)

From an integration's detail page, a tenant user with integrations manage access (Owner, Admin, or Manager) connects the integration by providing the configuration the integration requires, including secret credentials (for example an API key or signing secret). After connecting, the integration shows as connected, the secret values are never displayed again in full, and the admin can update the configuration or disconnect at any time. Disconnecting deactivates the integration but preserves its event history.

**Why this priority**: Connecting is the core lifecycle action of the feature, but it depends on the catalog and detail surfaces from Story 1 existing first.

**Independent Test**: As a tenant admin, connect a catalog integration by submitting required configuration including a secret, confirm the status becomes connected and the secret is only ever shown masked, then disconnect and confirm the status updates and the secret is no longer usable.

**Acceptance Scenarios**:

1. **Given** an integration that is not connected, **When** an admin submits the required configuration including secret values, **Then** the connection is created, its status becomes "connected", and a connection event is recorded.
2. **Given** a connected integration, **When** any user or client retrieves connection details, **Then** secret values are never included in any response — at most a masked reference (such as a label or last few characters) is shown.
3. **Given** a connected integration, **When** an admin disconnects it, **Then** its status becomes "disconnected", inbound events for it are no longer accepted, previously stored secrets are no longer usable, and a disconnection event is recorded.
4. **Given** a previously disconnected integration, **When** an admin reconnects it, **Then** the same connection is reactivated with freshly entered secrets, and its event log continues from the prior history (a single continuous history per tenant + integration).
5. **Given** an admin submitting a connection form with missing required fields, **When** they attempt to save, **Then** the save is rejected with a clear indication of what is missing and no partial connection is created.
6. **Given** a connected integration, **When** an admin replaces a secret value, **Then** the new value takes effect, the old value stops working, and the change is recorded as an event without exposing either value.

---

### User Story 3 - Receive webhooks and monitor integration health (Priority: P3)

An external system sends events to a webhook address that is unique to a tenant's integration connection. The platform accepts and acknowledges valid deliveries, rejects deliveries that fail verification or target inactive connections, and records every outcome in the integration's event log. The integration's health status (healthy, error, disconnected) is derived from recent activity so an admin can see at a glance whether an integration is working, and can open the event log to diagnose failures.

**Why this priority**: Webhook intake and health reporting make integrations operational and debuggable, but they only matter once connections exist (Stories 1–2).

**Independent Test**: Connect an integration, send a valid delivery to its webhook address and verify it is acknowledged and logged; send an invalid or misaddressed delivery and verify it is rejected and logged; verify the status badge reflects recent failures.

**Acceptance Scenarios**:

1. **Given** a connected integration with a webhook address, **When** the external system sends a correctly authenticated delivery, **Then** the platform acknowledges it promptly and records a received event in that integration's log.
2. **Given** a delivery whose authenticity cannot be verified against the connection's stored secret, **When** it arrives, **Then** it is rejected, no integration action is taken, and the rejection is recorded.
3. **Given** a disconnected integration or an unknown webhook address, **When** a delivery arrives, **Then** it is rejected without revealing whether the address ever existed.
4. **Given** an integration whose recent deliveries have repeatedly failed verification or processing, **When** an admin views the integrations list or detail page, **Then** the status badge shows an error state and the event log shows the failing entries with timestamps and reasons.
5. **Given** an integration with a mix of successful and failed events, **When** an admin opens its event log, **Then** events are listed newest-first with type, outcome, and time, and can be paged through.

---

### Edge Cases

- Connecting an integration that is already connected: the attempt is rejected as a duplicate; changes to an active connection go through the update flow instead.
- A catalog entry is retired by the platform (availability turned off) while tenants have it connected: existing connections keep working — they continue to accept deliveries and can be updated or disconnected — but the entry can no longer be newly connected, and a tenant that disconnects it cannot reconnect.
- A webhook delivery arrives while the connection is being disconnected: the delivery is either fully processed or fully rejected — never half-applied.
- Very large or malformed webhook payloads: rejected with a size/format limit without destabilizing the platform.
- A burst of webhook deliveries far above normal volume: the platform throttles or rejects the excess without affecting other tenants.
- An admin loses the Admin role while on the integrations page: subsequent actions are rejected by the platform regardless of what the page still displays.
- Secrets in logs: no secret value ever appears in integration event logs, audit records, or error messages.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The platform MUST maintain a catalog of integrations, where each entry has at minimum a stable identifier, display name, description, category, and an availability flag distinguishing connectable entries from non-connectable "coming soon" entries. The catalog is platform-managed; tenants cannot add or edit catalog entries. Attempts to connect an entry that is not available (a "coming soon" entry, or one the platform has since retired) MUST be rejected. Availability governs **new** connections only: an existing connection to an entry that later becomes unavailable MUST keep working, and MUST remain updatable and disconnectable.
- **FR-002**: Access MUST follow the platform's existing integrations permissions: users with integrations view access (tenant Owner, Admin, Manager, Viewer — and platform staff Developer in tenant context) can view the catalog, their tenant's connection statuses, and event logs; users without it (tenant Agent) MUST be denied access to the integrations area.
- **FR-003**: Users with integrations manage access (tenant Owner, Admin, Manager) MUST be able to connect a catalog integration for their tenant by supplying the configuration that the integration declares as required, including secret values. View-only users MUST NOT be able to connect, update, or disconnect.
- **FR-004**: A tenant MUST have exactly one connection record per catalog integration it has ever connected: reconnecting after a disconnect reactivates that same connection (secrets MUST be re-entered) and its event history continues unbroken. At most one connection per tenant + integration may be active at a time.
- **FR-005**: Secret values MUST be stored encrypted at rest, MUST never be returned by any read interface after being saved (only a masked reference may be shown), and MUST never appear in logs, event records, or error messages.
- **FR-006**: Users with integrations manage access MUST be able to update a connection's configuration, replace its secrets, and disconnect the integration. Disconnecting MUST deactivate the connection and stop acceptance of inbound deliveries while preserving the connection's event history.
- **FR-007**: Every connection MUST expose a status of at least: not connected, connected (healthy), error, and disconnected. The error state MUST be derived from recent integration activity (such as repeated failed deliveries or failed verification).
- **FR-008**: The platform MUST provide each active connection with a unique inbound webhook address for receiving events from the external system.
- **FR-009**: Inbound webhook deliveries MUST be verified against the connection's stored secret before acceptance; deliveries that fail verification, target inactive connections, or target unknown addresses MUST be rejected without leaking information about existing connections.
- **FR-010**: Accepted webhook deliveries MUST be acknowledged promptly and recorded; the foundation MUST store the delivery for later processing rather than requiring each integration to process synchronously.
- **FR-011**: The platform MUST record an integration event log per connection covering at minimum: connected, configuration updated, secret replaced, disconnected, delivery accepted, and delivery rejected (with reason category). Events MUST be viewable by users with integrations view access, newest-first, with pagination.
- **FR-012**: Connect, configuration change, secret replacement, and disconnect actions MUST additionally be captured in the platform's existing audit trail with who/what/when.
- **FR-013**: All integration data — connections, secrets, webhook addresses, and event logs — MUST be isolated per tenant; no tenant may observe another tenant's connection state or events.
- **FR-014**: Webhook intake MUST enforce payload size limits and per-connection rate limits so that misbehaving external systems cannot degrade the platform for other tenants.
- **FR-015**: Integration event log entries and accepted webhook delivery payloads MUST be retained for 90 days; older entries and payloads MAY be removed automatically.

### Key Entities

- **Integration (catalog entry)**: A platform-defined description of an external system that tenants can connect — identifier, name, description, category, the configuration fields it requires (including which are secret), and whether it is currently offered.
- **Integration Connection**: A tenant's instance of a catalog integration — which tenant, which integration, its non-secret configuration, its lifecycle state (connected / disconnected), its derived health, and when it was connected/disconnected and by whom. There is at most one connection record per tenant + integration for all time; reconnecting reactivates it rather than creating a new one.
- **Connection Secret**: A secret value attached to a connection (e.g., API key, signing secret) — stored encrypted, referenced by label, replaceable, never readable back in full.
- **Webhook Delivery**: An inbound event received at a connection's webhook address — arrival time, verification outcome, acceptance/rejection reason, and the stored payload for accepted deliveries (payloads retained 90 days, matching the event log).
- **Integration Event**: A log entry in a connection's history — event type (lifecycle or delivery), outcome, reason category for failures, and timestamp.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant admin can go from opening the integrations page to a successfully connected integration in under 3 minutes without documentation.
- **SC-002**: 100% of interface responses and log/event records exclude stored secret values after the moment of submission — verified by inspecting every read surface that touches connections.
- **SC-003**: Valid webhook deliveries are acknowledged within 2 seconds under normal load, and 100% of accepted and rejected deliveries appear in the connection's event log.
- **SC-004**: 100% of connect, configuration-change, secret-replacement, and disconnect actions appear both in the integration event log and in the platform audit trail.
- **SC-005**: Zero cross-tenant exposure: a tenant admin can never retrieve another tenant's connection status, configuration, webhook address, or events, verified by isolation tests.
- **SC-006**: An integration entering a failing state (repeated rejected deliveries) is reflected in its status badge within 1 minute, without the admin needing to refresh-hunt through logs.

## Assumptions

- This feature is a foundation: it delivers the catalog, connection lifecycle, secret handling, webhook intake, health, and logging mechanics. It does not implement any specific third-party integration's business behavior (no outbound calls to external services). The initial catalog is seeded with one connectable generic inbound-webhook integration (sufficient to exercise the full lifecycle end to end) plus non-connectable "coming soon" placeholder entries (e.g., Slack, Microsoft Teams, CRM).
- The catalog is global (platform-managed and identical for all tenants); per-tenant catalog curation and a partner marketplace are out of scope.
- Access reuses the platform's existing integrations permissions and role matrix unchanged: view for Owner/Admin/Manager/Viewer (and platform staff Developer in tenant context), manage for Owner/Admin/Manager, no access for Agent. Platform users with tenant context access follow the existing tenant-switching rules.
- One active connection per integration per tenant is sufficient for the foundation; multiple parallel connections to the same integration are out of scope.
- Accepted webhook deliveries are stored and acknowledged; downstream processing of those deliveries by specific integrations is out of scope for this feature.
- Outbound webhooks (platform → external system) and OAuth-based connection flows are out of scope; the foundation covers secret-based (key/token) connections and inbound deliveries.
- The 90-day event-log retention mirrors the platform's existing retention convention for high-volume operational records (notifications); the append-only audit trail keeps its own existing retention rules.
- Existing platform capabilities are reused: authentication/session handling, tenant isolation enforcement, role-based access control, and the audit trail.
