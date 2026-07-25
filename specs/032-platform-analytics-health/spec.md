# Feature Specification: Platform Analytics & System Health

**Feature Branch**: `032-platform-analytics-health`

**Created**: 2026-07-25

**Status**: Draft

**Input**: User description: "Platform Analytics & System Health — Goal: allow platform users to monitor the SaaS system. Scope: global tenant count, active conversations, AI usage, provider errors, system health, queue health, storage health. Frontend: platform overview dashboard, system health page, AI provider status page. Acceptance: platform users can monitor global usage; tenant users cannot access platform analytics; health indicators are visible; provider errors are trackable."

## Clarifications

### Session 2026-07-25

- Q: Is the two-permission split (new `platform.analytics.view` for the overview, existing `platform.diagnostics.view` for health/provider pages) the intended access model? → A: Yes — split as specified.
- Q: How should the cross-tenant aggregates meet the 2-second budget at scale? → A: Live query on every request, backed by new purpose-built indexes; no cache, no rollup tables.
- Q: What should the storage reachability probe actually do, given a 30-second refresh? → A: Read-only probe (metadata/list); never writes objects.
- Q: How does the new diagnostics surface relate to the existing public `/health`, `/ready`, `/metrics` probes? → A: A separate permission-gated platform endpoint; the existing probes keep their current contract and stay untouched.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Platform staff see the whole business at a glance (Priority: P1)

A platform staff member signs in and opens the Platform Overview. Without switching into any individual tenant, they see how many tenants exist and how many are active vs suspended, how many conversations are currently live across the entire platform, and how much AI the platform consumed over a chosen period (calls, tokens, and success/failure split). The numbers span every tenant, not just whichever tenant is currently selected in the tenant switcher.

**Why this priority**: This is the headline ask — "platform users can monitor global usage." It delivers standalone value even if no health page ever ships, and it replaces the existing empty Platform Overview placeholder page with something real.

**Independent Test**: Sign in as each platform role, open Platform Overview, and confirm global tenant/conversation/AI-usage figures render and match independently computed totals across seeded tenants. Fully testable without any health or provider page existing.

**Acceptance Scenarios**:

1. **Given** three tenants exist (two active, one suspended) with conversations in several states, **When** a platform user opens the Platform Overview, **Then** the tenant count shows 3 total with the active/suspended breakdown, and soft-deleted tenants are excluded from all counts.
2. **Given** conversations exist across multiple tenants, **When** the overview loads, **Then** the "active conversations" figure counts conversations in open or pending state across all tenants combined.
3. **Given** AI calls were recorded across multiple tenants over the last 30 days, **When** the platform user selects a date range, **Then** total calls, input/output tokens, and the success/failure split for that range are shown, aggregated across all tenants.
4. **Given** a platform user has switched their tenant context to a specific tenant, **When** they open the Platform Overview, **Then** the figures still reflect the entire platform and are not filtered to the switched tenant.
5. **Given** a tenant user (any tenant role, no platform role) is signed in, **When** they request the platform analytics data directly, **Then** the request is rejected by the server and no aggregated data is returned.

---

### User Story 2 - Platform engineers can tell whether the system is healthy right now (Priority: P2)

A platform engineer opens the System Health page during an incident. They see a single overall verdict plus a per-dependency breakdown: database, cache, background job queue, and object storage. Each dependency shows a healthy / degraded / unhealthy state, the evidence behind that state (queue backlog size, oldest waiting job age, probe latency, which storage backend is actually in use), and when the reading was taken. The page refreshes on its own so an engineer can leave it open while working.

**Why this priority**: Directly satisfies "health indicators are visible" and "queue health / storage health". Depends on nothing from Story 1 and can ship on its own.

**Independent Test**: Seed a backlog of unprocessed queue events and stop a dependency, then confirm the page shows the expected degraded/unhealthy states with the numeric evidence, and returns to healthy after the backlog clears.

**Acceptance Scenarios**:

1. **Given** every dependency is reachable and the queue is empty, **When** a platform engineer opens System Health, **Then** the overall verdict is healthy and each dependency shows healthy with its measured probe latency and a reading timestamp.
2. **Given** 150 queue jobs are waiting and the oldest has waited 12 minutes, **When** the page loads, **Then** the queue shows degraded and displays both the backlog count and the oldest-wait age as the reason.
3. **Given** 1,200 queue jobs are waiting, **When** the page loads, **Then** the queue shows unhealthy and the overall verdict is unhealthy.
4. **Given** object storage is configured for external storage but the system fell back to in-process storage at startup, **When** the page loads, **Then** storage shows unhealthy and names the fallback explicitly rather than reporting healthy.
5. **Given** the database is unreachable, **When** the page loads, **Then** the database shows unhealthy with the failure reason, and the page itself still renders the other dependencies rather than failing entirely.
6. **Given** the page is open, **When** 30 seconds pass, **Then** the readings refresh automatically and the reading timestamp updates.

