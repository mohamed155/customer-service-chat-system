# Feature Specification: Billing Foundation

**Feature Branch**: `031-billing-foundation`

**Created**: 2026-07-24

**Status**: Draft

**Input**: User description: "Billing Foundation — Prepare subscription and billing management. Scope: Plans, Subscription status, Usage limits, Token usage, Conversation usage, Billing page. Backend: store plan and subscription data, track usage, enforce basic limits. Frontend: billing overview page, usage cards, plan display, limit warnings."

## Clarifications

### Session 2026-07-24

- Q: What does the "past due" subscription status do behaviorally? → A: Same as canceled — AI replies are paused immediately while the subscription is past due.
- Q: Where do platform users manage plans and subscriptions? → A: Platform dashboard UI — pages for the plan catalog and per-tenant subscription assignment, plus the underlying endpoints.
- Q: What allowances does the seeded default free plan carry? → A: 100,000 AI tokens and 100 conversations per month.
- Q: Do plans carry a price in this foundation? → A: Yes — each plan stores a monthly display price shown on the tenant Billing page and platform plan catalog; informational only, nothing is charged.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Tenant views billing overview (Priority: P1)

A tenant Owner or Admin opens the Billing page in the dashboard and sees, at a glance: the plan their organization is on (name, included allowances), the subscription status (active, past due, or canceled), the current billing period and when usage resets, and usage cards showing how much of the AI token allowance and conversation allowance has been consumed this period.

**Why this priority**: Visibility is the foundation everything else builds on. Without a place to see plan, status, and consumption, limits and warnings have no home, and tenants cannot self-serve answers to "what plan am I on and how much have I used?" — the most common billing question.

**Independent Test**: Can be fully tested by signing in as a tenant Owner/Admin, opening the Billing page, and verifying the displayed plan, status, period, and usage figures match the tenant's stored subscription and recorded usage. Delivers standalone value as a read-only billing dashboard.

**Acceptance Scenarios**:

1. **Given** a tenant subscribed to a plan with recorded token and conversation usage this period, **When** an Owner or Admin opens the Billing page, **Then** they see the plan name, its monthly display price, subscription status, current-period date range, and two usage cards (tokens, conversations) each showing consumed amount, included allowance, and percentage used.
2. **Given** a tenant whose plan has an unlimited allowance for a metric, **When** the Billing page loads, **Then** that usage card shows consumption without a percentage bar or limit warning.
3. **Given** a signed-in user with a role below Admin (Manager, Agent, Viewer), **When** they attempt to access the Billing page, **Then** access is denied and no billing navigation entry is shown to them.
4. **Given** a tenant with no usage yet in the current period, **When** the Billing page loads, **Then** usage cards show zero consumption rather than an error or empty state.

---

### User Story 2 - Usage limits are enforced and surfaced (Priority: P2)

As a tenant approaches or exceeds its plan allowances, the system warns and then enforces: usage cards change state when consumption crosses a warning threshold, and once an allowance is exhausted, AI-generated replies for that tenant are paused until the period resets or the plan is upgraded. Human agents can keep working normally, and customers can still reach the business.

**Why this priority**: Enforcement is what makes plans meaningful — without it, allowances are decorative and the platform absorbs unbounded AI cost. It depends on the usage figures from Story 1 being available.

**Independent Test**: Can be tested by configuring a tenant on a plan with a small allowance, driving usage past the warning threshold and then past the limit, and verifying the warning state appears and AI replies stop while human replies continue.

**Acceptance Scenarios**:

