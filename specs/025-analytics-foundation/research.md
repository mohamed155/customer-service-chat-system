# Research: Analytics Foundation

All findings below were verified against the actual schema (migrations 0026–0051) and module source — not assumed from prior specs' plans.

## R1. Live aggregation queries vs. rollup/aggregation tables

- **Decision**: Live SQL aggregation at request time. No new aggregation tables. Migration 0052 adds indexes only.
- **Rationale**: The clarified freshness requirement (≤1 min lag) makes daily rollups insufficient on their own, and dual-write/refresh machinery (triggers, scheduled jobs, backfill) is the most expensive part of an analytics stack to maintain. At the foundation's stated scale bound (100k conversations / 90 days / tenant), Postgres set-based aggregates over indexed, date-bounded, tenant-scoped ranges comfortably meet the 3 s render / 2 s filter targets. Every source table already carries `(tenant_id, created_at…)`-friendly indexes or gets one in 0052.
- **Alternatives considered**: (a) Daily rollup table + live "today" overlay — deferred until a real tenant exceeds live-query budgets; the endpoint contract is deliberately shaped so a rollup can be swapped in behind it without API changes. (b) Materialized views — refresh cadence violates the 1-min freshness clarification and complicates migrations.

## R2. Source-of-truth definitions per metric (verified against schema)

- **Decision**:
  - **Conversation volume**: `COUNT(*)` of `conversations` with `created_at` in range, `deleted_at IS NULL` (attribution by creation date per spec).
  - **Concluded**: `status IN ('resolved','closed')` (status vocabulary from migration 0033: open/pending/resolved/closed).
  - **Handoff**: an `escalations` row exists for the conversation (`EXISTS` on `escalations(tenant_id, conversation_id)`). Escalation rows persist after closing, so "ever escalated" is queryable; `conversations.escalated_at` is NOT usable (0037 clears it when the escalation closes).
  - **AI-resolved**: concluded AND no `escalations` row ever (per clarification).
  - **Rates**: denominators = concluded conversations created in range (resolution rate) / all conversations created in range (handoff rate). NULL (rendered as no-data) when denominator is 0.
  - **First response time**: per conversation, first `messages.kind = 'customer'` row → first later row with `kind IN ('reply','ai')`; averaged over conversations having both. `system` and `note` kinds do not count as responses (kinds verified in 0034/0042).
  - **All-pairs response time** (secondary): every `customer` message → the **first** `reply`/`ai` message after it in the same conversation, via a `CROSS JOIN LATERAL (SELECT MIN(created_at) …)` over messages of in-range conversations. A `LEAD()` window function was rejected: it pairs a customer message only with the literally next message, so two consecutive customer messages before one reply would drop the first pair.
  - **Satisfaction**: `AVG(rating)` and `COUNT(*)` from `conversation_feedback` with `submitted_at` in range (attribution by submission date per spec; table is append-only, rating 1–5, verified in 0051).
  - **Token usage**: `SUM(COALESCE(input_tokens,0) + COALESCE(output_tokens,0))` from `ai_usage_records` with `created_at` in range. **All rows count regardless of `status`** — a failed provider call can still have consumed input tokens, and the metric answers "what did we spend". Token columns are nullable, so both are `COALESCE`d to 0.
- **Rationale**: Each definition maps to a column/constraint that exists today; no write-path changes required for any metric.
- **Alternatives considered**: Using `conversations.escalated_at` for handoff — rejected (cleared on close, would undercount).

## R3. Channel attribution for token usage

- **Decision**: Join `ai_usage_records → ai_generations (usage_record_id) → conversations` to attribute usage to a channel. When a channel filter is active, token usage includes only attributable records; the all-channels view sums all records (attributed + unattributed) and the contract exposes an `unattributed_tokens` figure so numbers reconcile.
- **Rationale**: `ai_usage_records` has NO conversation linkage (verified: no such column in 0040, none in `UsageWrite`), but `ai_generations` (0048) carries both `conversation_id` and `usage_record_id` with an existing conversation index. This satisfies FR-010 (channel filter composes) without touching the ~10 `UsageWrite` construction sites in `ai/src/service.rs`. Non-generation usage (connection tests, summaries) legitimately has no channel.
- **Alternatives considered**: Adding `conversation_id` to `ai_usage_records` + plumbing all write sites — more invasive, still leaves historical rows NULL, and duplicates linkage `ai_generations` already provides. Rejected for the foundation.

## R4. RBAC alignment (matrix change required)

