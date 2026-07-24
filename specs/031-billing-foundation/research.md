# Research: Billing Foundation

**Feature**: 031-billing-foundation | **Date**: 2026-07-24

All decisions below were made against the actual codebase at branch `031-billing-foundation` (verified by reading source, not prior spec documents).

## R1. Enforcement-check freshness (deferred from /speckit-clarify)

- **Decision**: Real-time, per-event check. The AI responder worker evaluates a single month-to-date aggregate query (tokens + conversations + subscription status, one round trip) immediately before generating each AI reply. No cached counters, no rollup tables, no background reset job — the billing period is just the query window (`date_trunc('month', now() AT TIME ZONE 'utc')`), so "reset at period start" and "resume on upgrade" are automatic.
- **Rationale**: SC-002 demands zero over-quota AI replies beyond in-flight tolerance; a cached counter with 5-minute lag could leak many replies for a chatty tenant. The check is one indexed aggregate over `ai_usage_records (tenant_id, created_at DESC)` (index exists, added in 0040) plus a count over `conversations` — the same live-aggregation pattern spec 025 (analytics) already ships in production. AI-reply latency is seconds; +\<10 ms of SQL is immaterial. SC-003's 5-minute display lag is trivially satisfied because display reads the same live queries.
- **Alternatives considered**: (a) Maintained per-period counter table updated on every usage insert — rejected: double-write consistency burden, needs backfill and reset logic, contradicts 025's proven no-rollup precedent. (b) Periodic cached snapshot — rejected: violates SC-002.

## R2. Where the enforcement gate lives

- **Decision**: In `ai::agent_responder::process_agent_responder_once` (backend/crates/modules/ai/src/agent_responder.rs), as a new gate alongside the existing three (deleted outbox row + skip), placed **last inside the `if !is_platform_persona` block** — after the channel, conversation-state, and open-escalation gates, after message-body fetch, and after `match rule_match` — so it is the final check before `engine::run_generation`.
- **Ordering correction (post-analysis)**: an earlier draft put the gate immediately after the open-escalation gate. That is wrong. The escalation-rule evaluation sits *below* the message-body fetch, and its `Baseline`/`Tenant` arms are what route a conversation to a human queue. Gating above it would stop auto-escalation for exactly the tenants whose AI has just been paused — the customer would reach neither the AI nor an agent, contradicting spec US2 ("the conversation remains open for human handling") and SC-006 ("enforcement never blocks a customer from reaching the business"). Running the gate last also means the three enforcement queries execute only on events that would actually generate. The check itself is a public function in the `billing` crate (`billing::enforcement`), so `ai` depends on `billing`, never the reverse — `billing` reads `ai_usage_records` via SQL only, so no crate cycle (same discipline as the notifications/escalations boundary).
- **Rationale**: Every AI reply on every channel (widget, WhatsApp, Telegram, dashboard-visible conversations) flows through this single outbox→responder worker, so one gate covers FR-005 for all channels. The gate style (consume outbox event, return `Ok(true)`) is the established idiom at that exact spot.
- **Alternatives considered**: Gating at message intake per channel — rejected: N channel-specific gates, and it would block conversation intake, which FR-005 explicitly forbids.

## R3. Data model shape

- **Decision**: Two new tables in one migration (`0059_billing_foundation.sql`): `billing_plans` (platform-global catalog: name, monthly_price_cents, nullable allowances where NULL = unlimited, `is_default` with partial unique index, active/retired status) and `tenant_subscriptions` (one row per tenant — `tenant_id UNIQUE`, plan FK, status active/past_due/canceled). The migration seeds the default Free plan (100,000 tokens / 100 conversations / $0). No usage tables: token usage comes from existing `ai_usage_records` (sum of `input_tokens + output_tokens` over the period, regardless of success/failure status — whatever the provider metered, we count); conversation usage is `COUNT(*)` of `conversations` created in the period.
- **Rationale**: One-row-per-tenant subscription + append-only audit records satisfies FR-002/FR-009 without a subscription-history table (audit *is* the history in this foundation). `billing_plans` carries no `tenant_id` because it is platform-owned reference data, like `tenants` itself — Constitution Principle II applies to tenant-owned tables only; `tenant_subscriptions` is tenant-owned and has `tenant_id`.
- **Alternatives considered**: Subscription history table — rejected as premature; billing audit records capture before/after. Storing allowance snapshots on the subscription — rejected: FR-006/clarification say the *plan's current* allowances apply immediately on change.

## R4. Tenant-side RBAC (Admin visibility)

- **Decision**: Add `Permission::BillingView` to the `TENANT_ADMIN` array in `backend/crates/modules/authz/src/matrix.rs` (and mirror in the frontend permission fixtures/spec expectations). `billing.manage` stays Owner-only and is not used by any endpoint in this feature (reserved for future self-serve plan changes). `STAFF_PRODUCTION_FINANCE` already grants `BillingView` for platform staff working inside a tenant — unchanged.
- **Rationale**: The clarified spec (FR-004) requires Owner **and** Admin to view billing. Verified in source: today `TENANT_ADMIN` has no `billing.*` entries, so the matrix must change; the enum variants, serde names, frontend `Permission` union, and `/test/tenant/billing/*` RBAC probe routes all already exist, so this is a matrix-row change, not new permission plumbing.
- **Alternatives considered**: Keeping billing Owner-only — rejected: contradicts the clarified spec. Granting Admin `billing.manage` too — rejected: no tenant-side write operations exist in this feature.

