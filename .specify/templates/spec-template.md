# Feature Specification: [FEATURE NAME]

**Feature Branch**: `[###-feature-name]`

**Created**: [DATE]

**Status**: Draft

**Input**: User description: "$ARGUMENTS"

## Scope & Non-Goals *(mandatory)*

**In scope**:

- [What this spec delivers]

**Non-goals** (explicitly out of scope for this spec):

- [What this spec will NOT deliver, and where it is deferred to if known]

## Users & Roles *(mandatory)*

<!--
  List every actor (platform users, tenant users, anonymous customers, system workers) and the
  permissions each needs. Authorization checks permissions, never role names (Constitution II).
-->

| Actor | Permission(s) required | What they can do |
|-------|------------------------|------------------|
| [e.g., Tenant Admin] | [e.g., `widgets.manage`] | [capability] |

## User Scenarios & Testing *(mandatory)*

<!--
  IMPORTANT: User stories should be PRIORITIZED as user journeys ordered by importance.
  Each user story/journey must be INDEPENDENTLY TESTABLE - meaning if you implement just ONE of them,
  you should still have a viable MVP (Minimum Viable Product) that delivers value.

  Assign priorities (P1, P2, P3, etc.) to each story, where P1 is the most critical.
  Think of each story as a standalone slice of functionality that can be:
  - Developed independently
  - Tested independently
  - Deployed independently
  - Demonstrated to users independently
-->

### User Story 1 - [Brief Title] (Priority: P1)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently - e.g., "Can be fully tested by [specific action] and delivers [specific value]"]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]
2. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

### User Story 2 - [Brief Title] (Priority: P2)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

### User Story 3 - [Brief Title] (Priority: P3)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

[Add more user stories as needed, each with an assigned priority]

### Edge Cases

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right edge cases.
-->

- What happens when [boundary condition]?
- How does system handle [error scenario]?

## Requirements *(mandatory)*

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right functional requirements.
-->

### Functional Requirements

- **FR-001**: System MUST [specific capability, e.g., "allow users to create accounts"]
- **FR-002**: System MUST [specific capability, e.g., "validate email addresses"]
- **FR-003**: Users MUST be able to [key interaction, e.g., "reset their password"]
- **FR-004**: System MUST [data requirement, e.g., "persist user preferences"]
- **FR-005**: System MUST [behavior, e.g., "log all security events"]

*Example of marking unclear requirements:*

- **FR-006**: System MUST authenticate users via [NEEDS CLARIFICATION: auth method not specified - email/password, SSO, OAuth?]
- **FR-007**: System MUST retain user data for [NEEDS CLARIFICATION: retention period not specified]

### Key Entities *(include if feature involves data)*

- **[Entity 1]**: [What it represents, key attributes without implementation]
- **[Entity 2]**: [What it represents, relationships to other entities]

## Security & Tenancy Impact *(mandatory)*

<!--
  Constitution I and II. Write "None" only with a reason.
-->

- **Tenant-owned data touched**: [tables/objects; how tenant scope is enforced in app + RLS]
- **Cross-tenant risks**: [ID guessing, shared caches/keys/queues, SSE streams, etc.]
- **Untrusted input boundaries**: [HTTP bodies, webhooks, files, channel messages, LLM output]
- **Secrets**: [any new credentials and where they are stored]
- **Audit events**: [sensitive actions that must write audit records]
- **AI/tool impact** (Constitution III): [new tools, prompts, guardrails, or N/A]

## Data Changes *(mandatory)*

- [New/changed tables, columns, indexes; migration strategy (expand → migrate → contract);
  idempotency keys; retention; PII export/deletion impact — or "None"]

## API Contracts *(mandatory if any API changes)*

- [Endpoints under `/api/v1`, permission per endpoint, pagination style, RFC 9457 error cases]

## UX States *(mandatory if UI changes)*

- **Loading**: [behavior]
- **Empty**: [behavior]
- **Error**: [behavior]
- **Permission denied**: [behavior]
- **Accessibility / theming**: [WCAG 2.2 AA, keyboard, light/dark, RTL, i18n notes]

## Test Strategy *(mandatory)*

<!--
  Constitution VI. Name the lowest effective level for each behavior.
-->

- **Unit**: [domain logic]
- **Integration** (real PostgreSQL/Redis): [repositories, migrations, RLS, queues]
- **Contract**: [OpenAPI endpoints]
- **Component / E2E**: [UI and critical journeys]
- **Negative / security**: [unauthorized, wrong tenant, malformed input]
- **AI evaluation**: [eval suite impact, or N/A]

## Success Criteria / Acceptance Criteria *(mandatory)*

<!--
  ACTION REQUIRED: Define measurable acceptance criteria.
  These must be technology-agnostic, measurable, and verifiable in QA (Constitution V).
-->

### Measurable Outcomes

- **SC-001**: [Measurable metric, e.g., "Users can complete account creation in under 2 minutes"]
- **SC-002**: [Measurable metric, e.g., "System handles 1000 concurrent users without degradation"]
- **SC-003**: [User satisfaction metric, e.g., "90% of users successfully complete primary task on first attempt"]
- **SC-004**: [Business metric, e.g., "Reduce support tickets related to [X] by 50%"]
- **SC-005**: [Performance budget if hot paths are touched, e.g., "API p95 < 300 ms"]

## Assumptions

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right assumptions based on reasonable defaults
  chosen when the feature description did not specify certain details.
-->

- [Assumption about target users, e.g., "Users have stable internet connectivity"]
- [Assumption about scope boundaries, e.g., "Mobile support is out of scope for v1"]
- [Assumption about data/environment, e.g., "Existing authentication system will be reused"]
- [Dependency on existing system/service, e.g., "Requires access to the existing user profile API"]
