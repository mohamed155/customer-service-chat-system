# Feature Specification: WhatsApp Channel

**Feature Branch**: `029-whatsapp-channel`

**Created**: 2026-07-23

**Status**: Draft

**Input**: User description: "WhatsApp Channel — Goal: Add WhatsApp as a customer communication channel. Scope: WhatsApp provider configuration, incoming webhook, outgoing messages, customer identity mapping, conversation mapping. Acceptance Criteria: Incoming WhatsApp messages create or update conversations. AI can respond through WhatsApp. Human agents can reply through WhatsApp. WhatsApp conversations appear in the main inbox."

## Clarifications

### Session 2026-07-23

- Q: Which WhatsApp provider is the v1 integration built against? → A: Meta WhatsApp Business Cloud API directly (tenant brings its own Meta business account, access token, phone number ID).
- Q: Where does WhatsApp configuration live? → A: As a connectable entry in the existing integrations catalog; its integration detail page hosts credentials, status, webhook address, and event log.
- Q: How is a new WhatsApp sender matched to existing customers? → A: Auto-link on exact number match against the tenant's existing phone or WhatsApp identities; otherwise create a new customer. No fuzzy heuristics.
- Q: Inbound media scope for v1? → A: All inbound media (images, audio, documents, video) is fetched, stored, and viewable/downloadable in the conversation; outbound messages stay text-only.
- Q: Outbound delivery status tracking? → A: Full lifecycle — outbound messages show sent / delivered / read / failed as provider status updates arrive.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Connect a WhatsApp business number to the tenant (Priority: P1)

A tenant user with channel management access opens the WhatsApp entry in the integrations catalog, enters the credentials and business phone number details from their WhatsApp business account, and connects the channel. The platform shows the connection status (connected, error, disconnected) and gives the admin the inbound delivery address and verification value they must register with their WhatsApp provider so that customer messages start flowing in. The admin can update credentials or disconnect the channel at any time; secrets are never displayed again after entry.

**Why this priority**: Nothing else in this feature can happen — no inbound messages, no replies — until a tenant can connect its WhatsApp business number. It is also independently verifiable as a configuration surface with status feedback.

**Independent Test**: As a tenant admin, connect WhatsApp with valid credential values, confirm the status shows connected and the inbound delivery address plus verification value are displayed; confirm secrets appear only masked afterwards; disconnect and confirm inbound deliveries are refused.

**Acceptance Scenarios**:

1. **Given** a tenant with no WhatsApp channel configured, **When** an authorized user submits the required provider configuration including secret credentials, **Then** the channel is created in a connected state, a channel-configuration audit record is written, and the inbound delivery address and verification value are shown.
2. **Given** a connected WhatsApp channel, **When** any user or client retrieves the channel configuration, **Then** secret values are never returned in full — at most a masked hint is shown.
3. **Given** a connected WhatsApp channel, **When** an authorized user disconnects it, **Then** its status becomes disconnected, inbound deliveries for it are refused, and outgoing WhatsApp sends are blocked with a clear reason.
4. **Given** a submission missing required fields, **When** the user attempts to save, **Then** the save is rejected with a clear indication of what is missing and no partial configuration is stored.
5. **Given** a tenant user without channel management access (e.g., Agent), **When** they attempt to open or change WhatsApp channel settings, **Then** access is denied.

---

### User Story 2 - Incoming WhatsApp messages become inbox conversations (Priority: P2)

A customer sends a WhatsApp message to the tenant's connected business number. The platform verifies the delivery, matches the sender's phone number to an existing customer or creates a new customer with that phone number as their WhatsApp identity, and either appends the message to that customer's open WhatsApp conversation or starts a new one. The conversation appears in the tenant's main inbox alongside conversations from other channels, clearly marked as a WhatsApp conversation, and team members see the new message in near real time.

**Why this priority**: Inbound message intake is the core value of the channel — it turns WhatsApp traffic into workable conversations. It depends only on Story 1 and is valuable even before replies exist (agents can read and act out-of-band).

**Independent Test**: With a connected channel, deliver a simulated inbound WhatsApp message for a new phone number and verify a new customer, a new WhatsApp conversation, and the message all appear in the inbox; deliver a second message from the same number and verify it lands in the same conversation with no duplicate customer.

**Acceptance Scenarios**:

