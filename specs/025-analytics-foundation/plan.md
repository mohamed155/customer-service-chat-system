# Implementation Plan: Analytics Foundation

**Branch**: `025-analytics-foundation` | **Date**: 2026-07-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/025-analytics-foundation/spec.md`

## Summary

Tenant-scoped analytics: headline metric cards (conversation volume, AI resolution rate, handoff rate, first response time, satisfaction, token usage), daily time-series charts, date-range and channel filters. Backend activates the placeholder `analytics` module crate with live SQL aggregation queries over existing tables (`conversations`, `messages`, `escalations`, `conversation_feedback`, `ai_usage_records`, `ai_generations`) — no rollup tables — exposed as two REST endpoints guarded by the existing `analytics.view` permission. Frontend replaces the fixture-driven `features/tenant/analytics` page with a SignalStore + API service, reusing existing `metric-card`, `sparkline`, and toolbar/filter components with hand-built inline SVG charts per the Helix convention.

## Technical Context

**Language/Version**: Backend Rust (workspace toolchain, Axum/Tokio/SQLx); frontend Angular 22 + TypeScript, standalone components, signals

**Primary Dependencies**: Backend: axum, sqlx (PostgreSQL), serde, utoipa, tracing. Frontend: @ngrx/signals SignalStore, RxJS, existing shared components (`metric-card`, `sparkline`, `dashboard-card`, `toolbar`, `empty-state`, `select-filter`, `channel-badge`). No chart library — inline SVG (003 convention).

**Storage**: PostgreSQL — read-only aggregation over existing tables; one new migration (0052) adding query indexes only. No new tables.

**Testing**: `cargo test` (server integration tests in `backend/crates/server/tests/analytics_api.rs`, module unit tests); `pnpm ng test dashboard` (Vitest/Karma per workspace), `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application (Rust modular-monolith backend + Angular dashboard frontend)

**Performance Goals**: Dashboard fully rendered ≤3 s for 100k conversations over 90 days (SC-004); filter changes re-render ≤2 s (SC-006); metrics lag live activity ≤1 min (clarified: near-real-time)

**Constraints**: Tenant isolation on every query (Principle II); UTC day buckets; zero-filled series; empty denominators render as explicit no-data states; soft-deleted conversations excluded everywhere

**Scale/Scope**: Foundation targets ≤100k conversations per tenant per 90-day window; 2 new endpoints; 1 rewired dashboard page; ~6 metrics + 4 chart series

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Modular Monolith | PASS | Activates the existing placeholder `analytics` crate; reads other modules' tables via its own queries (read-model pattern, same DB) — no cross-module Rust calls into private internals; route handlers live in the analytics module and are wired in `server/router.rs` like every other module |
| II. Multi-Tenant Isolation | PASS | Every query filters `tenant_id = $1`; endpoints sit under the existing tenant-context middleware; isolation integration test required (SC-002) |
| III. Zero-Trust & RBAC | PASS | Endpoints guarded by existing `Permission::AnalyticsView` via `require_permission`; RBAC matrix amended to match spec (remove `AnalyticsView` from `TENANT_VIEWER`) — read-only endpoints, no new audit events needed |
| IV. AI Provider Independence | PASS (n/a) | No LLM interaction; analytics only reads `ai_usage_records`/`ai_generations` |
| V. API-First | PASS | Two versioned REST endpoints under `/api/v1/tenant/analytics/*`, documented in utoipa/OpenAPI, standard error envelope, no pagination needed (bounded responses) |
| VI. Observability | PASS | Existing request-id/tracing middleware applies; queries run under `tracing` spans |
| VII. Test-First | PASS | All four required categories are task-backed: **unit** (`analytics::model` query-resolution tests), **integration/API** (`server/tests/analytics_api.rs` — seeded-data correctness, isolation, RBAC, date/channel filters, metric stability), **frontend unit** (store/component/chart specs), and **end-to-end** (`frontend/e2e/analytics.spec.ts`, matching the existing Playwright convention used by `escalation-routing`, `widget-chat`, `customer-profiles`). A performance check for SC-004 runs as an opt-in `#[ignore]` integration test |
| VIII. DB Integrity & Migrations | PASS | Index-only migration 0052 via the migration workflow; no manual schema changes; set-based aggregate SQL (no N+1) |
| IX. Design System | PASS | Reuses existing shared components; new chart pieces go to `shared/components/` if reusable; inline SVG charts per 003 |
| X. Performance | PASS | Single round-trip per endpoint, set-based SQL with covering indexes; date-bounded scans. SC-004's 3 s / 100k-conversation budget — the basis for choosing live aggregation over rollup tables — is verified by a dedicated bulk-seeded latency test rather than assumed |

**Post-Phase-1 re-check**: PASS — design introduces no new violations; Complexity Tracking stays empty.

## Project Structure

### Documentation (this feature)

```text
specs/025-analytics-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── analytics-api.md # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit-tasks - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0052_analytics_indexes.sql        # NEW: query indexes only
└── crates/
    ├── modules/
    │   ├── analytics/src/
    │   │   ├── lib.rs                     # activate placeholder crate
    │   │   ├── model.rs                   # NEW: response/read models (utoipa schemas)
    │   │   ├── queries.rs                 # NEW: aggregate SQL
    │   │   └── routes.rs                  # NEW: GET summary + timeseries handlers
    │   └── authz/src/matrix.rs            # EDIT: remove AnalyticsView from TENANT_VIEWER
    └── server/
        ├── src/router.rs                  # EDIT: wire analytics routes (require_permission)
        ├── src/openapi.rs                 # EDIT: register schemas/paths
        └── tests/analytics_api.rs         # NEW: integration tests

frontend/
├── e2e/
│   └── analytics.spec.ts                  # NEW: Playwright E2E (route-mocked, per repo convention)
└── apps/dashboard/src/app/
    ├── core/api/tenant-api.models.ts      # EDIT: analytics wire/domain models + mappers
    ├── design-system/tokens/tokens.css    # EDIT: validated chart series color tokens
    ├── shared/components/
    │   ├── metric-card/                   # EDIT: make delta/trend optional
    │   ├── trend-chart/                   # NEW: inline-SVG 1–2 series line chart
    │   └── breakdown-bars/                # NEW: horizontal channel-share bars
    └── features/tenant/analytics/
        ├── analytics.component.ts         # REWRITE: fixtures → store-driven
        ├── analytics.component.spec.ts    # REWRITE
        ├── analytics-api.service.ts       # NEW
        ├── analytics.store.ts             # NEW: SignalStore (filters, load, state)
        └── analytics.store.spec.ts        # NEW
```

**Structure Decision**: Web application layout already in place. Backend work activates `backend/crates/modules/analytics` (currently a placeholder crate) following the `feedback` module's file layout (`model.rs`/`queries.rs`/routes). Frontend work is confined to the existing `features/tenant/analytics` folder plus shared models, following the `conversations` feature's API-service + SignalStore pattern.

## Complexity Tracking

No constitution violations — table intentionally empty.
