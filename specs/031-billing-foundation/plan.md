# Implementation Plan: Billing Foundation

**Branch**: `031-billing-foundation` | **Date**: 2026-07-24 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/031-billing-foundation/spec.md`

## Summary

Activate the placeholder `billing` crate into a real module: a platform-global plan catalog (`billing_plans`, seeded with a default Free plan: 100k tokens / 100 conversations / $0) and one-row-per-tenant `tenant_subscriptions` (active/past_due/canceled). Usage is never stored twice — token consumption is aggregated live from the existing `ai_usage_records` table and conversation counts from `conversations`, windowed to the UTC calendar month (the analytics-module pattern). Enforcement is a real-time gate added to the single AI responder worker (`ai::agent_responder`) that all channels share: exhausted allowance or past_due/canceled status consumes the outbox event without generating a reply, leaving inbound messages and human agents untouched. The gate runs **last**, after escalation-rule evaluation, so an over-limit tenant still auto-escalates to a human queue instead of going silent (SC-006). Surface: `GET /tenant/billing/overview` plus platform plan/subscription CRUD (audited via `tenancy::audit::record_in_tx`), a tenant Billing page (plan card, usage cards with 80%/100% states, AI-paused banner), and a platform Billing page (plan catalog + tenant subscription assignment). RBAC largely pre-exists; the deltas are adding `billing.view` to tenant Admin and introducing `platform.billing.manage` (Super Admin + Finance).

## Technical Context

**Language/Version**: Backend Rust (workspace toolchain, Axum/Tokio); Frontend TypeScript / Angular 22 (standalone components, signals)

**Primary Dependencies**: Axum, SQLx, utoipa (OpenAPI), Serde, Tracing; Angular 22, Taiga UI (wrapped), NgRx SignalStore — all already in the workspace; no new dependencies

**Storage**: PostgreSQL — one new migration `0059_billing_foundation.sql` (two tables + seed); reads existing `ai_usage_records`, `conversations`, `tenants`, `audit_logs`

**Testing**: `cargo test -p billing`, `-p ai`, `-p authz` (DB-backed, narrow suites — workspace-wide runs skip DB tests); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Existing modular-monolith server (Linux/macOS) + browser dashboard

**Project Type**: Web application (Rust backend + Angular frontend monorepo)

**Performance Goals**: Enforcement gate adds one indexed aggregate round trip (< 10 ms typical) to an AI-reply path measured in seconds; `GET /tenant/billing/overview` < 500 ms p95; display lag effectively zero (live queries — satisfies SC-003's 5-minute bound)

**Constraints**: No rollup tables, no background workers/cron (period reset and AI resume are derived from the query window per FR-006); enforcement must be real-time per SC-002; no crate cycle — `ai` → `billing` only, `billing` reads other modules' tables via SQL exclusively

**Scale/Scope**: Foundation scale — existing tenant count; `ai_usage_records` month-range sums ride the existing `(tenant_id, created_at DESC)` index; 6 HTTP endpoints, 2 dashboard pages, ~1 matrix + 1 new permission RBAC delta

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Modular Monolith | **PASS with recorded deviation** | `billing` is an isolated crate and the only cross-module *call* is `ai` → `billing::enforcement::check` (public interface). But billing reads `ai_usage_records`, `conversations`, and `tenants` with raw SQL, and Principle I says modules communicate "never through direct cross-module data access". That is a real deviation, not a non-issue — it is justified and bounded in Complexity Tracking below rather than waved through. |
| II | Multi-Tenant Isolation | PASS | `tenant_subscriptions` carries `tenant_id` (unique); every tenant query is tenant-filtered via `TenantContext`. `billing_plans` is platform-owned reference data (no `tenant_id` by design, like `tenants`). Platform endpoints are platform-role gated, not tenant-scoped. |
| III | Zero-Trust & RBAC | PASS | Every endpoint behind `require_permission`; sensitive changes (plan/subscription) audited with actor + before/after (FR-009). No secrets involved. |
| IV | AI Provider Independence | PASS | No LLM/provider surface touched; the gate sits in the responder worker before generation and is provider-agnostic. |
| V | API-First | PASS | REST endpoints with utoipa/OpenAPI registration, standard error envelope; subscription assignment is an idempotent PUT upsert. |
| VI | Observability | PASS | Existing request-id/tracing middleware covers new routes; gate skips log via `tracing` like sibling gates. |
| VII | Test-First | PASS | DB-backed tests per crate (enforcement matrix, RBAC 403s, audit rows); frontend store/component specs; quickstart defines E2E validation. Task IDs list implementation before its test so each test task can name the exact symbols it targets; tasks.md Convention 11 fixes the required working order as test-first and enumerates the pairs. |
| VIII | DB Integrity & Migrations | PASS | Single migration, normalized (usage derived, not duplicated), FKs RESTRICT, partial unique for single default plan, existing indexes serve query paths; no N+1 (single-query aggregates and joins). |
| IX | Design System | PASS | Reuses `--app-*` tokens, wrapped Taiga components, analytics metric-card visual pattern; no new UI primitives outside `shared/`/feature folders. |
| X | Performance | PASS | Live indexed aggregates; one extra query per AI reply; no polling, no rollups to drift. |

**Post-Phase-1 re-check**: PASS — one deviation (Principle I, cross-module table reads) is recorded in Complexity Tracking; no others introduced by the design artifacts.

## Project Structure

### Documentation (this feature)

```text
specs/031-billing-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 — decisions R1–R8
├── data-model.md        # Phase 1 — tables, derived data, transitions, audit actions
├── quickstart.md        # Phase 1 — end-to-end validation guide
├── contracts/
│   └── billing-api.md   # Phase 1 — HTTP + crate-boundary contracts
└── tasks.md             # Phase 2 (/speckit-tasks — not created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0059_billing_foundation.sql        # billing_plans + tenant_subscriptions + Free seed
└── crates/
    ├── modules/billing/                   # placeholder → real module
    │   ├── src/lib.rs                     # module docs (Purpose/Responsibilities/…), pub mods
    │   ├── src/model.rs                   # DTOs (PlanDto, BillingOverviewDto, …)
    │   ├── src/queries.rs                 # plan/subscription CRUD + usage aggregation
    │   ├── src/enforcement.rs             # EnforcementDecision + check(pool, tenant_id)
    │   ├── src/audit.rs                   # billing_plan.* / billing_subscription.* writers
    │   ├── src/routes.rs                  # 6 utoipa-annotated handlers
    │   └── tests/                         # usage math (pure) + DB-backed queries & enforcement
    │                                      # (route/RBAC/audit tests live in server/tests/)
    ├── modules/authz/src/
    │   ├── permission.rs                  # + PlatformBillingManage variant
    │   └── matrix.rs                      # + BillingView→TENANT_ADMIN; manage→PLATFORM_ALL/FINANCE
    │                                      #   + fix owner-exclusives test in same file
    ├── modules/ai/src/agent_responder.rs  # + billing gate, last gate before generation
    ├── server/src/
    │   ├── router.rs                      # route registration + permission layers + probe route
    │   └── openapi.rs                     # register billing DTOs
    └── server/tests/                      # billing_overview, billing_platform,
                                           # billing_responder_gate + edits to
                                           # auth.rs, rbac.rs, openapi_coverage.rs

