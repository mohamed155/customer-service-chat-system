# Feature Specification: AI Customer Service Platform (Software Requirements Specification)

**Feature Branch**: `001-ai-customer-service-platform`

**Created**: 2026-07-03

**Status**: Draft

**Input**: User description: "Using the Constitution as the source of truth, produce a complete Software Requirements Specification (SRS) for the AI Customer Service Platform. The specification should be implementation-ready."

---

## 1. Product Overview

The AI Customer Service Platform is an Enterprise Operating System for AI-driven
customer service. It is not a chatbot: it is a multi-tenant SaaS platform that lets
businesses configure, deploy, monitor, analyze, debug, and continuously improve AI
agents that converse with their end customers across communication channels,
escalating to human agents when needed.

Two audiences operate the system:

- **Platform Operators** (the company running the SaaS) administer tenants, billing,
  system health, and platform-wide configuration, and may enter a tenant's context
  through a controlled Tenant Switcher.
- **Tenant Organizations** (the paying businesses) configure their AI agents,
  knowledge bases, prompts, integrations, and human support teams, and serve their
  own end customers through embedded chat and other channels.

The initial release delivers web chat as the primary channel, with the architecture
explicitly prepared for future channels (voice, email, social messaging) and
integrations (CRM, ERP, marketplace) as extensions rather than rewrites.

## Clarifications

### Session 2026-07-03

- Q: Should the data model support multiple named AI agent configurations per
  tenant, or exactly one per tenant in v1? → A: Exactly one AI agent
  configuration per tenant in v1; the data model must allow a future move to
  multiple agents per tenant without a schema redesign.
- Q: How should escalated conversations be auto-assigned to human agents? → A:
  Skill/tag-based routing — escalations matched to agents by configured skill
  tags, falling back to load-based assignment (fewest active conversations)
  when no tagged agent is available.
- Q: What scale should the customer CSAT rating (FR-CONV-008) use? → A: 5-star
  rating (1–5) with an optional free-text comment.
- Q: How long is the customer session-resumption window? → A: 30 minutes of
  inactivity (default), tenant-configurable in Settings; returning within the
  window resumes the conversation, after it a new conversation starts linked
  to the same customer profile.

## 2. Goals

- **G-01**: Enable a business to go from sign-up to a working AI agent answering
  customer questions from its own knowledge base in under one day, without
  engineering effort on the tenant's side beyond embedding a chat widget.
- **G-02**: Give tenants full operational control of AI behavior: prompt versioning,
  knowledge management, tool permissions, escalation rules, and per-conversation
  execution timelines for debugging.
- **G-03**: Guarantee hard tenant isolation — no tenant can ever observe another
  tenant's data, configuration, or conversations.
- **G-04**: Keep the AI subsystem provider-independent (OpenAI, Anthropic, Gemini at
  launch) so tenants and operators can switch or mix providers without behavior
  contract changes.
- **G-05**: Provide AI-to-human handoff so that customers experience one
  continuous conversation even when a human agent takes over — made measurable
  by SC-004 (100% context sufficiency) and User Story 3's acceptance scenarios.
- **G-06**: Provide platform operators with the observability, billing, and audit
  tooling required to run the product as a commercial SaaS.

## 3. Non-Goals

- **NG-01**: Building a general-purpose LLM playground or model fine-tuning service.
- **NG-02**: Voice, email, and social channels in v1 (architecture must allow them;
  implementation is deferred — see Future Extensions).
- **NG-03**: A public app marketplace or third-party plugin execution in v1.
- **NG-04**: Native mobile applications for agents or admins in v1 (responsive web
  only; the customer chat widget must work inside mobile web views).
- **NG-05**: On-premise / self-hosted deployment in v1.
- **NG-06**: Workforce management features (shift scheduling, payroll, QA scoring of
  human agents beyond basic conversation metrics).
- **NG-07**: Direct LLM access to databases or arbitrary code execution — permanently
  excluded by the Constitution, not just deferred.

## 4. User Personas

### 4.1 Platform Users (operator staff)

| Persona | Role | Primary needs |
|---------|------|---------------|
| **Super Admin** | Owns platform configuration | Manage tenants, platform users, feature flags, AI provider credentials, global limits; full Tenant Switcher access |
| **Developer** | Platform engineering/support | Debug AI executions, inspect system health, trace requests across tenants (read-focused), manage integrations health |
| **Sales** | Commercial team | Create/provision tenants, manage plans and trials, view tenant usage summaries (no conversation content) |
| **Support** | Operator's own support staff | Assist tenants with configuration, view tenant settings and health via Tenant Switcher, cannot alter billing |
| **Finance** | Billing operations | Manage plans, invoices, payment status, usage reports; no access to conversation content |

### 4.2 Tenant Users (business staff)

| Persona | Role | Primary needs |
|---------|------|---------------|
| **Owner** | Account holder | Everything Admin can do, plus billing, plan changes, data export, tenant deletion |
| **Admin** | Tenant administrator | Manage users, roles, AI configuration, prompts, knowledge base, integrations, settings |
| **Manager** | Team lead | Monitor conversations and analytics, manage agent assignments, configure escalation rules, edit knowledge |
| **Agent** | Human support agent | Work an inbox of escalated conversations, chat with customers, view customer context and AI history |
| **Viewer** | Read-only stakeholder | View dashboards, analytics, and conversations without modifying anything |

Viewers see every page their role has read access to rendered in a read-only
state (data visible, all action controls — edit/publish/delete/save — hidden
or disabled) rather than an access-denied page; only pages with zero readable
content for the role (e.g., billing) produce access-denied.

### 4.3 Customers (end users)