1. **Given** a connected channel and a message from a phone number the tenant has never seen, **When** the delivery is received and verified, **Then** a new customer is created with that phone number as their WhatsApp identity, a new WhatsApp conversation is opened, and the message appears in it.
2. **Given** a customer whose WhatsApp identity is already known and who has an open WhatsApp conversation, **When** they send another message, **Then** the message is appended to that same conversation rather than creating a new one.
3. **Given** a customer whose previous WhatsApp conversation has been closed, **When** they send a new message, **Then** a new conversation is opened for the same customer, preserving the link to their history.
4. **Given** the provider redelivers the same message more than once, **When** the duplicate deliveries arrive, **Then** the message appears exactly once in the conversation.
5. **Given** a delivery that fails verification against the channel's stored secret, or targets a tenant with no connected channel, **When** it arrives, **Then** it is rejected without creating any customer, conversation, or message, and the rejection is recorded for diagnostics.
6. **Given** WhatsApp conversations exist, **When** a team member opens the main inbox, **Then** WhatsApp conversations are listed together with other channels' conversations, each visibly identified as WhatsApp, and inbox filters by channel include WhatsApp.
7. **Given** an inbound message containing media (image, audio, video, or document), **When** it is received, **Then** the media is retrieved and stored by the platform and the conversation shows the message with its media viewable or downloadable by the team; if retrieval fails, the message still appears with its content type and a retrieval-failure indication.

---

### User Story 3 - Human agents reply through WhatsApp (Priority: P3)

An agent working a WhatsApp conversation in the inbox types a reply exactly as they would on any other channel. The platform delivers the reply to the customer on WhatsApp and records it in the conversation. If delivery fails — including because the messaging window allowed by WhatsApp has expired — the agent sees a clear failure indication on the message with the reason.

**Why this priority**: Replies close the loop and make WhatsApp a two-way channel, but they require Stories 1–2 to exist first.

**Independent Test**: In a WhatsApp conversation with a recent inbound message, send an agent reply and verify it is recorded as an outgoing WhatsApp message and handed to the provider for delivery; simulate a provider delivery failure and verify the message shows a failed state with a reason.

**Acceptance Scenarios**:

1. **Given** an assigned agent viewing a WhatsApp conversation with a recent customer message, **When** they send a reply, **Then** the reply is delivered to the customer's WhatsApp number and appears in the conversation as an outgoing agent message whose status progresses through sent, delivered, and read as the provider reports them.
2. **Given** the provider rejects or fails a delivery, **When** the failure is known, **Then** the message is marked failed in the conversation with a human-readable reason, and the conversation remains usable.
3. **Given** a WhatsApp conversation where the customer's last message is older than the messaging window WhatsApp permits, **When** an agent attempts a free-form reply, **Then** the platform blocks or fails the send with a clear explanation that the messaging window has expired.
4. **Given** a disconnected WhatsApp channel, **When** an agent attempts to reply in an existing WhatsApp conversation, **Then** the send is blocked with a clear reason rather than silently dropped.

---

### User Story 4 - AI responds automatically on WhatsApp (Priority: P4)

When a WhatsApp conversation is under AI handling, an inbound customer message triggers the tenant's existing AI agent to generate a reply, which is delivered to the customer over WhatsApp just like an agent reply. All existing AI behaviors — knowledge grounding, tool use, escalation to a human — apply unchanged; when the AI escalates, the conversation routes to human agents exactly as it does for other channels.

**Why this priority**: AI response is the platform's headline capability, but on this channel it is a reuse of the existing AI pipeline over the outbound path built in Story 3, so it comes last and mostly needs verification rather than new behavior.

**Independent Test**: With AI handling enabled, deliver an inbound WhatsApp message and verify an AI-generated reply is recorded in the conversation and dispatched to WhatsApp; trigger an escalation condition and verify the conversation is handed to human routing with the standard notifications.

**Acceptance Scenarios**:

1. **Given** a WhatsApp conversation under AI handling, **When** a customer message arrives, **Then** the AI generates a reply that is recorded in the conversation and delivered to the customer on WhatsApp.
2. **Given** the AI decides to escalate to a human, **When** the escalation occurs in a WhatsApp conversation, **Then** the standard human handoff flow (routing, availability, notifications) applies exactly as on existing channels.
3. **Given** AI reply generation fails, **When** the failure occurs, **Then** the existing AI-failure behavior (recorded failure, notification triggers) applies and no partial or duplicate message is sent to the customer.

---

### Edge Cases