---

### User Story 3 - Platform engineers can track AI provider errors (Priority: P3)

A platform engineer opens the AI Provider Status page to answer "is this provider having a bad day, or is it just us?". Per provider, they see call volume, success/failure counts, failure rate, typical and worst-case latency, and a breakdown of failures by category (authentication, rate limited, unavailable, timeout, invalid request) over a selectable window. Each provider carries a healthy / degraded / unhealthy verdict derived from its failure rate. A recent-failures list shows individual failed calls with their provider, model, category, latency, and time.

**Why this priority**: Satisfies "provider errors are trackable". Valuable but the least time-critical of the three, and it reuses the same aggregate data source as Story 1's AI usage figures.

**Independent Test**: Seed AI call records with mixed success and failure categories across providers, then confirm per-provider rates, category breakdown, verdicts, and the recent-failures list are correct for the selected window.

**Acceptance Scenarios**:

1. **Given** 100 calls to one provider in the window of which 25 failed, **When** the page loads, **Then** that provider shows a 25% failure rate and an unhealthy verdict.
2. **Given** 100 calls to a provider of which 4 failed, **When** the page loads, **Then** that provider shows a 4% failure rate and a degraded verdict.
3. **Given** failures across several categories, **When** the page loads, **Then** the failure counts are broken down by category and sum to the provider's total failure count.
4. **Given** a provider had only 3 calls in the window, **When** the page loads, **Then** it is reported as having insufficient data rather than being labelled unhealthy off a tiny sample.
5. **Given** a provider had zero calls in the window, **When** the page loads, **Then** it is listed as having no activity rather than being omitted or shown as failing.
6. **Given** failed calls exist, **When** the engineer views the recent-failures list, **Then** each entry shows provider, model, error category, latency, and timestamp, most recent first.

---

### Edge Cases

- **No data at all** (fresh install, zero tenants, zero AI calls): every surface renders zero-state figures and an explicit "no data yet" message — never a blank card, an error, or a rate computed from a zero denominator.
- **Selected date range is invalid** (end before start, non-date text, range longer than the maximum): the request is rejected with a clear message and the previously shown data stays on screen.
- **A single data source fails** while others succeed (e.g. the queue query errors but the database probe succeeds): the failing panel shows an error state; the rest of the page still renders.
- **A health probe hangs**: the probe is abandoned after its timeout and reported as unhealthy with a timeout reason, rather than hanging the whole page.
- **Platform user with partial permission** (may see business analytics but not infrastructure diagnostics): the overview loads, while the health and provider pages are not reachable and are absent from navigation.
- **Tenant user reaches a platform URL directly** by typing it: they are denied and redirected, and the underlying data request is refused by the server regardless of what the client does.
- **Very large data volumes**: aggregate queries stay within the performance budget in Success Criteria; the recent-failures list is capped rather than unbounded.
- **Clock/timezone**: all periods and timestamps are expressed in UTC so two platform users in different regions see identical figures for the same range.

## Requirements *(mandatory)*

### Functional Requirements

#### Access control

- **FR-001**: The system MUST expose global business analytics (tenant counts, conversation counts, AI usage totals) only to authenticated platform users holding a new `platform.analytics.view` permission, and MUST grant that permission to all five platform roles (Super Admin, Developer, Support, Sales, Finance).
- **FR-002**: The system MUST expose infrastructure diagnostics (system health, queue health, storage health, AI provider status) only to platform users holding the existing `platform.diagnostics.view` permission, whose grant MUST remain unchanged at Super Admin and Developer only.
- **FR-003**: The system MUST reject every platform analytics and diagnostics request from a user without a platform role, enforced on the server independently of any client-side navigation or route guard.
- **FR-004**: The system MUST hide platform analytics and diagnostics navigation entries from users lacking the corresponding permission, and MUST block direct URL access to those pages for such users.
- **FR-005**: Platform analytics and diagnostics results MUST aggregate across all tenants and MUST NOT be narrowed by whichever tenant the platform user has currently switched into.

#### Global business metrics (Platform Overview)

- **FR-006**: The system MUST report the total number of tenants, split by active and suspended, excluding soft-deleted tenants.
- **FR-007**: The system MUST report the number of currently active conversations platform-wide, where "active" means conversation status is open or pending.
- **FR-008**: The system MUST report, for a caller-selected UTC date range, platform-wide AI usage: total calls, successful calls, failed calls, total input tokens, and total output tokens.
- **FR-009**: The system MUST default the analytics date range to the last 30 days when the caller supplies none, and MUST reject ranges where the start is after the end or the span exceeds 365 days.
- **FR-010**: The system MUST report the number of tenants created within the selected range, so platform staff can see growth and not only a standing total.
- **FR-011**: The Platform Overview MUST NOT embed health data. Users holding the diagnostics permission reach health information through a navigation link to the System Health page; that link follows the same visibility rule as every other diagnostics entry point (FR-004).