The tenant's end customers. They interact only through the chat widget (or future
channels). They have no dashboard login; they may be anonymous or identified
(name/email captured by the widget or passed by the tenant's website). They expect
instant, accurate answers and a smooth transfer to a human when the AI cannot help.

### 4.4 Support Agents (human-in-the-loop)

Support Agents are Tenant Users with the Agent role, called out separately because
their workflow is distinct: they live in a real-time inbox, accept handoffs from the
AI, see the full AI conversation history and suggested context, respond to
customers, and hand conversations back to the AI or close them.

**Terminology note**: "Agent" (capitalized, unqualified) always refers to the
human Tenant User role defined in §4.2/§4.4. The AI is always referred to as
"the AI," "AI Agent," or "Agent Configuration" (the latter being its data
entity, §8) — never bare "Agent." This disambiguation is applied consistently
throughout this document.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Customer gets an AI answer (Priority: P1)

A customer opens the chat widget on a tenant's website, asks a question, and the AI
agent answers using the tenant's knowledge base, streaming its reply in real time.

**Why this priority**: This is the core value loop of the entire product. Without
it nothing else matters; with only it, the product already delivers value.

**Independent Test**: Seed one tenant with a small knowledge base, open the widget,
ask a question covered by the knowledge, and verify a correct, streamed, cited
answer within the latency target.

**Acceptance Scenarios**:

1. **Given** a tenant with an active AI agent and published knowledge, **When** a
   customer sends a question answerable from that knowledge, **Then** the AI
   responds with a relevant answer that begins streaming within 3 seconds.
2. **Given** a question not covered by tenant knowledge, **When** the customer asks
   it, **Then** the AI acknowledges the gap per configured fallback behavior (e.g.,
   offers escalation) instead of fabricating an answer.
3. **Given** two tenants with different knowledge bases, **When** customers of each
   ask the same question, **Then** each answer draws only from that tenant's own
   knowledge.

---

### User Story 2 - Tenant onboarding and AI configuration (Priority: P2)

A new business signs up, creates its organization, invites teammates, uploads
knowledge documents, configures its AI agent (persona, prompt, escalation rules),
and embeds the chat widget on its site.

**Why this priority**: Required to get any tenant to the P1 loop self-service;
gates commercial viability.

**Independent Test**: Complete the full onboarding flow with a fresh account and
verify the widget serves AI answers from the uploaded knowledge.

**Acceptance Scenarios**:

1. **Given** a new sign-up, **When** the owner completes the guided onboarding
   (organization details → invite users → upload knowledge → configure agent →
   embed widget), **Then** the widget answers questions from the uploaded content.
2. **Given** an uploaded document, **When** ingestion completes, **Then** the owner
   sees its processing status change to "ready" and can preview which content the
   AI will use.
3. **Given** an invited teammate, **When** they accept the invitation, **Then** they
   can sign in with exactly the permissions of their assigned role.

---

### User Story 3 - Human handoff (Priority: P3)

The AI recognizes it cannot help — via any of the five triggers in FR-AI-006
(explicit customer request, low confidence, sentiment/frustration signals,
configured topic rules, or repeated failed resolution attempts) — and escalates
to a human agent, who continues the same conversation in the agent inbox with
full context.

**Why this priority**: Escalation is the trust safety-net that makes enterprises
willing to put AI in front of customers.

**Independent Test**: Trigger each escalation condition and verify the conversation
appears in the agent inbox with full history, and the customer sees a continuous
thread.

**Acceptance Scenarios**:

1. **Given** an active AI conversation, **When** the customer asks for a human,
   **Then** the conversation enters the escalation queue and available agents are
   notified within 5 seconds.
2. **Given** an escalated conversation, **When** an agent accepts it, **Then** the
   agent sees the full transcript, AI reasoning summary, and customer profile, and
   the customer is told a human has joined.
3. **Given** no agent is available, **When** an escalation occurs, **Then** the
   configured offline behavior applies (queue with expectation message, or capture
   contact details for follow-up).
4. **Given** an agent resolves the issue, **When** they close or return the
   conversation to the AI, **Then** the transition is recorded and the customer
   experience remains one continuous thread.

---

### User Story 4 - Knowledge base management (Priority: P4)

A manager adds, updates, and removes knowledge sources (documents, URLs, articles)
and immediately sees how changes affect AI answers.

**Why this priority**: Knowledge quality drives answer quality; tenants iterate on
it constantly after launch.

**Acceptance Scenarios**:

1. **Given** a new document upload, **When** processing completes, **Then** the AI
   uses the new content in answers without any other configuration change.
2. **Given** a deleted knowledge source, **When** deletion completes, **Then** the
   AI no longer uses that content in any new answer.
3. **Given** an ingestion failure (unsupported/corrupt file), **When** it occurs,
   **Then** the user sees an actionable error state, and other sources remain
   unaffected.

**Independent Test**: Upload, verify answers include content; delete, verify
answers exclude it.

---

### User Story 5 - Prompt management with versioning (Priority: P5)

An admin edits the AI agent's prompt, previews behavior in a sandbox, publishes a
new version, and can roll back to any prior version.

**Why this priority**: Differentiating "operating system" capability — safe,
auditable AI behavior change management.

**Acceptance Scenarios**:

1. **Given** a draft prompt edit, **When** the admin tests it in the sandbox,
   **Then** live customer conversations remain on the currently published version.
2. **Given** a published new version, **When** new conversations start, **Then**
   they use the new version, and every conversation records which version served it.
3. **Given** a bad release, **When** the admin rolls back, **Then** the previous
   version is restored for new conversations within one minute, and the rollback is
   audit-logged.

**Independent Test**: Publish v2 with a distinct persona trait, verify new
conversations exhibit it, roll back, verify reversion.

---

### User Story 6 - Platform operations and Tenant Switcher (Priority: P6)

A platform Super Admin provisions a tenant, monitors platform health, and uses the
Tenant Switcher to enter a tenant's context to help with configuration — with every
such access audit-logged.

**Why this priority**: Required to operate the business, but only after the
tenant-facing loops exist.

**Acceptance Scenarios**:

1. **Given** a Super Admin, **When** they create a tenant with a plan, **Then** the
   tenant owner receives an activation invitation and the tenant appears in the
   platform directory.
2. **Given** a platform user with Tenant Switcher permission, **When** they enter a
   tenant context, **Then** their access is scoped to that tenant, visibly
   indicated in the UI, and recorded in the audit log with actor, tenant, time, and
   actions taken.
3. **Given** a platform Finance user, **When** they browse tenant data, **Then**
   they can see usage and billing but never conversation content.

---

### User Story 7 - Analytics and conversation insight (Priority: P7)

Managers and owners view dashboards: conversation volume, AI resolution rate,
escalation rate, response times, CSAT, top topics, and knowledge gaps; and drill
into individual conversations including the AI execution timeline.

**Why this priority**: Turns the product from a chat tool into an improvement
loop; depends on data produced by P1–P3.

**Acceptance Scenarios**:

1. **Given** a period with completed conversations, **When** a manager opens the
   analytics dashboard, **Then** volume, resolution rate, escalation rate, and
   response-time metrics for a selected date range are displayed and exportable.
2. **Given** a specific conversation, **When** an authorized user opens its detail,
   **Then** they can inspect each AI step (retrieval, tool calls, model calls,
   escalation decisions) with timing.

---

### User Story 8 - Billing and plan management (Priority: P8)

A tenant owner selects a plan, sees usage against plan limits (AI interactions,
seats, knowledge storage), receives invoices, and upgrades/downgrades; platform
Finance manages plans and handles billing exceptions.

**Why this priority**: Monetization; required for GA but not for validating the
product loop.

**Acceptance Scenarios**:

1. **Given** an active tenant, **When** usage approaches a plan limit, **Then** the
   owner is notified at configurable thresholds (default 80% and 100%).
2. **Given** a plan limit reached, **When** further AI usage is attempted, **Then**
   the configured limit behavior applies (soft-warn or hard-stop with graceful
   customer-facing fallback) — never a silent failure.
3. **Given** a billing period ends, **When** the invoice is generated, **Then** it
   itemizes plan fee and usage, and is available to Owner and platform Finance.

---

### Edge Cases

- **AI provider outage**: active conversations fail over to the configured fallback
  provider; if none is available, customers get a graceful message and optional
  escalation; operators see the incident on the health dashboard.
- **Customer sends messages while AI is responding**: messages are queued in order;
  the AI considers all pending messages before its next response.
- **Concurrent prompt edits**: two admins editing the same draft see a conflict
  warning; last-publish wins with both versions preserved in history.
- **Tenant suspension (non-payment)**: dashboard access becomes read-only, widget
  shows the tenant's configured offline message; no data is deleted during the
  grace period.
- **Oversized/unsupported knowledge upload**: rejected upfront with clear limits
  stated; partial ingestion never corrupts the existing knowledge index.
- **Customer returns after inactivity**: conversation context is restored if within
  the tenant's configured session window; otherwise a new conversation starts with
  prior history linked to the same customer profile.
- **Agent disconnects mid-handoff**: conversation returns to the escalation queue
  with elevated priority; customer is informed of the delay.
- **Tenant deletion**: soft-delete with a recovery window (default 30 days), then
  permanent purge including knowledge embeddings and stored files; audit trail of
  the deletion itself is retained.
- **Clock-skewed or replayed widget requests**: idempotency and session validation
  prevent duplicate customer messages from creating duplicate AI executions.
- **Customer requests data deletion mid-active-conversation**: the conversation
  is immediately flagged for closure and the assigned agent (or AI) is notified
  of the deletion request; content purge proceeds per FR-CUST-004 once the
  conversation reaches a terminal state (resolved/closed).
- **Webhook signing secret rotated while deliveries are in flight**: both the
  old and new secret verify successfully for a tenant-configured grace window
  (default 24 hours) so in-flight deliveries are not rejected mid-rotation.
- **Prompt rollback targets a version referencing a since-deleted or disabled
  tool**: the tool reference is treated as unavailable and the AI proceeds
  without it (per its normal graceful tool-failure behavior) rather than
  blocking the rollback.

---

## Requirements *(mandatory)*

### Functional Requirements

#### 5.1 Authentication

- **FR-AUTH-001**: System MUST provide email/password sign-up and sign-in for
  platform and tenant users, with email verification before first sign-in.
- **FR-AUTH-002**: System MUST support session management with configurable idle
  and absolute timeouts, and allow users to view and revoke their active sessions.
- **FR-AUTH-003**: System MUST provide self-service password reset via time-limited
  email link.
- **FR-AUTH-004**: System MUST support mandatory two-factor authentication,
  enforceable per tenant (for tenant users) and enforced always for platform users.
- **FR-AUTH-005**: System MUST support SSO via standard enterprise identity
  federation for tenants on eligible plans.
- **FR-AUTH-006**: System MUST lock accounts after repeated failed sign-ins
  (default: 5 attempts, 15-minute lockout) and notify the account owner.
- **FR-AUTH-007**: System MUST issue scoped API credentials for tenant
  programmatic access, revocable at any time, with last-used visibility.
- **FR-AUTH-008**: Customer widget sessions MUST be authenticated per tenant via
  widget tokens; tenants MAY pass verified customer identity to link conversations
  to known customers. Verification MUST use a short-lived signed token: the
  tenant's own backend (already trusting its logged-in customer) signs an
  identity assertion (customer id, optional email/attributes, expiry) using the
  widget's per-tenant shared secret; the widget verifies the signature before
  trusting the identity. This is distinct from platform authentication — see
  Assumption A-07.

#### 5.2 Organizations (Tenants)

- **FR-ORG-001**: System MUST support creating tenant organizations either
  self-service (sign-up) or operator-provisioned (by Sales/Super Admin). Tenant
  creation MUST auto-provision exactly one default AI Agent Configuration (a
  starter persona/prompt template) so the tenant is never in a zero-configuration
  state before onboarding completes.
- **FR-ORG-002**: Each tenant MUST have a unique identity, profile (name, logo,
  locale, timezone), lifecycle state (trial, active, suspended, pending-deletion,
  deleted), and plan assignment.
- **FR-ORG-003**: Every piece of tenant-owned data MUST belong to exactly one
  tenant, and no interface — UI, API, export, or search — may ever return data
  across tenant boundaries to tenant users.
- **FR-ORG-004**: Tenant deletion MUST be a two-step, Owner-only action
  (request + confirmation) with a 30-day recovery window before permanent purge.
- **FR-ORG-005**: System MUST enforce per-tenant limits (seats, AI usage, knowledge
  storage, API rate) according to plan, with operator-adjustable overrides.

#### 5.3 RBAC

- **FR-RBAC-001**: System MUST implement role-based access control with the fixed
  platform roles (Super Admin, Developer, Sales, Support, Finance) and tenant roles
  (Owner, Admin, Manager, Agent, Viewer) defined in the Constitution.
- **FR-RBAC-002**: Every capability in the system MUST map to explicit permissions;
  every request MUST be authorized server-side against the actor's role and tenant
  scope; client-side checks are advisory only.
- **FR-RBAC-003**: Platform users with Tenant Switcher permission MUST be able to
  assume a scoped context within a tenant; every switch and every action taken
  in-context MUST be audit-logged and visibly indicated in the UI.
- **FR-RBAC-004**: Each tenant MUST have at least one Owner at all times; the last
  Owner cannot be removed or downgraded.
- **FR-RBAC-005**: Role changes MUST take effect within one minute across all
  active sessions of the affected user.

#### 5.4 Users

- **FR-USER-001**: Tenant Admins/Owners MUST be able to invite users by email with
  a designated role; invitations expire (default 7 days) and are revocable.
- **FR-USER-002**: Users MUST be able to manage their own profile (name, avatar,
  locale, notification preferences) and security settings.
- **FR-USER-003**: Admins MUST be able to deactivate users, immediately revoking
  access while preserving their historical activity for audit.
- **FR-USER-004**: A user identity MAY belong to multiple tenants with independent
  roles per tenant, with an explicit context switcher.
- **FR-USER-005**: Agents MUST have an availability status (online, away, offline)
  and MAY be assigned one or more skill tags; both drive escalation routing.
- **FR-USER-006**: Escalation auto-assignment MUST match the escalation's tags
  (topic/conversation tags per FR-CONV-006) against available agents' skill
  tags first; when no tagged agent is available (or the escalation carries no
  matching tags), assignment MUST fall back to the available agent with the
  fewest active conversations (load-based). Tenants MAY disable auto-assignment
  in favor of manual claim from a shared queue (see FR-CONV, escalation queue).

#### 5.5 Customers

- **FR-CUST-001**: System MUST maintain per-tenant customer profiles (identifier,
  name, email if known, attributes, conversation history), created automatically on
  first contact.
- **FR-CUST-002**: System MUST merge anonymous sessions into an identified customer
  profile when identity becomes known.
- **FR-CUST-003**: Tenant users (Manager and above; Agents for customers they
  serve) MUST be able to view customer profiles and full conversation history.
- **FR-CUST-004**: Tenants MUST be able to delete a customer's data on request
  (privacy compliance), removing profile and conversation content within 30 days
  while retaining anonymized aggregate metrics.
- **FR-CUST-005**: Tenants MUST be able to attach custom attributes to customers
  (e.g., plan, region) usable by AI context and analytics segmentation.

#### 5.6 Conversations

- **FR-CONV-001**: System MUST support real-time bidirectional messaging between a
  customer and the AI agent or a human agent, with typing/streaming indicators.
- **FR-CONV-002**: Every conversation MUST have a lifecycle: open → active (AI or
  human) → waiting → resolved → closed, with all transitions recorded and
  timestamped.
- **FR-CONV-003**: Conversations MUST preserve the complete ordered message
  history, including AI messages, human agent messages, system events (handoffs,
  status changes), and customer messages.
- **FR-CONV-004**: Agents and Managers MUST be able to search and filter
  conversations by status, channel, assignee, customer, tag, date range, and
  content.
- **FR-CONV-005**: Conversations MUST support internal notes visible only to tenant
  staff, never to customers.
- **FR-CONV-006**: Conversations MUST support tags and dispositions for reporting.
- **FR-CONV-007**: System MUST auto-close inactive conversations per tenant policy
  (default: resolved after 24h idle, closed after 72h), notifying the customer
  before closure.
- **FR-CONV-008**: Customers MUST be able to rate a conversation (CSAT) at
  resolution, if the tenant enables it, using a 5-star scale (1–5) with an
  optional free-text comment.

#### 5.7 AI Agent

- **FR-AI-001**: The AI agent MUST answer customer messages using the tenant's
  published prompt version, knowledge base, conversation history, and approved
  tools — and nothing else.
- **FR-AI-002**: The AI MUST NEVER access databases or internal services directly;
  all data access flows through approved, explicitly-defined tools (Constitution
  Principle IV).
- **FR-AI-003**: AI responses MUST stream to the customer incrementally.
- **FR-AI-004**: The AI MUST attach source citations to answers derived from
  knowledge content, visible to the customer where the tenant enables it and always
  visible to tenant staff.
- **FR-AI-005**: The AI MUST compute a confidence assessment per response and apply
  tenant-configured behavior below thresholds (caveat, clarify, or escalate).
- **FR-AI-006**: The AI MUST detect and honor escalation triggers: explicit customer
  request, low confidence, sentiment/frustration signals, configured topic rules,
  and repeated failed resolution attempts.
- **FR-AI-007**: Every AI execution MUST record a complete execution timeline —
  context assembly, retrievals, tool calls with inputs/outputs, model invocations
  with provider/version, token usage, latency per step, and final decision —
  inspectable by authorized tenant and platform users.
- **FR-AI-008**: Tenants MUST be able to constrain AI behavior: blocked topics,
  required disclaimers, response length/tone parameters, and business-hours
  behavior.
- **FR-AI-009**: The AI MUST operate in the customer's language when the tenant
  enables multilingual support, within the tenant's configured language list.

#### 5.8 Knowledge Base

- **FR-KB-001**: Tenants MUST be able to add knowledge from: file upload (PDF,
  Word, text, Markdown, HTML), pasted/authored articles, and website URLs (single
  page and crawl within tenant-set bounds). Cross-tenant content deduplication
  is explicitly out of scope: each tenant's knowledge is ingested and stored
  independently even if identical to another tenant's content.
- **FR-KB-002**: Ingestion MUST be asynchronous with visible per-source status
  (queued, processing, ready, failed) and actionable failure reasons drawn from
  an enumerated set (e.g., `unsupported_format`, `file_too_large`,
  `unreachable_url`, `extraction_failed`, `quota_exceeded`). For a multi-page
  crawl, per-page failures MUST NOT fail the whole source: the source reaches
  "ready" if at least one page succeeds (with per-page failure detail retained
  for review), and "failed" only if every page fails.
- **FR-KB-003**: Knowledge sources MUST support update (re-upload/re-crawl),
  disable (temporarily excluded from AI use), and delete (permanently excluded).
  Deletion MUST remove the source's segments and embeddings from the retrievable
  index within the same 5-minute propagation window as FR-KB-004, not merely
  mark them inactive.
- **FR-KB-004**: Knowledge changes MUST be reflected in AI answers within 5 minutes
  of a source reaching "ready" state.
- **FR-KB-005**: Tenants MUST be able to organize knowledge into collections and
  scope which collections the AI agent uses.
- **FR-KB-006**: System MUST provide a retrieval test tool: enter a question, see
  which knowledge passages would be retrieved and their relevance ranking.
- **FR-KB-007**: System MUST enforce per-plan knowledge storage quotas with clear
  usage visibility.
- **FR-KB-008**: Analytics MUST surface knowledge gaps: clusters of questions with
  low-confidence or escalated answers lacking matching knowledge.

#### 5.9 Prompt Management

- **FR-PROMPT-001**: Each AI agent MUST have a prompt configuration composed of
  structured sections (persona, instructions, constraints, escalation guidance)
  rather than a single opaque text blob.
- **FR-PROMPT-002**: Prompt configurations MUST be versioned: drafts, published
  versions, full immutable history with author, timestamp, and change note.
- **FR-PROMPT-003**: Exactly one version per agent is "published" at any time; new
  conversations use the published version; every conversation records the version
  that served it.
- **FR-PROMPT-004**: Admins MUST be able to test draft prompts in a sandbox that
  simulates conversations (with real knowledge retrieval) without affecting live
  traffic.
- **FR-PROMPT-005**: Rollback to any prior version MUST be a one-step, audited
  action taking effect for new conversations within one minute. If the
  target version references a since-deleted or disabled tool, rollback MUST
  still succeed; the AI treats that tool as unavailable at runtime (see Edge
  Cases) rather than the rollback being blocked.
- **FR-PROMPT-006**: Final prompt assembly MUST be deterministic: identical inputs
  (version, knowledge results, history, customer context) produce an identical
  assembled prompt (Constitution Principle IV).

#### 5.10 Integrations

- **FR-INT-001**: System MUST provide an embeddable web chat widget, configurable
  in appearance (colors, logo, position, welcome message, launcher) per tenant
  without code changes beyond the embed snippet.
- **FR-INT-002**: System MUST provide outbound webhooks for key events
  (conversation started/escalated/resolved, CSAT submitted, knowledge ingestion
  completed/failed) with signed payloads, delivery retries, and a delivery log.
  Secret rotation MUST support a dual-secret grace window (tenant-configurable,
  default 24 hours) during which both the old and new signing secret verify
  successfully, so in-flight deliveries are never rejected by a rotation.
- **FR-INT-003**: System MUST expose a public API (see API Requirements) covering
  conversations, customers, knowledge, and analytics export for tenant
  integrations.
- **FR-INT-004**: The integration framework MUST be channel-extensible: adding a
  future channel (email, Slack, Messenger, etc.) must not require changes to
  conversation, AI, or analytics behavior contracts.
- **FR-INT-005**: Tenants MUST be able to register custom AI tools (declared
  actions calling the tenant's own endpoints) with per-tool approval, input/output
  schemas, timeouts, and per-conversation invocation visibility.

#### 5.11 Notifications

- **FR-NOTIF-001**: System MUST deliver in-app notifications and (per user
  preference) email notifications for: escalations assigned/waiting, mentions in
  internal notes, ingestion failures, plan-limit thresholds, billing events, and
  system incidents affecting the tenant.
- **FR-NOTIF-002**: Agents MUST receive real-time alerts (in-app, with optional
  sound/desktop) when escalations enter their queue.
- **FR-NOTIF-003**: Users MUST be able to configure notification preferences per
  category and channel; security-critical notices are non-optional.
- **FR-NOTIF-004**: Notification delivery MUST respect tenant quiet-hours
  configuration for non-urgent categories.

#### 5.12 Analytics

- **FR-ANLT-001**: Tenant dashboards MUST report, for selectable date ranges and
  with period-over-period comparison: conversation volume, AI resolution rate
  (resolved without human), escalation rate, first-response time, resolution time,
  CSAT (computed only over conversations with a submitted rating, per
  FR-CONV-008 — CSAT collection remains tenant-optional), active customers, and
  usage against plan.
- **FR-ANLT-002**: Analytics MUST support segmentation by channel, tag, agent,
  prompt version, and customer attributes.
- **FR-ANLT-003**: Topic analytics MUST cluster conversations into themes and rank
  them by volume, resolution rate, and CSAT, surfacing knowledge gaps (FR-KB-008).
- **FR-ANLT-004**: Dashboard data MUST be exportable (tabular download and API).
- **FR-ANLT-005**: Platform operators MUST have cross-tenant aggregate analytics
  (tenant counts, usage, revenue, AI cost, provider performance) with no access to
  conversation content through analytics surfaces.
- **FR-ANLT-006**: Dashboard metrics MUST be no more than 5 minutes stale, and
  clearly labeled with freshness.

#### 5.13 Billing

- **FR-BILL-001**: Platform operators MUST be able to define plans with: monthly
  fee, included seats, included AI interactions, knowledge storage quota, feature
  entitlements, and overage pricing.
- **FR-BILL-002**: System MUST meter billable usage (AI interactions, seats,
  storage) accurately and idempotently — a retried operation is never
  double-billed (see A-10 for the precise AI-interaction unit definition).
  At a soft (default 80%) limit, the platform takes no restrictive action beyond
  notification (FR-NOTIF-001). At a tenant-configured hard-stop limit (default
  100%), new AI interactions are declined with no visible change to the
  customer-facing conversation flow other than the AI's normal escalation
  behavior taking over (i.e., the widget continues working; only further
  AI-generated replies are withheld until the tenant raises the limit or the
  period resets) — this is never a silent failure or broken widget.
- **FR-BILL-003**: Tenants MUST see current AI-interaction/seat/storage usage
  against plan in near-real-time (≤1 hour lag; a metering pipeline SLA,
  distinct from the ≤5-minute dashboard-metrics freshness in FR-ANLT-006, which
  governs conversation/CSAT/topic analytics) and receive threshold notifications
  (FR-NOTIF-001).
- **FR-BILL-004**: System MUST generate itemized invoices per billing period,
  handle payment collection via an external payment processor, and manage dunning
  (retry, notify, suspend per policy) for failed payments. If the payment
  processor is unreachable, invoice generation MUST queue and retry rather than
  fail; tenant service continues uninterrupted, and the Owner is notified of
  delayed billing.
- **FR-BILL-005**: Plan upgrades take effect immediately with proration; downgrades
  take effect at the next billing period, with validation that current usage fits
  the target plan.
- **FR-BILL-006**: Trials MUST be supported with configurable length and
  limits, converting to paid or expiring to read-only.

#### 5.14 Settings

- **FR-SET-001**: Tenant settings MUST cover: organization profile, branding
  (widget and email), business hours and holidays, default locale/timezone,
  escalation and auto-close policies, CSAT enablement, data retention windows, and
  security policies (2FA enforcement, session limits, SSO).
- **FR-SET-002**: Platform settings MUST cover: provider credentials and routing
  policy, plan catalog, platform user management, global rate limits, and default
  tenant policies.
- **FR-SET-003**: All settings changes MUST take effect without downtime and be
  audit-logged with before/after values.

#### 5.15 Feature Flags

- **FR-FLAG-001**: Platform operators MUST be able to enable/disable features
  globally, per plan, and per individual tenant.
- **FR-FLAG-002**: Flag changes MUST propagate to running sessions within 5 minutes
  without deployment or restart.
- **FR-FLAG-003**: Flag evaluations MUST fail safe: an unresolvable flag uses its
  defined default, never blocking core conversation flow.
- **FR-FLAG-004**: Flag state and change history MUST be visible to operators and
  audit-logged.

#### 5.16 Audit Logs

- **FR-AUDIT-001**: System MUST record an immutable audit event for every sensitive
  operation: authentication events, role/permission changes, Tenant Switcher usage,
  prompt publishes/rollbacks, settings changes, knowledge deletions, data exports,
  customer-data deletions, billing changes, API credential lifecycle, and feature
  flag changes.
- **FR-AUDIT-002**: Each audit event MUST capture actor, actor type (platform user,
  tenant user, system, API credential), tenant scope, action, target, timestamp,
  origin (IP/user agent), and before/after summary where applicable.
- **FR-AUDIT-003**: Tenant Owners/Admins MUST be able to search their tenant's
  audit log; platform Super Admins the platform-wide log; audit data is read-only
  for everyone.
- **FR-AUDIT-004**: Audit logs MUST be retained for at least 12 months (tenant
  plan may extend) and be exportable.

#### 5.17 System Health

- **FR-HEALTH-001**: Platform operators MUST have a health dashboard covering:
  service availability, error rates, latency percentiles, queue depths (ingestion,
  escalation, webhooks), AI provider status/latency/error rates, and active
  incident state.
- **FR-HEALTH-002**: System MUST expose liveness/readiness signals per component
  for automated operations.
- **FR-HEALTH-003**: Operators MUST be able to declare incidents with status
  updates surfaced to affected tenants (banner + notification).
- **FR-HEALTH-004**: Alerting thresholds on health metrics MUST be configurable by
  operators, with notification to on-call platform staff.

#### 5.18 AI Providers

- **FR-PROV-001**: System MUST support OpenAI, Anthropic, and Gemini at launch
  behind a uniform provider abstraction covering chat completion, streaming, tool
  calling, and embeddings.
- **FR-PROV-002**: Adding a new provider MUST require implementing only the
  abstraction interface — no changes to prompt management, conversation flow, or
  analytics (Constitution Principle IV; Goal G-04).
- **FR-PROV-003**: Operators MUST configure provider credentials, model catalogs,
  and routing policy (default provider/model per capability, per-plan or per-tenant
  overrides); tenant-facing configuration expresses capability tiers, not raw
  provider coupling, unless the tenant supplies its own provider keys (eligible
  plans).
- **FR-PROV-004**: System MUST support automatic failover to a configured fallback
  provider on provider outage or sustained error rates, recorded in execution
  timelines and health dashboards.
- **FR-PROV-005**: Per-provider usage and cost MUST be tracked per tenant for
  billing and margin analysis.
- **FR-PROV-006**: Provider API keys MUST be stored encrypted, never exposed in
  UI, logs, or API responses after entry (write-only with last-4 visibility).

### 6. Non-Functional Requirements

#### 6.1 Performance

- **NFR-PERF-001**: AI responses begin streaming to the customer within 3 seconds
  of message receipt (p95) under normal load.
- **NFR-PERF-002**: Dashboard pages become interactive within 2 seconds (p95).
- **NFR-PERF-003**: Human agent messages reach the customer within 500 ms (p95).
- **NFR-PERF-004**: Knowledge search/retrieval completes within 1 second (p95).
- **NFR-PERF-005**: List/browse operations over large datasets (conversations,
  customers, audit) return the first page within 2 seconds regardless of total
  data volume.

#### 6.2 Scalability

- **NFR-SCAL-001**: v1 targets: 1,000 active tenants, 10,000 concurrent customer
  conversations, 1M conversations/month platform-wide, without architecture change.
- **NFR-SCAL-002**: Per-tenant scale targets: 500 seats, 100k customers, 10 GB
  knowledge content, 50k conversations/month.
- **NFR-SCAL-003**: Load growth MUST be absorbable by adding capacity
  (horizontal scaling), not redesign, up to 10× the v1 targets.

#### 6.3 Availability

- **NFR-AVAIL-001**: Customer-facing conversation service: 99.9% monthly
  availability. Dashboards: 99.5%.
- **NFR-AVAIL-002**: Planned maintenance MUST NOT interrupt active conversations.
- **NFR-AVAIL-003**: Degraded modes: if AI is unavailable, escalation/offline
  capture still works; if analytics is unavailable, conversations are unaffected.

#### 6.4 Security

- **NFR-SEC-001**: Zero-trust: every request authenticated and authorized
  server-side; tenant isolation enforced at the data-access layer on every
  tenant-aware query (Constitution Principles II, III).
- **NFR-SEC-002**: All data encrypted in transit and at rest; secrets and provider
  keys in a managed secret store, never in source control.
- **NFR-SEC-003**: All user-supplied and AI-generated content is treated as
  untrusted and sanitized against injection attacks at every boundary (including
  prompt-injection defenses for AI context).
- **NFR-SEC-004**: Rate limiting on all public endpoints, per credential and per
  tenant; brute-force protections on authentication.
- **NFR-SEC-005**: The platform MUST be built to satisfy GDPR obligations (data
  export, deletion, processing records) and follow OWASP secure development
  practices; SOC 2 readiness is a design constraint from day one.
- **NFR-SEC-006**: Security-relevant events feed audit logs (FR-AUDIT-001) in near
  real time (≤1 minute lag).

#### 6.5 Accessibility

- **NFR-ACC-001**: All user-facing surfaces (dashboards and chat widget) conform to
  WCAG 2.1 AA: keyboard navigability, screen-reader support, contrast, focus
  management, reduced-motion support. Verified via automated axe-core scanning
  in CI (every build) plus one manual keyboard-and-screen-reader (NVDA or
  VoiceOver) pass per release.
- **NFR-ACC-002**: The chat widget remains accessible when embedded in third-party
  sites (self-contained styling and focus behavior).

#### 6.6 Internationalization

- **NFR-I18N-001**: All user-facing text is externalized for translation; v1 ships
  English with the framework proven by at least one additional locale in the
  widget and the dashboard's core flows (inbox, conversations, knowledge,
  prompts, analytics). Email notification templates and platform-operator
  surfaces remain English-only in v1.
- **NFR-I18N-002**: Dates, numbers, and currencies render per user locale; all
  timestamps stored timezone-safe and displayed in user timezone.
- **NFR-I18N-003**: Conversation content is fully Unicode, including
  right-to-left script support in the widget and inbox.

#### 6.7 Logging

- **NFR-LOG-001**: Structured logging platform-wide, every entry carrying request
  ID, tenant scope (where applicable), and actor context (Constitution
  Principle VI).
- **NFR-LOG-002**: Logs MUST NEVER contain secrets, credentials, or full
  conversation content; sensitive fields are redacted at source.
- **NFR-LOG-003**: Operational logs retained ≥30 days hot, ≥12 months archived.

#### 6.8 Monitoring

- **NFR-MON-001**: Every request traceable end-to-end via request ID and
  distributed tracing, including across AI provider calls.
- **NFR-MON-002**: Golden-signal metrics (traffic, errors, latency, saturation) per
  component, with SLO burn-rate alerting for NFR-AVAIL targets.
- **NFR-MON-003**: AI-specific monitoring: per-provider latency/error/cost, token
  consumption, escalation-rate anomalies, and confidence-score drift.

#### 6.9 Backups

- **NFR-BKP-001**: All persistent data backed up continuously with point-in-time
  recovery ≥30 days; stored files and knowledge indexes backed up daily.
- **NFR-BKP-002**: Backups encrypted, access-controlled, and stored separately
  from primary infrastructure.
- **NFR-BKP-003**: Restore procedures tested at least quarterly with documented
  results.

#### 6.10 Disaster Recovery

- **NFR-DR-001**: RPO ≤ 5 minutes for conversation and configuration data;
  RTO ≤ 4 hours for full service restoration.
- **NFR-DR-002**: A documented, rehearsed (≥ annually) DR runbook covers regional
  failure, data corruption, and provider-dependency failure scenarios.
- **NFR-DR-003**: Tenant data recovery from backup MUST be possible per-tenant
  without restoring the entire platform.

---

## 7. User Flows

### 7.1 Tenant Onboarding

1. Prospect signs up (or Sales provisions the tenant) → email verification.
2. Guided setup: organization profile → plan/trial selection → invite teammates.
3. Knowledge: upload documents / add URLs → watch ingestion status → test
   retrieval with sample questions.
4. AI agent: choose persona template → adjust prompt sections → sandbox test →
   publish v1.
5. Widget: customize appearance → copy embed snippet → verify installation
   (platform detects first widget load).
6. Go live: first real conversation appears on the dashboard; onboarding checklist
   completes.

### 7.2 Conversation Lifecycle

1. Customer opens widget → session established → new or resumed conversation.
2. Customer message → AI context assembly (see 10.2) → streamed AI reply with
   citations.
3. Loop continues; every turn appends to the execution timeline.
4. Resolution: AI confirms the issue is addressed → conversation marked resolved →
   optional CSAT prompt → auto-close after idle window.
5. At any point: escalation triggers may route to a human (7.3); staff may tag,
   note, or reassign.

### 7.3 Human Handoff

1. Trigger fires (customer request, low confidence, rule, sentiment, repeated
   failure) → conversation enters escalation queue with priority and context
   summary.
2. Available agents notified → an agent accepts, or is auto-assigned by
   skill/tag match with load-based fallback (FR-USER-006), or claims it
   manually from the shared queue if the tenant disables auto-assignment.
3. Agent sees transcript, AI summary, customer profile, and suggested knowledge →
   customer is informed a human has joined.
4. Agent converses; may consult AI-suggested replies; may return the conversation
   to the AI or resolve it.
5. If no agent available: offline behavior — queue with expectation message or
   contact capture; conversation flagged for follow-up.

### 7.4 Knowledge Ingestion

1. User adds a source (upload/URL/article) → validation (type, size, quota).
2. Source queued → processed (extraction, segmentation, indexing) → status visible
   throughout.
3. On success: source "ready"; AI uses it within 5 minutes; retrieval test
   available. On failure: actionable error; existing knowledge untouched.
4. Updates re-process atomically: old content serves until new content is ready.

### 7.5 Prompt Editing

1. Admin opens prompt editor → creates draft from current published version.
2. Edits structured sections → sandbox conversation testing against real knowledge.
3. Publishes with a change note → new version live for new conversations within
   one minute; version recorded per conversation.
4. If regression observed → one-step rollback; both actions audit-logged.

### 7.6 Tool Execution

1. Tenant registers a tool (name, description, input/output schema, endpoint,
   timeout) → tool approved/enabled for the agent.
2. During a conversation the AI decides a tool is needed → platform validates the
   call against the tool's schema and permissions → executes with timeout →
   result returned to the AI.
3. Every invocation (inputs, outputs, duration, errors) appears in the execution
   timeline; failures degrade gracefully (AI explains or escalates).

### 7.7 Analytics

1. Manager opens dashboard → selects date range and segments.
2. Reviews KPIs and topic clusters → identifies a knowledge gap.
3. Drills into example conversations → inspects execution timelines.
4. Takes action (adds knowledge, edits prompt) → later verifies metric movement
   with period-over-period comparison → exports data if needed.

### 7.8 Billing

1. Owner selects/changes plan → payment method captured via payment processor →
   plan entitlements applied immediately (upgrade) or scheduled (downgrade).
2. Usage meters accrue through the period; threshold notifications at 80%/100%.
3. Period closes → itemized invoice generated → payment collected → receipt
   issued. Failure → dunning: retries, notifications, grace period, suspension.
4. Finance (platform) handles exceptions: credits, refunds, manual adjustments —
   all audit-logged.

---

## 8. Data Model

*Conceptual entities and relationships; no storage design implied.*

### Key Entities

- **Tenant**: A customer organization. Identity, profile, lifecycle state, plan
  assignment, settings. Root of all tenant-scoped data.
- **Platform User**: Operator staff member with a platform role; may hold Tenant
  Switcher grants.
- **Tenant User**: A person's membership in a tenant with a role. One identity may
  have memberships in multiple tenants.
- **Customer**: An end user of a tenant. Identity (possibly anonymous → merged),
  attributes, linked conversations. Belongs to exactly one tenant.
- **Conversation**: A threaded interaction between a customer and the
  AI/human agents. Status, channel, assignee, tags, prompt version reference,
  CSAT (1–5 star rating plus optional comment). Belongs to one tenant and one
  customer.
- **Message**: One entry in a conversation: author type (customer, AI, agent,
  system), content, citations, timestamps. Ordered within its conversation.
- **AI Execution**: The full timeline for one AI turn: context assembly record,
  retrievals, tool invocations, model calls (provider, model, tokens, latency),
  confidence, decision. Linked to one message.
- **Agent (AI) Configuration**: A tenant's AI agent definition: persona, behavior
  constraints, knowledge scope, tool enablement, escalation rules. Exactly one
  Agent Configuration is active per tenant in v1; the relationship is modeled as
  Tenant 1—N Agent Configurations so a future move to multiple named agents per
  tenant requires no schema redesign (see Clarifications, Session 2026-07-03).
- **Prompt Version**: An immutable published (or draft) prompt configuration with
  author, timestamp, change note. Belongs to one agent configuration.
- **Knowledge Source**: An ingested document/URL/article with processing status
  and quota accounting. Belongs to one tenant, optionally to a Collection.
- **Knowledge Collection**: A named grouping of sources, scopeable to agents.
- **Knowledge Segment**: A retrievable unit of processed knowledge content derived
  from one source (conceptual unit of retrieval and citation).
- **Tool**: An approved action the AI may invoke: schema, endpoint, limits,
  enablement. Platform-provided or tenant-registered.
- **Tool Invocation**: One execution of a tool within an AI execution.
- **Escalation**: A handoff record: trigger, queue time, assignment, outcome.
- **Plan**: A commercial package: fees, quotas, entitlements. Platform-owned.
- **Subscription**: A tenant's assignment to a plan over time, with trial state.
- **Usage Record**: Idempotent metered usage events (AI interactions, seats,
  storage) attributed to a tenant and period.
- **Invoice**: An itemized bill for a tenant and period, with payment status.
- **Notification**: A message to a user via a channel with read/delivery state.
- **Webhook Subscription / Delivery**: Tenant event subscriptions and their
  delivery attempts.
- **Feature Flag / Flag Override**: Platform feature switches and their per-plan /
  per-tenant overrides.
- **Audit Event**: Immutable record of a sensitive action (see FR-AUDIT-002).
- **API Credential**: Scoped programmatic access key for a tenant.
- **AI Provider Configuration**: Operator-managed provider credentials, model
  catalog entries, and routing/failover policy.
- **Incident**: An operator-declared platform incident with status history and
  affected-tenant scope.

### Key Relationships

- Tenant 1—N: Tenant Users, Customers, Conversations, Knowledge Sources/Collections,
  Tools (tenant-registered), API Credentials, Subscriptions, Invoices, Usage
  Records, Webhook Subscriptions, Audit Events (tenant-scoped).
- Customer 1—N Conversations; Conversation 1—N Messages; Message (AI) 1—1 AI
  Execution; AI Execution 1—N Tool Invocations and 1—N retrieval references to
  Knowledge Segments.
- Agent Configuration 1—N Prompt Versions (exactly one published); Conversation
  N—1 Prompt Version (the version that served it).
- Knowledge Source 1—N Knowledge Segments; Collection 1—N Sources; Agent
  Configuration N—M Collections (scope).
- Plan 1—N Subscriptions; Subscription N—1 Tenant; Invoice N—1 Tenant; Usage
  Records N—1 Tenant.
- Escalation N—1 Conversation, N—1 assigned Tenant User (Agent).
- Feature Flag 1—N Overrides (per plan or tenant).
- Every tenant-owned entity carries its tenant identity for isolation enforcement
  (Constitution Principle II).

---

## 9. API Requirements

- **API-001 (REST-first)**: All functionality is exposed through a versioned REST
  API; the platform's own dashboards consume the same API family (API-first
  principle). Real-time messaging additionally uses a streaming channel, with REST
  fallbacks for message history.
