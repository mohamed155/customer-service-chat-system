# Feature Specification: Analytics Foundation

**Feature Branch**: `025-analytics-foundation`

**Created**: 2026-07-19

**Status**: Draft

**Input**: User description: "Analytics Foundation — Create the analytics data foundation. Scope: conversation volume, AI resolution rate, human handoff rate, average response time, customer satisfaction, token usage, channel breakdown. Backend: analytics queries, aggregation tables if needed, tenant analytics APIs. Frontend: tenant analytics dashboard, metric cards, charts, date range filters, channel filters. Acceptance: tenant admins can view basic analytics, data is tenant-scoped, analytics support date filtering, feedback ratings appear in analytics."

## Clarifications

This section is the decision log — a dated record of what was asked and answered. Where a decision needs elaboration (exact formulas, boundary rules), the **Assumptions** section at the end of this document is authoritative; update it there rather than expanding entries here.

### Session 2026-07-19

- Q: What data freshness is acceptable for analytics metrics? → A: Near-real-time — metrics may lag live activity by up to ~1 minute.
- Q: What counts as an "AI-resolved" conversation? → A: A conversation that concluded without ever being escalated to a human; any escalation counts the conversation as a handoff.
- Q: How should "average response time" be measured? → A: Both — average first response time is the headline metric card; the all-pairs average (each customer message → next reply) is a secondary metric.
- Q: Who can view tenant analytics? → A: Tenant Owner, Admin, and Manager roles; Agent and Viewer roles cannot.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - View key metrics at a glance (Priority: P1)

A tenant administrator opens the Analytics page and immediately sees a set of headline metric cards for a selected date range: total conversation volume, AI resolution rate, human handoff rate, average first-response time, average customer satisfaction rating, and total AI token usage. Each metric reflects only the administrator's own tenant.

**Why this priority**: This is the core value of the feature — without headline numbers there is no analytics product. Every other story (trends, filters, breakdowns) refines this view.

**Independent Test**: Seed a tenant with a known set of conversations (some AI-resolved, some escalated, some rated) and verify each metric card shows the expected computed value; verify a second tenant with different data sees only its own numbers.

**Acceptance Scenarios**:

1. **Given** a tenant with conversations in the selected period, **When** an admin opens the analytics dashboard, **Then** metric cards display conversation volume, AI resolution rate, handoff rate, average first response time, average all-message response time, satisfaction score, and token usage computed from that tenant's data only.
2. **Given** a tenant with no conversations in the selected period, **When** an admin opens the analytics dashboard, **Then** each metric card shows an explicit empty/zero state rather than an error or misleading value.
3. **Given** two tenants with different activity, **When** an admin of tenant A views analytics, **Then** no data from tenant B is included in any metric.
4. **Given** conversations that received customer feedback ratings, **When** the admin views the satisfaction metric, **Then** the value reflects those submitted ratings for the selected period.

---

### User Story 2 - Filter analytics by date range (Priority: P2)

The administrator narrows or widens the reporting window using a date range filter (preset ranges such as last 7 days, last 30 days, last 90 days, plus a custom range). All metric cards and charts update to reflect only activity within the chosen window.

**Why this priority**: Point-in-time totals are of limited use without the ability to scope them to a period; date filtering is the first question any admin asks ("how did we do this week?").

**Independent Test**: Seed conversations on known dates, select different ranges, and verify metrics include exactly the conversations created inside each range.

**Acceptance Scenarios**:

1. **Given** conversations spread across 90 days, **When** the admin selects "last 7 days", **Then** all metrics and charts recompute to include only the last 7 days of activity.
2. **Given** a custom start and end date, **When** the admin applies it, **Then** metrics reflect only activity between those dates inclusive.
3. **Given** an applied date range, **When** the admin navigates away and returns to the analytics page, **Then** the default range (last 30 days) is restored — filter state is not persisted across navigation in this foundation, and the restored default is always the same.
4. **Given** a range with no activity, **When** it is applied, **Then** empty states are shown and charts render with zeroed axes rather than failing.

---

### User Story 3 - See trends over time in charts (Priority: P3)

Beyond headline totals, the administrator sees time-series charts (e.g., conversations per day, resolution vs. handoff over time, satisfaction trend, token usage over time) so they can spot patterns — rising volume, degrading satisfaction, or growing AI cost.