#### System, queue, and storage health

- **FR-012**: The system MUST report an overall health verdict of healthy, degraded, or unhealthy, computed as the worst state among all monitored dependencies.
- **FR-013**: The system MUST report per-dependency health for database, cache, background job queue, and object storage, each with a state, a human-readable reason when not healthy, the measured probe duration, and the UTC timestamp of the reading.
- **FR-014**: Queue health MUST be derived from the backlog of unprocessed background events using these default thresholds: healthy when fewer than 100 events are waiting AND the oldest has waited under 5 minutes; degraded when 100–999 events are waiting OR the oldest has waited 5–30 minutes; unhealthy when 1,000 or more events are waiting OR the oldest has waited more than 30 minutes.
- **FR-015**: Queue health MUST additionally report the count of events that have exceeded 5 delivery attempts, and MUST report at least degraded whenever that count is non-zero, so retry storms are visible even when the backlog is small.
- **FR-016**: Storage health MUST report which storage backend is actually in use, and MUST report unhealthy when the system is running on the in-process fallback backend while external object storage was configured — silent fallback MUST NOT be reported as healthy.
- **FR-017**: Storage health MUST verify reachability with a read-only round-trip probe (a metadata or listing operation) rather than inferring health from configuration alone. The probe MUST NOT create, overwrite, or delete any object, so it is safe to repeat every 30 seconds and leaves nothing behind in the bucket.
- **FR-017a**: Because the probe is read-only, a bucket that is readable but not writable MUST be reported as healthy. Write-path failures surface through the operations that actually write, not through this indicator.
- **FR-018**: Each dependency probe MUST be abandoned after a bounded timeout and reported as unhealthy with a timeout reason; one slow probe MUST NOT prevent the other dependencies from being reported.
- **FR-019**: The System Health page MUST refresh its readings automatically every 30 seconds while open and MUST offer a manual refresh, always displaying the UTC time of the reading on screen.
- **FR-019a**: The three-state health surface MUST be delivered as a permission-gated platform surface, distinct from the system's existing unauthenticated liveness, readiness, and metrics probes. Those existing probes MUST keep their current behaviour and response contract, so orchestrator health checking is unaffected by this feature.
- **FR-019b**: The detail introduced by this feature — queue backlog size, oldest-wait age, stuck-retry count, storage backend identity, and probe durations — MUST be reachable only through the permission-gated surface and MUST NOT be added to the existing public probes. (Those probes already expose a per-check failure reason; this feature neither widens nor narrows that, per FR-019a.)

#### AI provider status and error tracking

- **FR-020**: The system MUST report, per AI provider, for a caller-selected window: total calls, successes, failures, failure rate, median latency, 95th-percentile latency, and total tokens.
- **FR-021**: The system MUST break failures down by error category (authentication, rate limited, unavailable, timeout, invalid request) per provider.
- **FR-022**: The system MUST assign each provider a verdict from its failure rate using these default thresholds: healthy below 2%, degraded from 2% up to 10%, unhealthy at 10% or above.
- **FR-023**: The system MUST report "insufficient data" instead of a verdict when a provider had fewer than 20 calls in the window, and MUST report "no activity" when it had zero calls, rather than omitting the provider.
- **FR-024**: The system MUST list recent failed AI calls with provider, model, error category, latency, and UTC timestamp, most recent first, capped at 50 entries.
- **FR-025**: The provider status window MUST default to the last 24 hours and MUST offer at least 1-hour, 24-hour, and 7-day windows.
- **FR-026**: Failure detail displayed to platform users MUST NOT include provider credentials, API keys, or raw customer message content.

#### Presentation and reliability

- **FR-027**: Every health and provider verdict MUST be distinguishable without relying on colour alone (a label or icon in addition to colour), so the indicators remain readable for colour-blind users.
- **FR-028**: Each page MUST render partial results when one of its data sources fails, showing an error state scoped to the affected panel rather than replacing the whole page with an error.
- **FR-029**: All figures, ranges, and timestamps MUST be expressed in UTC and labelled as such.
- **FR-030**: Reading platform analytics and health data MUST NOT modify any business data; these are read-only surfaces.
- **FR-031**: Every figure MUST be computed from live data at the moment of the request. The system MUST NOT serve cached or precomputed aggregates, so a platform user never sees a stale number without knowing it.

### Key Entities