- **API-002 (Versioning)**: APIs are versioned in the URL path (e.g., `/v1/...`).
  Breaking changes require a new major version; prior versions remain supported
  through a published deprecation window (≥6 months notice).
- **API-003 (Resources)**: Resource-oriented endpoints with consistent plural
  nouns and predictable nesting, covering at minimum: auth/sessions, tenants,
  users, invitations, roles, customers, conversations, messages, escalations,
  agent configurations, prompt versions, knowledge sources/collections, retrieval
  tests, tools, webhooks, notifications, analytics reports, plans, subscriptions,
  invoices, usage, feature flags (read for tenants), audit events, health, and
  provider configuration (platform-scoped).
- **API-004 (Pagination)**: All list endpoints use cursor-based pagination with a
  consistent envelope (items, next cursor, has-more) and a bounded page size
  (default 25, max 100).
- **API-005 (Filtering)**: List endpoints accept documented, typed filter
  parameters (equality, ranges for dates/numbers, status enums, free-text where
  supported); unsupported filters produce a validation error, never silent
  ignoring.
- **API-006 (Sorting)**: List endpoints accept a sort parameter over a documented
  field whitelist with explicit direction; default sort is documented per
  endpoint (typically newest-first).
- **API-007 (Errors)**: All errors use a standardized envelope: machine-readable
  error code, human-readable message, field-level details for validation errors,
  and the request ID. HTTP status usage is consistent platform-wide.