## R5. Platform-side write permission

- **Decision**: Add a new `platform.billing.manage` permission (enum variant + serde name + `as_str` + catalog + frontend union), granted to `PLATFORM_ALL` (Super Admin) and `PLATFORM_FINANCE`. Reads use the existing `platform.billing.view`. Add a `/test/platform/billing/manage` RBAC probe route mirroring the existing probe pattern.
- **Rationale**: FR-008 requires write authority for plan catalog + subscription assignment; only `platform.billing.view` exists today (verified). Following the established view/manage pair convention (`integrations.view`/`integrations.manage`, etc.) keeps the matrix legible; Developer/Sales/Support platform roles get neither view nor manage, satisfying FR-008's denial case.
- **Alternatives considered**: Reusing `platform.billing.view` for writes — rejected: collapses the read/write distinction every other module maintains. Reusing `PlatformAdmin` — rejected: locks Finance out, contradicting spec role mapping.

## R6. API surface & audit actions

- **Decision**: Six endpoints, all registered in `server/src/router.rs` with `require_permission` layers and utoipa-annotated for the OpenAPI doc (DTOs added to `server/src/openapi.rs`):
  - `GET /tenant/billing/overview` (`billing.view`) — plan, price, subscription status, period window, per-metric usage (used/allowance/percent/state), `ai_paused` + reason.
  - `GET /platform/billing/plans` (`platform.billing.view`), `POST /platform/billing/plans` (`platform.billing.manage`), `PATCH /platform/billing/plans/{plan_id}` (`platform.billing.manage`, status active↔retired only).
  - `GET /platform/billing/subscriptions` (`platform.billing.view`) — all tenants joined with plan + status for the assignment UI.
  - `PUT /platform/billing/tenants/{tenant_id}/subscription` (`platform.billing.manage`) — idempotent upsert of `{plan_id, status}`.
  Audit actions written via `tenancy::audit::record_in_tx`: `billing_plan.created`, `billing_plan.status_changed`, `billing_subscription.changed` (details JSON carries before/after per FR-009).
- **Rationale**: Matches the module conventions verified in `analytics` and `audit` crates (routes + queries + model files, utoipa paths, permission layers in the central router). PUT-as-upsert makes subscription assignment idempotent per Constitution Principle V. Plan mutation is restricted to status changes because the spec only requires create + retire; editing allowances in place would silently change enforcement for subscribed tenants without an audit-visible plan identity change.
- **Alternatives considered**: Nesting subscription under `/platform/tenants/{id}` — rejected: billing module owns the resource; keeps the tenancy module untouched.

## R7. Frontend structure

- **Decision**: Follow the verified feature-folder idiom in `apps/dashboard/src/app/`:
  - Tenant: `features/tenant/billing/` — `billing-api.service.ts` (typed HTTP), `billing.store.ts` (NgRx SignalStore), `billing-page.component.ts` (plan card with name/price/status, period line, two usage cards with normal/warning/exceeded states and an AI-paused banner). Route `APP_PATHS.tenant.billing` guarded by `billing.view` via `PAGE_PERMISSIONS`; sidebar entry added.
  - Platform: `features/platform/billing/` — plan catalog table (create form + retire action, manage-gated) and tenant subscriptions table (plan/status assignment), route guarded by `platform.billing.view`.
  - Wire types + `fromWire` mappers in `core/api/tenant-api.models.ts`; fixtures in `shared/fixtures/billing.fixtures.ts`; usage-card visuals follow the analytics metric-card pattern (inline SVG/CSS, no chart library).
- **Rationale**: Identical layering to notifications/audit-logs/integrations features (verified in `frontend/CLAUDE.md` and the feature directories); `billing.view`/`platform.billing.view` already exist in the frontend `Permission` union so guards wire up without type changes.
- **Alternatives considered**: A `libs/feature-billing` library — rejected: recent features (audit, notifications, integrations, analytics) all live under `apps/dashboard/.../features/`, and consistency wins.

## R8. Period & edge-case semantics

- **Decision**: Period = UTC calendar month, computed in SQL at query time. Unlimited allowance = SQL NULL → usage card shows consumption only, never warning/exceeded, never contributes to enforcement. Exactly-at-limit (consumed ≥ allowance) counts as exhausted. Tenants with no subscription row resolve to the default plan via `LEFT JOIN` + fallback — no lazy row creation (avoids write-on-read). Enforcement treats `past_due` and `canceled` identically (clarified spec). In-flight overshoot is tolerated: the gate runs before generation starts.
- **Rationale**: Matches every clarified spec answer; write-on-read would complicate read paths and tests for zero benefit since the platform assignment flow creates rows explicitly.
- **Alternatives considered**: Backfilling subscription rows for all existing tenants in the migration — viable but rejected: the default-plan fallback must exist anyway for tenants created later, so backfill adds a second code path instead of removing one.