1. **Given** a tenant whose token or conversation usage crosses 80% of its allowance, **When** an Owner or Admin views the Billing page, **Then** the affected usage card shows a clearly distinguishable warning state.
2. **Given** a tenant whose token allowance is exhausted, **When** a customer sends a message that would normally get an AI reply, **Then** no AI reply is generated, the conversation remains open for human handling, and the Billing page shows the limit as exceeded.
3. **Given** a tenant whose conversation allowance is exhausted, **When** a new customer conversation arrives on any channel, **Then** the conversation is still accepted and stored, but AI replies are paused for it, and the Billing page shows the limit as exceeded.
4. **Given** a tenant that exceeded an allowance in the prior period, **When** a new billing period begins, **Then** usage counters reset and AI replies resume automatically.
5. **Given** a tenant at 100% of an allowance, **When** usage is exactly at (not over) the limit, **Then** the system treats the allowance as exhausted and enforcement applies.

---

### User Story 3 - Platform operator manages plans and subscriptions (Priority: P3)

A platform user with billing authority (Super Admin or Finance) uses dedicated platform dashboard pages to maintain the catalog of plans (name, allowances, active/retired) and to assign or change a tenant's subscription — for example, moving a tenant from the default free plan to a paid plan, or marking a subscription past due or canceled. Every such change is audited.

**Why this priority**: Someone must be able to set what Stories 1 and 2 display and enforce. Manual platform-side management is sufficient for a foundation; self-serve purchasing comes later.

**Independent Test**: Can be tested by signing in as a platform user, changing a tenant's plan or subscription status, and verifying the tenant's Billing page reflects the change and an audit record exists.

**Acceptance Scenarios**:

1. **Given** a platform user with billing authority, **When** they assign a different plan to a tenant, **Then** the tenant's subscription reflects the new plan immediately and the new plan's allowances apply to the current period's already-recorded usage.
2. **Given** a platform user changes a tenant's subscription status to past due or canceled, **When** the tenant's Owner views the Billing page, **Then** the new status is displayed.
3. **Given** any plan assignment or subscription status change, **When** the change is saved, **Then** an audit record captures who made the change, what changed, and when.
4. **Given** a platform user without billing authority (e.g., Developer, Sales, Support), **When** they attempt to modify plans or subscriptions, **Then** the change is rejected.

---

### Edge Cases

- A tenant with no explicit subscription record is treated as being on the default free plan — the Billing page never errors for lack of billing data.
- A plan is retired while tenants are still subscribed to it: existing subscriptions keep functioning with the retired plan's allowances; the plan simply cannot be assigned to new tenants.
- A plan change mid-period: the new allowances apply immediately against usage already recorded in the current period (an upgrade can un-pause AI replies; a downgrade can trigger enforcement).
- Usage recorded at the boundary of a period rollover is attributed to the period in which it occurred; the reset is driven by the period boundary, not by page views.
- Concurrent usage close to the limit: brief overshoot past an allowance (e.g., an AI reply already in flight when the limit is hit) is tolerated; enforcement applies to subsequent AI replies.
- A past-due or canceled subscription behaves like an exhausted allowance for AI replies (AI paused, human agents unaffected) rather than locking tenants out of their data.
- While AI replies are paused, automatic escalation to a human still runs: a message that would have triggered an escalation rule still routes the conversation to the human queue. Pausing AI must not also silence the path that gets a customer to an agent.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST maintain a catalog of subscription plans, each with a name, a monthly display price, a monthly AI token allowance, a monthly conversation allowance (either allowance may be unlimited), and an active/retired flag.
- **FR-002**: System MUST associate every tenant with exactly one subscription referencing a plan and carrying a status of active, past due, or canceled; tenants without an explicit subscription are treated as subscribed to the default free plan.
- **FR-003**: System MUST track, per tenant and per monthly billing period, cumulative AI token consumption (input plus output tokens across all AI provider calls) and the count of conversations started, reusing usage that is already recorded rather than double-counting.
- **FR-004**: System MUST expose to tenant Owners and Admins the current plan, subscription status, current-period date range, and per-metric usage (consumed, allowance, percent used) — and MUST deny this view to lower tenant roles.
- **FR-005**: System MUST pause AI-generated replies for a tenant when either allowance for the current period is exhausted (consumed ≥ allowance) or when the subscription status is past due or canceled, while continuing to accept inbound customer messages and allowing human agents to reply.
- **FR-006**: System MUST automatically resume AI replies and reset displayed usage when a new billing period begins or when a plan change raises the allowance above current consumption.
- **FR-007**: The Billing page MUST visually distinguish three usage states per metric: normal, warning (at or above 80% of allowance), and exceeded (at or above 100%); unlimited allowances never enter warning or exceeded states.
- **FR-008**: Platform users with billing authority MUST be able to create and retire plans and to assign a plan or set the subscription status for any tenant, through dedicated platform dashboard pages backed by authorized endpoints; platform users without billing authority MUST be denied these operations.
- **FR-009**: Every plan or subscription change MUST produce an audit record identifying the actor, the affected tenant or plan, the before/after values, and the time of change.
- **FR-010**: All billing data MUST be tenant-isolated: a tenant user can never view or infer another tenant's plan, status, or usage.