- **API-008 (AuthN/AuthZ)**: Dashboard calls authenticate via user session;
  programmatic calls via scoped API credentials; widget calls via widget tokens.
  Every call is authorized server-side against role and tenant scope
  (NFR-SEC-001). Rate limits per NFR-SEC-004 with standard rate-limit response
  headers.
- **API-009 (Idempotency)**: All create/mutating operations that could be retried
  accept an idempotency key; replays return the original result (supports
  FR-BILL-002 and reliable integrations).
- **API-010 (Consistency)**: Naming, timestamp format (ISO 8601 UTC),
  identifier format, and envelope conventions are uniform across all endpoints and
  documented in a public API reference.

---

## 10. Dashboard Information Architecture

### 10.1 Platform Dashboard (operator staff)

- **Overview**: platform KPIs — tenant count/growth, usage, revenue snapshot,
  incident status.
- **Tenants**: directory, lifecycle management, plan/limits overrides, Tenant
  Switcher entry point.
- **Billing**: plan catalog, invoices, payment exceptions, revenue reports
  (Finance, Super Admin).
- **AI Providers**: credentials, model catalog, routing and failover policy,
  provider health and cost (Super Admin; Developer read-only).
- **Feature Flags**: global/plan/tenant flag management and history.
- **System Health**: service status, queues, SLO dashboards, incident management
  (Developer, Super Admin).