**Why this priority**: Trends turn raw numbers into decisions (staffing, AI tuning, cost control), but they depend on the metrics and date filtering existing first.

**Independent Test**: Seed conversations across multiple days with varying outcomes and verify each chart's per-day points match the seeded distribution for the selected range.

**Acceptance Scenarios**:

1. **Given** a selected date range, **When** the dashboard loads, **Then** a conversation-volume chart shows one data point per day (or per suitable interval) covering the range.
2. **Given** days with zero activity inside the range, **When** the chart renders, **Then** those days appear as zero values rather than being skipped, so the time axis is continuous.
3. **Given** conversations with recorded token usage, **When** the token usage chart renders, **Then** daily totals match the sum of usage recorded that day.

---

### User Story 4 - Break down and filter by channel (Priority: P4)

The administrator sees how activity splits across communication channels (currently the website chat widget; more channels will follow) and can filter the entire dashboard to a single channel.

**Why this priority**: Channel breakdown becomes essential as more channels launch; today it primarily establishes the foundation so future channels appear automatically.

**Independent Test**: Seed conversations attributed to different channels, verify the breakdown shows correct per-channel counts, and verify selecting a channel filter recomputes all metrics for that channel only.

**Acceptance Scenarios**:

1. **Given** conversations from more than one channel, **When** the admin views the channel breakdown, **Then** per-channel conversation counts and shares are shown for the selected date range.
2. **Given** a channel filter selection, **When** it is applied, **Then** all metric cards and charts recompute to include only that channel's conversations.
3. **Given** a tenant using only one channel, **When** the breakdown renders, **Then** it shows 100% attribution to that channel without errors.

---

### Edge Cases

- A conversation starts inside the selected range but ends (or is resolved/escalated) after it — attribution rules must be deterministic and documented (see Assumptions).
- A conversation is still open/in progress at query time — it counts toward volume but not toward resolution or handoff rates until it reaches a terminal outcome.
- Rates whose denominator is zero (e.g., resolution rate with no concluded conversations) must display as an explicit "no data" state, never as 0% or an error.
- Feedback ratings submitted days after the conversation ended — the rating must be attributed to a consistent, documented date (see Assumptions) so numbers don't silently shift between views.
- Very large tenants (hundreds of thousands of conversations) — the dashboard must stay responsive for any supported date range.
- A user with tenant context switched (platform user) sees the analytics of the currently selected tenant only, and the switch is auditable per existing platform rules.
- Deleted (soft-deleted) conversations are excluded from all metrics consistently.
- Date ranges are interpreted in a single documented timezone so a "day" boundary is unambiguous.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a tenant-scoped analytics view accessible from the tenant dashboard to authorized tenant users (Owner, Admin, Manager); Agents and Viewers MUST NOT access it unless later granted.
- **FR-002**: The system MUST report conversation volume: the count of conversations **created** in the selected period, per tenant. (Creation date is the attribution rule for every conversation-based metric — a conversation created before the period is not counted even if it was active during it.)
- **FR-003**: The system MUST report AI resolution rate: the share of concluded conversations that ended without human handoff, out of all concluded conversations in the period.
- **FR-004**: The system MUST report human handoff rate: the share of conversations in the period that were escalated to a human.
- **FR-005**: The system MUST report average first response time (headline metric): the mean time from **the customer's first message** to the first AI or human reply, averaged across conversations in the period. As a secondary metric, the system MUST also report the all-pairs average response time: the mean time from each customer message to the first subsequent reply. Both values MUST be visible in the analytics view, not only returned by the API.
- **FR-006**: The system MUST report customer satisfaction: the average submitted feedback rating and the count of ratings in the period, sourced from the existing customer feedback capability.
- **FR-007**: The system MUST report AI token usage totals for the period, and MUST be able to present usage over time.
- **FR-008**: The system MUST attribute every conversation to a communication channel and provide a per-channel breakdown of conversation volume.
- **FR-009**: All analytics MUST be filterable by date range, including preset ranges (last 7, 30, 90 days) and a custom start/end date.
- **FR-010**: All analytics MUST be filterable by channel, and the channel filter MUST compose with the date range filter.
- **FR-011**: Every analytics query and API response MUST be scoped to a single tenant; no aggregate may combine data across tenants.
- **FR-012**: Metrics with an empty denominator or no underlying data MUST render as explicit empty states, not as zero or an error.
- **FR-013**: Time-series outputs MUST include zero-valued intervals for days without activity so trend lines are continuous over the selected range.
- **FR-014**: Soft-deleted conversations and their messages MUST be excluded from all metrics.
- **FR-015**: The analytics view MUST present headline metric cards and time-series charts for at least: conversation volume, resolution vs. handoff, satisfaction trend, and token usage.
- **FR-016**: Analytics data MUST remain available and correct as new conversations, escalations, ratings, and token usage accrue — a metric viewed today for a past period MUST equal the same metric viewed later for that period, except for late-arriving feedback attributed per the documented rule.