- **Decision**: Keep guarding with existing `Permission::AnalyticsView`; remove `AnalyticsView` from `TENANT_VIEWER` in `authz/src/matrix.rs`. `TENANT_AGENT` already lacks it. Owner (full TENANT set), Admin, and Manager keep it. Platform staff production roles (`STAFF_PRODUCTION_*`) retain it (platform users acting via tenant switcher, per spec assumption). Frontend nav/route visibility follows the permission from `/me` as elsewhere.
- **Rationale**: Clarification Q4 says Viewer must not see analytics; the current matrix grants it. Single-line matrix change + updated RBAC tests, no new permission needed.
- **Alternatives considered**: New `analytics.view_restricted` permission — pointless duplication.

## R5. API shape

- **Decision**: Two GET endpoints under `/api/v1/tenant/analytics/`:
  - `GET /tenant/analytics/summary?from&to&channel` → headline metrics + channel breakdown (single round-trip for all cards).
  - `GET /tenant/analytics/timeseries?from&to&channel` → all daily series (volume, resolved-vs-handoff, satisfaction, tokens), zero-filled per day, in one response.
  Both guarded by `analytics.view`, standard error envelope, no pagination (responses bounded: ≤ ~366 buckets enforced by a max-range validation).
- **Rationale**: The dashboard renders everything at once; one request per section (cards / charts) keeps payloads small, contracts cohesive, and lets a future rollup implementation replace internals per endpoint. Matches existing REST conventions (`/tenant/...`, utoipa-documented, cursorless bounded responses).
- **Alternatives considered**: One mega-endpoint (harder to evolve, mixes cadences); per-metric endpoints (chatty: 6+ round-trips).

## R6. Date/channel filter semantics

- **Decision**: `from`/`to` are inclusive UTC dates (`YYYY-MM-DD`); day buckets are UTC calendar days (per spec assumption). Default range: last 30 days. Max custom range: 366 days (validated, 422 on violation). `channel` accepts the DB vocabulary (`widget`, `email`, `phone`, `web_chat`, `whatsapp`, `telegram` — CHECK constraint verified in 0051); omitted = all channels. Frontend presets: 7 / 30 / 90 days + custom range.
- **Rationale**: Matches clarified UTC decision; bounded ranges protect the live-query budget; channel vocabulary comes from the existing CHECK constraint rather than inventing a parallel enum.
- **Alternatives considered**: Tenant-local timezones — explicitly deferred by spec.

## R7. Frontend architecture

- **Decision**: Follow the `conversations` feature pattern: `analytics-api.service.ts` (typed wire models + `fromWire` mappers in `core/api/tenant-api.models.ts`, RxJS all the way), `analytics.store.ts` NgRx SignalStore holding filters (dateRange preset/custom, channel) + summary/timeseries state + loading/error, and a rewritten `analytics.component.ts` that drops `RoutedPageStore`/fixtures. Reuse `metric-card`, `sparkline`, `dashboard-card`, `toolbar`, `empty-state`, `loading-state`, `select-filter`, `channel-badge`. Charts remain hand-built inline SVG (003 rule: no chart library); if the existing single-series `sparkline` is insufficient (e.g., resolved-vs-handoff needs two series), add a small reusable chart component under `shared/components/` rather than page-local markup.
- **Rationale**: The analytics page already exists as a fixture-driven Helix page with the exact layout the spec asks for (toolbar filters, metric grid, chart cards) — this feature swaps its data source, not its design. Store/service split matches every wired tenant feature.
- **Alternatives considered**: Introducing a chart library — violates the 003 convention and the 97 KB-conscious design culture; unnecessary for line/bar series of ≤366 points.

## R8. Indexes (migration 0052)

- **Decision**: Add exactly:
  1. `conversations (tenant_id, created_at) WHERE deleted_at IS NULL` — drives every conversation-cohort scan (existing indexes lead on status/customer/activity, none on created_at).
  2. `ai_generations (tenant_id, usage_record_id)` — reverse join for R3 (existing index is `(tenant_id, conversation_id, created_at)`).
  Existing indexes already cover the rest: `ai_usage_records (tenant_id, created_at DESC)` (0040), `conversation_feedback (tenant_id, submitted_at DESC)` (0051), `messages (tenant_id, conversation_id, created_at DESC, seq DESC)` (0034), `escalations (tenant_id, conversation_id)` unique partial (0037).
- **Rationale**: Principle VIII mandates indexes for production query paths; these two are the only gaps for the query plans in data-model.md.
- **Alternatives considered**: `messages (tenant_id, created_at)` — not needed; message scans always enter via conversation_id through the timeline index.