- **Audit**: platform-wide audit search (Super Admin).
- **Platform Users**: operator staff and role management (Super Admin).

Access per platform role: Super Admin sees all; Developer sees health, providers
(read), tenants (read + switcher); Sales sees tenants (create/manage, no
conversation content) and plans (read); Support sees tenants + switcher (no
billing); Finance sees billing, plans, usage (no conversation content).

### 10.2 Tenant Dashboard (business staff)

- **Home**: KPI snapshot, onboarding checklist (until complete), alerts.
- **Inbox**: real-time conversation workspace — queues (mine, unassigned, all),
  conversation view with customer context panel and AI timeline (Agent+).
- **Conversations**: historical search/browse across all conversations
  (Manager+; Viewer read-only).
- **Customers**: customer directory and profiles (Manager+; Agent for own).
- **AI Agent**: agent configuration, prompt editor + versions + sandbox,
  escalation rules, tools (Admin+).
- **Knowledge**: sources, collections, ingestion status, retrieval testing
  (Manager+ edit; Viewer read).
- **Analytics**: dashboards, topics, exports (Manager+; Viewer read).
- **Settings**: organization, branding, widget, notifications, security, users &
  roles (Admin+), billing & plan (Owner).
- **Audit Log**: tenant audit search (Admin+).