### Key Entities

- **Analytics metric**: A named, tenant-scoped measurement (volume, resolution rate, handoff rate, response time, satisfaction, token usage) computed over a date range and optional channel filter.
- **Metric time series**: A per-interval (daily) sequence of values for one metric over the selected range, with zero-filled gaps.
- **Channel**: The communication surface a conversation arrived through (currently the website chat widget; designed to accommodate future channels such as email or messaging platforms without rework).
- **Conversation outcome**: The terminal state used for rate calculations — resolved without handoff, handed off to a human, or still open (excluded from rate denominators).
- **Feedback rating**: An existing customer-submitted score (with optional comment) attached to a conversation; the analytics source for satisfaction.
- **Token usage record**: An existing per-AI-interaction record of tokens consumed, aggregable by day and tenant.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant admin can answer "how many conversations did we have, how many did AI resolve, and how satisfied were customers" for any supported date range in under 30 seconds from opening the dashboard, without exporting data or asking an engineer.
- **SC-002**: 100% of analytics values shown to a tenant are computed exclusively from that tenant's data (verified by cross-tenant isolation tests with seeded data in multiple tenants).
- **SC-003**: For a seeded, known dataset, every displayed metric matches the independently hand-computed expected value exactly.
- **SC-004**: The analytics dashboard reaches a fully rendered state within 3 seconds for a tenant with 100,000 conversations over a 90-day range.
- **SC-005**: Feedback ratings submitted through the existing feedback flow are reflected in the satisfaction metric within 1 minute of submission.
- **SC-006**: Changing the date range or channel filter updates every metric card and chart within 2 seconds.

## Assumptions

- **Audience**: Tenant Owner, Admin, and Manager roles can view analytics; Agent and Viewer roles cannot. Platform users viewing via tenant switcher see the selected tenant's analytics under existing audit rules.
- **Conversation attribution**: A conversation is attributed to a day (and thus to a date range) by its creation date. Rates use conversations created in the range that have reached a terminal outcome by query time.
- **"AI resolution" definition**: A concluded conversation counts as AI-resolved when it ended without ever being escalated to a human. Any escalation (regardless of what happened afterward) counts the conversation toward the handoff rate.
- **Response time definition**: The headline card shows average first response time (the conversation's **first customer message** → first reply, averaged across conversations). Measuring from the first customer message rather than the conversation record's creation timestamp is deliberate: a conversation row can be created before the customer types anything, and the customer's wait only begins once they have asked something. A secondary metric shows the all-pairs average (each customer message → first subsequent reply, averaged over all pairs). Conversations with no reply are excluded from both averages but still counted in volume.
- **Satisfaction attribution**: A feedback rating is attributed to the date the rating was submitted, and is the mechanism by which late ratings can change a past period's satisfaction value (the only permitted retroactive change).
- **Timezone**: Day boundaries use UTC consistently across metrics, filters, and charts for this foundation; per-tenant timezone display is a future enhancement.
- **Channels today**: The website chat widget is the only live customer channel; the channel model must nonetheless represent conversations generically so future channels (email, messaging platforms) appear in breakdowns without redesign.
- **Freshness**: Near-real-time is sufficient; metrics may lag live activity by up to 1 minute. No requirement for streaming/live-updating dashboards in this foundation.
- **Scope boundaries**: Exports (CSV/PDF), scheduled reports, per-agent performance analytics, and cross-tenant/platform-level analytics are out of scope for this feature.
- **Existing data reuse**: Conversation records, escalation signals, feedback ratings, and AI token usage are already captured by prior features (014, 021, 023, 024); this feature aggregates them rather than introducing new capture points.