- **Platform Usage Summary**: A point-in-time, all-tenant rollup for a selected UTC date range — tenant totals by status, tenants created in range, active conversation count, and AI call and token totals with success/failure split. Derived from existing tenant, conversation, and AI usage records; introduces no new stored data.
- **Health Reading**: A point-in-time snapshot containing an overall verdict and a set of dependency results. Not persisted; produced on request.
- **Dependency Result**: One monitored dependency (database, cache, queue, storage) with its state, reason, probe duration, and dependency-specific evidence (queue backlog size, oldest wait age, stuck-retry count, active storage backend).
- **Provider Status**: Per-AI-provider aggregate over a selected window — call counts, failure rate, latency percentiles, token totals, failure counts by category, and a derived verdict. Derived from existing AI usage records.
- **Provider Failure Entry**: A single failed AI call surfaced for troubleshooting — provider, model, error category, latency, timestamp. A read-only projection of an existing record.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A platform user can determine total tenant count, platform-wide active conversations, and AI usage for a chosen period within 15 seconds of signing in, without switching tenant context.
- **SC-002**: 100% of platform analytics and diagnostics requests made by users without a platform role are refused, verified by automated tests covering every tenant role.
- **SC-003**: Each of the three pages presents its first meaningful figures within 2 seconds on a dataset of 1,000 tenants and 1,000,000 AI usage records.
- **SC-004**: A health reading requested while a dependency is down reports that dependency as unhealthy in that same response, and a reading requested after recovery reports it healthy — verified by inducing an outage and recovery. (Readings are computed per request; the system does not monitor continuously between requests.)
- **SC-005**: A platform engineer can identify which AI provider is failing and its dominant failure category in under 30 seconds from opening the AI Provider Status page.
- **SC-006**: Every health and provider verdict is identifiable from text or icon alone with colour removed, verified on all three pages.
- **SC-007**: When any single data source fails, the remaining panels on that page still render, verified for each panel on each page.
- **SC-008**: No credential, API key, or customer message content appears in any platform analytics or health response, verified by automated tests over every response shape.

## Assumptions

- **Reuse over new storage**: All business metrics are computed on demand from existing tenant, conversation, and AI usage records. No rollup or snapshot tables are introduced, consistent with how tenant analytics was built in 025-analytics-foundation. If query cost later exceeds SC-003, rollups become a separate feature.
- **New indexes are in scope, and are the only performance lever**: existing indexes on the usage and conversation tables lead with `tenant_id`, but every query here is deliberately cross-tenant and filters on time alone. Meeting SC-003 requires new indexes supporting platform-wide, time-ranged aggregation; that work belongs to this feature, not a follow-up. Caching and rollups were both considered and rejected in favour of live queries (see Clarifications), so if indexing alone cannot hit the 2-second budget, that is a finding to escalate at plan time rather than something to silently paper over with a cache.
- **Permission model**: The existing `platform.diagnostics.view` permission — today wired only to a test-only route and its RBAC tests, with no production endpoint behind it — is activated for the health and provider pages (Super Admin and Developer only — that is its current grant). A new `platform.analytics.view` permission is introduced for the business-metrics overview and granted to all five platform roles, because the existing `platform.admin` permission is held by Super Admin alone and would otherwise leave Sales, Support, and Finance unable to "monitor global usage." Splitting business aggregates from infrastructure internals is the intended security boundary.
- **Overview placeholder is replaced**: The existing Platform Overview placeholder page becomes the real overview dashboard. Its route path is retained rather than renamed, to avoid breaking existing navigation and breadcrumb expectations.
- **Health surface is three-state and separate**: The existing readiness probe contract is binary (ready / not ready) and is consumed by orchestrators. This feature layers healthy / degraded / unhealthy on top of it behind a new permission-gated surface, leaving the public probes unchanged (see Clarifications). Thresholds in FR-014, FR-015, and FR-022 are stated defaults chosen for a system of this size; making them operator-configurable is out of scope.
- **Storage fallback is a real gap**: Today, external object storage failing at startup falls back to in-process storage with only a log warning. FR-016 treats that state as unhealthy, which requires the running system to expose which backend it actually selected.
- **Queue means the existing outbox**: "Queue health" refers to the existing background event outbox (unprocessed events, their age, and their attempt counts). No external queue system is assumed.
- **Cache is monitored via the existing readiness check** and reported as binary healthy/unhealthy plus probe latency; no degraded band is defined for it.
- **No alerting**: This feature makes health observable in the dashboard. Paging, email/chat alerts, on-call routing, and historical health time series are explicitly out of scope.
- **No per-tenant drill-down**: Aggregates are platform-wide. Ranking or filtering metrics by individual tenant is out of scope; tenant-level analytics already exists in the tenant area.
- **No cost or currency figures**: AI usage is reported in calls and tokens. Monetary cost attribution belongs to the billing feature (031), not here.
- **UTC everywhere**: Per-user timezone display is out of scope.
- **Adding a permission has known blast radius**: introducing a new permission touches the exact-inventory permission, route, and navigation tests that enumerate the full set; those updates are expected work, not regressions.