### 10.3 Navigation & Permissions

- Persistent left navigation with role-filtered items: users never see navigation
  entries they cannot use.
- Global elements: search, notification center, user menu, tenant context
  indicator. Platform users in Tenant Switcher mode see a prominent, persistent
  banner identifying the assumed tenant context with one-click exit.
- All navigation-level gating is duplicated by server-side authorization
  (FR-RBAC-002); deep links to unauthorized content produce a clear
  access-denied state, never a data leak.
- Multi-tenant users (FR-USER-004) switch tenants from the user menu; context is
  unambiguous at all times.

---

## 11. AI Architecture (behavioral requirements)

### 11.1 Prompt Lifecycle

Draft → sandbox-tested → published (exactly one live per agent) → superseded or
rolled back; all versions immutable and retained; every conversation stamped with
its serving version (FR-PROMPT-001..006).

### 11.2 Context Assembly

For each AI turn, the context is assembled deterministically from, in defined
order: published prompt sections, tenant behavior constraints, customer profile
attributes (whitelisted), conversation memory (11.4), retrieved knowledge (11.3),
and tool results from the current turn. Identical inputs yield an identical
assembled context. Assembly is recorded in the execution timeline (FR-AI-007).

### 11.3 RAG Pipeline

Customer intent → retrieval query formation → semantic + keyword retrieval over
the agent's scoped knowledge collections → relevance ranking and thresholding →
selected segments injected as context with source attribution → citations carried
through to the response (FR-AI-004). Retrieval must respect tenant isolation
absolutely and complete within NFR-PERF-004. The retrieval test tool (FR-KB-006)
exposes this pipeline directly.