frontend/apps/dashboard/src/app/
├── core/
│   ├── router/                            # APP_PATHS.tenant.billing / APP_PATHS.platform.billing
│   ├── authz/permissions.ts               # + 'platform.billing.manage'; PAGE_PERMISSIONS entries
│   └── api/tenant-api.models.ts           # billing wire types + fromWire mappers
├── layout/                                # sidebar entries (tenant Billing, platform Billing)
├── features/tenant/billing/               # billing-api.service / billing.store / billing-page
├── features/platform/billing/             # plans catalog + tenant subscription assignment
└── shared/fixtures/billing.fixtures.ts    # typed fixtures for specs
```

**Structure Decision**: Backend follows the activated-placeholder-module pattern proven by `audit` (026): model/queries/routes files inside the existing crate, wiring centralized in `server/src/{router,openapi}.rs`. Frontend follows the feature-folder idiom of notifications/audit-logs/integrations under `apps/dashboard/src/app/features/{tenant,platform}/` — no new lib. Full rationale in [research.md](research.md) R2–R7.

## Complexity Tracking

| Deviation | Principle | Why it is needed | Simpler alternative rejected because | Containment |
|-----------|-----------|------------------|--------------------------------------|-------------|
| `billing` reads `ai_usage_records`, `conversations`, and `tenants` with raw SQL instead of going through those modules' interfaces | I (Modular Monolith — "never through direct cross-module data access") | Usage must be derived live from the rows those modules already write. FR-003 forbids a second tracking mechanism, and SC-002 forbids a lagging cached counter, so billing has to aggregate at read time over tables it does not own. | (a) Depending on the `ai` and `conversations` crates for query functions creates the cycle `ai → billing → ai`, which does not compile. (b) A usage-event stream into a billing-owned rollup table is the extraction-safe design, but it needs double-write consistency, backfill, and reset logic — rejected for a foundation, and rejected already by 025/analytics, which ships this same live-aggregation pattern in production. | Reads are confined to `billing::queries` (four functions, no writes, tenant-filtered). The dependency arrow stays one-way: `ai → billing`, never the reverse. Extracting `billing` later means replacing those four SELECTs with a usage API or a read replica — not a rewrite of callers. |