### Key Entities

- **Plan**: A named tier in the catalog with a monthly display price and monthly allowances for AI tokens and conversations; may be active (assignable) or retired; one plan is designated the default free plan.
- **Subscription**: The link between a tenant and a plan, carrying a status (active, past due, canceled) and a record of when it was assigned or changed.
- **Usage Period Summary**: A tenant's consumption within one monthly billing period — total AI tokens and conversations started — derived from existing per-event usage records; used for display and enforcement.
- **Billing Audit Record**: Who changed which plan or subscription, the before/after values, and when — append-only.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant Owner can answer "what plan am I on, and how much of it have I used?" from a single page in under 30 seconds without contacting support.
- **SC-002**: When a tenant's allowance is exhausted, no further AI replies are generated for that tenant within the same billing period (zero over-quota AI replies after the in-flight tolerance), verified by driving a test tenant past its limit.
- **SC-003**: Usage shown on the Billing page reflects actual recorded consumption with no more than 5 minutes of lag.
- **SC-004**: 100% of plan and subscription changes appear in the audit trail with actor, change, and timestamp.
- **SC-005**: Tenants at or above 80% of any allowance see a warning on the Billing page before enforcement occurs, giving them the full remaining allowance window to act.
- **SC-006**: Enforcement never blocks a customer from reaching the business: inbound messages and human-agent replies succeed at the same rate for over-limit tenants as for in-limit tenants.

## Assumptions

- **No payment processing in this foundation**: no payment provider integration, invoices, checkout, or self-serve plan purchase. Plans are assigned manually by platform users; each plan's monthly price is stored and displayed for information only — nothing is charged.
- **Billing period is the calendar month (UTC)** for all tenants — no per-tenant anchor dates in this foundation.
- **Token usage source already exists**: AI provider calls are already recorded per tenant with input/output token counts; this feature aggregates them per period rather than introducing a second tracking mechanism. Conversation usage is counted from conversations created in the period.
- **Enforcement scope is AI replies only**: exhausted allowances pause AI-generated replies; they never block inbound customer messages, human agent replies, or dashboard access. This keeps enforcement safe for the business's end customers.
- **Warning threshold is fixed at 80%** of an allowance; it is not tenant-configurable in this foundation.
- **Default free plan**: a seeded plan with allowances of 100,000 AI tokens and 100 conversations per month acts as the fallback for tenants without an explicit subscription. Existing tenants land on this plan when the feature ships; platform users can immediately assign larger plans to tenants whose current usage exceeds it.
- **Brief overshoot is acceptable**: enforcement is checked before starting an AI reply; work already in flight when the limit is crossed may complete, so consumption can slightly exceed the allowance.
- **Platform billing authority** maps to the existing platform role set (Super Admin and Finance); tenant-side visibility maps to Owner and Admin, consistent with how other sensitive pages are gated.
- **Trial status is out of scope**: subscription status is limited to active, past due, and canceled; trials, grace periods, and dunning flows are future work.
- **Existing notification and escalation behavior is unchanged**: this feature adds no new notification triggers; surfacing limit warnings beyond the Billing page (bell notifications, emails) is future work.