### 11.4 Memory

- **In-conversation memory**: recent messages verbatim; older history via rolling
  summarization (11.7) so long conversations stay within context limits without
  losing key facts (stated preferences, unresolved issues, commitments).
- **Cross-conversation memory**: on conversation close, durable customer facts may
  be extracted to the customer profile per tenant policy (opt-in), visible and
  editable by tenant staff — never a hidden store.

### 11.5 Tool Calling

The AI may only invoke tools explicitly enabled for its agent configuration. Every
invocation is schema-validated on input and output, executed with timeouts and
per-conversation rate limits, and fully recorded (7.6, FR-INT-005). Tools are the
sole mechanism by which the AI reaches any data or action beyond its assembled
context (FR-AI-002).

### 11.6 Provider Abstraction

A uniform capability interface (chat, streaming, tool calling, embeddings)
isolates all AI features from provider specifics. Routing policy selects
provider/model per capability with per-plan/per-tenant overrides and automatic
failover (FR-PROV-001..006). Execution timelines always record actual
provider/model used.

### 11.7 Conversation Summarization

Rolling summaries maintain long-conversation coherence (11.4); a final summary is
generated at resolution for the agent inbox, escalation context (7.3), analytics
topic clustering (FR-ANLT-003), and the customer's history view. Summaries are
marked as AI-generated wherever displayed.

### 11.8 Escalation