- The same phone number contacts two different tenants: each tenant gets its own independent customer record and conversations; nothing leaks across tenants.
- An inbound message arrives for a customer that already exists with the same exact number recorded as a phone identity: the WhatsApp identity is added to that customer, not a duplicate profile. Numbers that differ in formatting are compared in normalized form.
- Provider redelivers webhooks out of order or after long delays: messages are deduplicated by the provider's message identity, and ordering in the conversation follows the provider's message timestamps where available.
- A delivery arrives while the channel is being disconnected: it is either fully processed or fully rejected, never half-applied.
- An outgoing reply exceeds WhatsApp's message length limit: the send is rejected with a clear reason before dispatch rather than silently truncated.
- Credentials become invalid after connection (revoked by the provider): outgoing sends fail with a recorded reason and the channel surfaces an error status so admins can re-enter credentials.
- Two agents (or the AI and an agent) reply nearly simultaneously: both messages are delivered and recorded in a consistent order; none are lost.
- An inbound message type the platform does not understand at all arrives: the delivery is acknowledged to the provider (to stop retries) and recorded as an unsupported-content entry for diagnostics.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Tenant users with channel management access MUST be able to configure a WhatsApp channel for their tenant by supplying the provider configuration, including secret credentials and the business phone number identity.
- **FR-002**: The system MUST store WhatsApp credentials as secrets that are never returned in full to any client after entry; at most a masked hint may be shown.
- **FR-003**: The system MUST present, after connection, the inbound delivery address and verification value the tenant must register with their WhatsApp provider, and MUST support the provider's endpoint verification handshake.
- **FR-004**: The system MUST expose the WhatsApp channel's status (connected, error, disconnected) to authorized tenant users, and MUST let them update configuration, rotate secrets, and disconnect; configuration changes MUST be audited.
- **FR-005**: The system MUST accept inbound WhatsApp deliveries only after verifying their authenticity against the tenant channel's stored secret, and MUST reject unverifiable or misaddressed deliveries without side effects while recording the rejection for diagnostics.
- **FR-006**: The system MUST acknowledge verified inbound deliveries promptly and process the contained messages such that provider retries do not create duplicate messages (deduplication by the provider's message identity).
- **FR-007**: For each verified inbound message, the system MUST resolve the sender's phone number to a customer within the tenant by exact number match only: match an existing customer holding that number as a WhatsApp identity; else, if a customer holds that exact number as a phone identity, attach the WhatsApp identity to that customer; else create a new customer with that WhatsApp identity. Fuzzy matching (name, email, profile heuristics) MUST NOT be used.
- **FR-008**: For each verified inbound message, the system MUST append the message to the customer's currently open WhatsApp conversation, or open a new WhatsApp conversation for that customer if none is open.
- **FR-009**: WhatsApp conversations MUST appear in the tenant's main inbox with a visible WhatsApp channel identification, MUST be included in channel filtering, and MUST update in near real time as messages arrive, consistent with existing channels.
- **FR-010**: Inbound media messages (image, audio, video, document) MUST have their media retrieved from the provider, stored under the tenant's ownership, and made viewable or downloadable within the conversation, with the media caption (if any) shown as message text; media retrieval failure MUST still produce a message entry with content type and a failure indication. Other non-text types (e.g., location, contacts, stickers) and unrecognized types MUST be acknowledged and recorded as typed entries rather than breaking intake.
- **FR-011**: Agents with access to a WhatsApp conversation MUST be able to send text replies from the inbox; each reply MUST be recorded in the conversation and dispatched to the customer's WhatsApp number.
- **FR-012**: The system MUST track the full delivery lifecycle of outgoing WhatsApp messages from provider status updates — sent, delivered, read, failed — and show the current status on each outbound message in the conversation; failures MUST carry a human-readable reason visible to the team.
- **FR-013**: The system MUST enforce WhatsApp's customer-service messaging window for free-form replies: sends attempted after the window since the customer's last inbound message MUST be blocked or failed with a clear explanation. Pre-approved template messaging outside the window is out of scope for v1.
- **FR-014**: When a WhatsApp conversation is under AI handling, inbound customer messages MUST trigger the tenant's existing AI response pipeline, and AI replies MUST be delivered over WhatsApp like agent replies, with all existing AI behaviors (knowledge grounding, tool use, escalation, failure handling) unchanged.
- **FR-015**: Human escalation from a WhatsApp conversation MUST use the existing handoff and routing flow identically to other channels.
- **FR-016**: All WhatsApp data — channel configuration, customer identities, conversations, messages, delivery records — MUST be tenant-isolated; no tenant may observe another tenant's WhatsApp traffic or configuration.
- **FR-017**: Outgoing sends attempted while the channel is disconnected or in a credential-error state MUST be blocked with a clear reason, never silently dropped.
- **FR-018**: The system MUST record channel-level activity (deliveries accepted/rejected, sends succeeded/failed) in a form authorized tenant users can inspect to diagnose problems.

### Key Entities

- **WhatsApp Channel Connection**: A tenant's link to its WhatsApp business number — provider configuration, masked secrets, business phone identity, status (connected / error / disconnected), and its inbound delivery address and verification value. At most one active connection per tenant per business number.
- **Customer WhatsApp Identity**: The mapping of a phone number to a customer within a tenant on the WhatsApp channel; unique per tenant + number; extends the customer's existing channel identities.
- **WhatsApp Conversation**: A conversation whose channel is WhatsApp, owned by one customer within one tenant, participating in the same lifecycle (open, AI/human handling, escalation, close) as conversations from other channels.
- **WhatsApp Message**: An inbound or outbound entry in a WhatsApp conversation — sender kind (customer / agent / AI), text content and/or a stored media attachment (type, caption, tenant-owned stored copy) for inbound media, provider message identity for deduplication, and — for outbound — a delivery status lifecycle (pending, sent, delivered, read, failed) with failure reason.
- **Inbound Delivery Record**: The receipt log of a webhook delivery — verification outcome, processing outcome, timestamps — used for diagnostics and retention, independent of whether it produced a message.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant admin can go from opening WhatsApp channel settings to a connected channel (with the inbound delivery details displayed) in under 5 minutes, excluding time spent inside the external provider's console.
- **SC-002**: A verified inbound WhatsApp message is visible in the tenant's inbox within 5 seconds of the platform receiving the delivery, for 95% of messages under normal load.
- **SC-003**: Repeated provider deliveries of the same message produce exactly one conversation message in 100% of cases.
- **SC-004**: 100% of inbound messages from a phone number already known to the tenant land in that customer's record — no duplicate customers are created for the same number within a tenant.
- **SC-005**: Agent and AI replies accepted by the platform are handed off for WhatsApp delivery within 5 seconds for 95% of messages; every outbound message reflects its latest reported delivery status (sent/delivered/read), and every failed delivery is visibly marked with a reason in the conversation.
- **SC-006**: WhatsApp conversations are fully workable through the existing inbox: a team can receive, read, assign, reply, and close a WhatsApp conversation end-to-end without leaving the platform.
- **SC-007**: Zero cross-tenant exposure: no WhatsApp message, identity, or configuration of one tenant is ever readable by another tenant.

## Assumptions

- **Provider model**: v1 integrates directly with Meta's WhatsApp Business Cloud API; each tenant supplies credentials for its own Meta business account and number (bring-your-own-number: access token, phone number identity, webhook verification and signing values). Alternative providers (e.g., Twilio) and platform-provisioned numbers are out of scope for v1, but the channel boundary must not preclude adding them later.
- **Configuration surface**: WhatsApp ships as a connectable entry in the existing integrations catalog, reusing the established connection lifecycle (secret handling, masked display, connect/update/rotate/disconnect, health status, per-connection event log) — its integration detail page is where credentials, status, the inbound delivery address, and the event log live. The existing integrations permission model (Owner/Admin/Manager manage, Viewer read-only, Agent none) applies to the connection, while conversation access follows existing inbox permissions.
- **One number per tenant**: v1 assumes a tenant connects one WhatsApp business number; multiple numbers per tenant is a future extension.
- **Media scope**: v1 delivers full text support in both directions and full inbound media support — images, audio, video, and documents are retrieved from the provider, stored in the platform's existing object storage under tenant ownership, and viewable/downloadable in the conversation. Outbound remains text-only; outbound media and template messages are future extensions. Non-media special types (location, contacts, stickers) are captured as typed placeholders.
- **Messaging window**: WhatsApp's customer-service window (currently 24 hours from the customer's last inbound message) limits free-form business replies; v1 enforces the window and does not implement template-based re-engagement.
- **Existing machinery reused**: Conversation lifecycle, inbox, assignment/routing, escalation, notifications, AI response pipeline, and audit logging already exist and are reused unchanged; this feature adds the WhatsApp transport, identity mapping, and channel configuration around them. The platform's data model already reserves WhatsApp as a channel and phone-number channel identities for customers.
- **Retention**: Inbound delivery records follow the platform's existing 90-day integration-log retention convention; conversations and messages follow the platform's standard conversation retention.