Escalation decisioning combines: explicit customer request (always honored),
confidence thresholds (11.9), tenant topic/keyword rules, sentiment signals, and
repeated-failure detection (FR-AI-006). Every escalation records its trigger and
appears in analytics. Escalation must function even when AI providers are down
(NFR-AVAIL-003).

### 11.9 Confidence Scoring

Each response carries a confidence assessment: a normalized 0.0–1.0 score
derived from retrieval strength, model self-assessment, and answer-knowledge
grounding. Default tenant-tunable thresholds map score to behavior: escalate
below 0.35, clarify 0.35–0.55, caveat 0.55–0.75, answer at 0.75 and above
(see research.md R-14 for the derivation). Sentiment/frustration signals used
by FR-AI-006 are classified from the last 3 customer messages into
{neutral, frustrated, angry}; `angry`, or two consecutive `frustrated`
classifications, triggers escalation. Confidence values appear in execution
timelines and analytics; sustained drift triggers operator monitoring alerts
(NFR-MON-003).

---

## 12. Risks

| # | Risk | Impact | Mitigation (spec-level) |
|---|------|--------|------------------------|
| R-01 | AI hallucination damages tenant trust | High | Grounded answers with citations (FR-AI-004), confidence gating (FR-AI-005), fallback honesty (US1-S2), knowledge-gap analytics |
| R-02 | Cross-tenant data leak | Critical | Isolation at data-access layer on every query (NFR-SEC-001), isolation test coverage as release gate, audit of switcher access |
| R-03 | Prompt injection via customer messages or knowledge content | High | Untrusted-content handling (NFR-SEC-003), tool allow-listing with schema validation, no direct data access (FR-AI-002) |
| R-04 | AI provider outage or pricing change | High | Multi-provider abstraction + failover (FR-PROV-004), degraded modes (NFR-AVAIL-003), per-provider cost tracking |
| R-05 | Runaway AI cost per tenant | Medium | Metering (FR-BILL-002), plan limits with hard/soft stops (US8-S2), operator cost dashboards (FR-ANLT-005) |
| R-06 | Escalation floods overwhelm small agent teams | Medium | Queue prioritization, offline capture, business-hours behavior (FR-AI-008), threshold alerts |
| R-07 | Privacy/regulatory exposure (customer PII in conversations) | High | GDPR deletion/export (FR-CUST-004, NFR-SEC-005), retention windows (FR-SET-001), log redaction (NFR-LOG-002) |
| R-08 | Scope breadth delays MVP | Medium | Prioritized user stories P1–P8 are independently shippable slices; NG list bounds v1 |
| R-09 | Knowledge ingestion quality varies by source format | Medium | Visible processing status + failure reasons (FR-KB-002), retrieval test tool (FR-KB-006) |
| R-10 | Billing disputes from usage metering errors | Medium | Idempotent metering (FR-BILL-002), near-real-time usage visibility (FR-BILL-003), itemized invoices |

---

## 13. Future Extensions

Per the Constitution's Future Readiness section, the architecture must accept the
following as extensions without rewriting conversation, AI, or analytics
behavior contracts (FR-INT-004, FR-PROV-002):

- **Channels**: Voice AI, Email, Facebook Messenger, Instagram, Slack, Microsoft
  Teams — each as a new channel adapter feeding the same conversation lifecycle.
- **Integrations**: CRM (customer context sync), ERP (order/account tools) — as
  tool providers and data-sync integrations.
- **Marketplace**: third-party apps, tools, and knowledge connectors with a
  review/approval model.
- **Workflow automation**: tenant-defined triggers and actions across platform
  events (beyond v1 webhooks).
- **Custom AI providers**: tenant- or operator-added providers via the same
  abstraction interface.
- **Advanced QA**: automated conversation quality scoring and coaching insights.
- **On-premise / regional deployment options** for regulated industries.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A new tenant can go from sign-up to a working AI agent answering
  from its own knowledge in under 1 day of elapsed time and under 2 hours of
  hands-on effort, without engineering support (Goal G-01).
- **SC-002**: 95% of customer messages receive a streaming AI response starting
  within 3 seconds under normal load.
- **SC-003**: Across pilot tenants, ≥60% of conversations are resolved by the AI
  without human involvement within 90 days of launch ("resolved" = reached the
  `resolved` or `closed` status per the conversation state machine in FR-CONV-002
  without ever entering `active_human`).
- **SC-004**: 100% of escalations reach an available human agent (or configured
  offline capture) with full conversation context; agent-reported context
  sufficiency ≥90%.
- **SC-005**: Zero cross-tenant data exposure incidents; isolation verification
  suite passes on every release.
- **SC-006**: Prompt rollback restores prior AI behavior for new conversations in
  under 1 minute, verified on every release.
- **SC-007**: 100% of AI responses are traceable to a complete execution timeline
  (context, retrievals, tool calls, provider, timing).
- **SC-008**: Platform sustains 10,000 concurrent conversations and 1M
  conversations/month within availability targets (99.9% conversation service).
- **SC-009**: Switching a tenant's AI provider (or failover during an outage)
  causes no conversation failures and no tenant-visible configuration change
  (defined as: no difference in the tenant's Agent Configuration, Prompt
  Version, or Routing Policy screens before and after the switch — the tenant
  never has to reconfigure anything because of a provider change or failover).
- **SC-010**: 100% of metered usage is billed exactly once (no double-billing
  under retries); usage visibility lag ≤1 hour.
- **SC-011**: ≥90% CSAT-response average of 4/5 or higher among pilot tenants'
  customers who rate AI-resolved conversations within 90 days of launch. The
  denominator is conversations with a submitted CSAT rating only (CSAT
  submission is optional per FR-CONV-008 and not all conversations will have one).
- **SC-012**: 100% of sensitive operations (per FR-AUDIT-001) produce a complete
  audit event, verified by audit coverage tests.

---

## Assumptions

- **A-01**: v1 ships one AI agent configuration per tenant; multiple named agents
  per tenant is a natural later extension and the data model must not preclude it.
- **A-02**: Web chat widget is the only customer channel in v1 (NG-02); the
  conversation model is channel-agnostic from day one.
- **A-03**: Payment collection uses an external payment processor; the platform
  owns plans, metering, and invoicing logic but not card handling.
- **A-04**: Platform operator staff are trusted-but-audited: Tenant Switcher
  access is broad for Super Admin/Support but every in-tenant action is logged
  and visible.
- **A-05**: Default data retention: conversation content retained while the
  tenant is active unless the tenant configures shorter windows; deleted per
  FR-CUST-004 / FR-ORG-004 on request or tenant deletion.
- **A-06**: English-first launch (NFR-I18N-001); AI multilingual response
  capability is tenant-configurable where provider capability allows.
- **A-07**: Tenants' end customers do not authenticate with the platform;
  identity, when present, is asserted by the tenant's website via the widget
  integration (this is distinct from the optional signed identity-assertion
  mechanism in FR-AUTH-008, which lets an already-authenticated tenant website
  pass a verified customer identity into the widget — it is not a platform
  login and grants no platform session).
- **A-08**: SSO (FR-AUTH-005, OIDC-based) and tenant-supplied provider keys
  (FR-PROV-003) are plan-gated enterprise features delivered in the GA
  hardening milestone, not v1-blocking for lower tiers; SAML support is
  post-GA.
- **A-09**: SOC 2 certification itself is a business milestone post-launch;
  design-for-auditability is in scope now (NFR-SEC-005).
- **A-10**: Usage-based billing unit for AI is the "AI interaction," precisely:
  one customer message that receives an AI-generated reply (one AiExecution),
  inclusive of every retrieval and tool call performed while producing that
  reply, counted exactly once regardless of provider retries or failover
  (idempotency key = execution id, per R-11). A customer message that results
  only in an escalation with no AI-generated reply does NOT count as an AI
  interaction. Exact commercial packaging (price per interaction, bundling)
  is a business decision the metering model must support flexibly.
- **A-11**: Default customer session-resumption window is 30 minutes,
  tenant-configurable (see Clarifications, Session 2026-07-03).
