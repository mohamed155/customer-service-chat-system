# Implementation Plan: Platform Analytics & System Health

**Branch**: `032-platform-analytics-health` | **Date**: 2026-07-25 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/032-platform-analytics-health/spec.md`

## Summary

Activate the placeholder `platform` module crate as a read-only, cross-tenant monitoring surface: three permission-gated endpoints (global business summary, three-state system health, AI provider status) plus three dashboard pages. All business metrics are computed live from existing `tenants`, `conversations`, and `ai_usage_records` tables — no rollups, no cache — backed by one additive index migration. System health layers a healthy/degraded/unhealthy verdict over the existing `HealthCheck` trait objects, adding queue-backlog and storage-backend evidence, while leaving the public `/health`, `/ready`, `/metrics` probes untouched. Two supporting changes fall out of the clarifications and are prerequisites rather than cleanup: `ObjectStorage` gains a read-only `probe()` method, and the duplicated S3-or-fallback selection is centralized so the backend actually in use can be reported.

## Technical Context

**Language/Version**: Backend Rust (workspace, Axum 0.8, Tokio, SQLx, utoipa); Frontend Angular 22 (standalone components, signals, NgRx SignalStore), TypeScript

**Primary Dependencies**: Axum + utoipa (`routes!()` co-registration), SQLx/PostgreSQL, `aws_sdk_s3` (`HeadBucket` for the storage probe), serde; Angular + existing Helix shared components (`metric-card`, `trend-chart`, `data-table`, `status-badge`, `select-filter`, `empty-state`, `loading-state`, `dashboard-card`)

**Storage**: PostgreSQL — existing `ai_usage_records` (0040), `outbox_events` (0002/0021/0022), `tenants` (0004), `conversations` (0033). No new tables; one additive index migration `0060_platform_analytics_indexes.sql`.

**Testing**: `cargo test -p authz -p platform -p server` with `REQUIRE_DB_TESTS=1` and `DATABASE_URL` set (DB-gated suites skip vacuously otherwise); exact-inventory suites `rbac.rs`, `auth.rs`, `openapi_coverage.rs`, `matrix.rs`; `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Performance Goals**: All three pages under 2 s first-figures at 1,000 tenants / 1,000,000 usage records (SC-003); one SQL statement per metric group, no per-tenant or per-provider looping

**Constraints**: Read-only surface (FR-030); live queries only — caching and rollups explicitly rejected in clarification (FR-031); cross-tenant by construction and never narrowed by switched tenant context (FR-005); public probes' contract must not change (FR-019a)

**Scale/Scope**: 3 new endpoints + 1 new permission (reusing 1 existing) + 1 index migration + 1 shared-crate trait method; 3 dashboard pages + 1 new shared component

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Modular Monolith | PASS | Feature lives in the activated `platform` crate (today a 1-line placeholder). It reads aggregate counts from tables owned by `tenancy`, `conversations`, and `ai` — the same read-only coupling the existing `analytics` crate already has, and the reason it is *not* folded into `analytics` (whose contract is entirely tenant-scoped) is recorded in research R7. No writes to other modules' tables; no cross-module service calls added. |
| II. Multi-Tenant Isolation | PASS | Inverted but deliberate: these endpoints are *intentionally* cross-tenant (FR-005) and mount only under `platform_routes`, behind `platform_permission_middleware`. They receive no `X-Tenant-ID`. Tenant users are refused server-side (FR-003), verified by `rbac.rs` deny-by-default sweeps — not by hiding navigation. |
| III. Zero-Trust & RBAC | PASS | Every endpoint carries `require_permission`. New `platform.analytics.view` (all 5 platform roles); existing `platform.diagnostics.view` reused with its grant unchanged (Super Admin + Developer). Read-only surface, so no new sensitive operation requiring audit. FR-026/SC-008 forbid credential and customer-content leakage in responses. |
| IV. AI Provider Independence | PASS (n/a) | Reads the `ai_usage_records` audit trail; performs no LLM interaction and adds no provider-specific logic beyond the three-value `provider` column the CHECK constraint already fixes. |
| V. API-First & Contracts | PASS | Three utoipa-documented GETs with the standard error envelope, registered via `routes!()` so `openapi_coverage.rs` sees them. GETs are idempotent by construction. Contract in `contracts/platform-analytics-api.md`. |
| VI. Observability | PASS | This feature *is* an observability surface. Handlers use `tracing::error!` on query failure, matching `analytics`. Reuses existing `HealthCheck` probes rather than duplicating them. |
| VII. Test-First & Regression | PASS | DB-gated integration tests per endpoint (thresholds, verdict ladder, zero-state, leak check), RBAC matrix + deny-by-default rows, OpenAPI coverage entries, frontend store/component specs. Threshold boundaries (99/100, 999/1000, 19/20 calls, 2%/10%) are the specific regression targets. |
| VIII. DB Integrity & Migrations | PASS | One additive migration `0060` — four indexes, no schema mutation, no data migration. Indexes are mandatory here, not optional: every query is cross-tenant and time-filtered while existing indexes lead with `tenant_id`. |
| IX. Design System Discipline | PASS | Three pages composed from existing shared components; exactly one new shared component (`health-indicator`), justified because the three-state verdict renders on all three pages. Icon + text label, never colour alone (FR-027). |
| X. Performance & Efficiency | PASS **with a tracked risk** | Single-pass `FILTER` aggregates; no N+1. The live-query decision (clarification Q2) removed caching and rollups, making index design the only lever. `percentile_cont` (FR-020) sorts rather than streams — bounded by the ≤7-day window, but this is the one place SC-003 could fail. See Complexity Tracking. |

**Post-Phase-1 re-check**: PASS. Design artifacts introduce two deviations from a strictly minimal footprint (shared-crate trait change; centralizing duplicated startup logic), both recorded in Complexity Tracking as requirement-driven rather than discretionary.

## Project Structure

### Documentation (this feature)

```text
specs/032-platform-analytics-health/
├── plan.md                              # This file
├── research.md                          # Phase 0 output
├── data-model.md                        # Phase 1 output
├── quickstart.md                        # Phase 1 output
├── contracts/
│   └── platform-analytics-api.md        # Phase 1 output
├── checklists/
│   └── requirements.md                  # from /speckit-specify + /speckit-clarify
└── tasks.md                             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0060_platform_analytics_indexes.sql   # NEW: 4 additive indexes (next-after-max; 0055/0058 absent)
└── crates/
    ├── shared/storage/src/lib.rs             # ObjectStorage::probe(); centralized backend selection
    │                                         #   + StorageBackend { kind, s3_configured }
    ├── modules/
    │   ├── platform/src/                     # ACTIVATE placeholder crate
    │   │   ├── lib.rs                        # module wiring + docs (Purpose/Responsibilities/...)
    │   │   ├── model.rs                      # NEW: DTOs, verdict enums + ladders, query params
    │   │   ├── queries.rs                    # NEW: Q1–Q6 aggregate SQL
    │   │   ├── health.rs                     # NEW: dependency probes, threshold → verdict mapping
    │   │   └── routes.rs                     # NEW: 3 handlers
    │   └── authz/src/
    │       ├── permission.rs                 # + PlatformAnalyticsView (variant, rename, as_str, ALL)
    │       └── matrix.rs                     # + grant to all 5 PLATFORM_* arrays
    └── server/
        ├── src/
        │   ├── main.rs                       # use centralized storage selection
        │   ├── state.rs                      # (unchanged — deps travel via Extension)
        │   └── router.rs                     # mount 3 routes in platform_routes();
        │                                     #   + Extension(health_checks), Extension(storage_backend);
        │                                     #   de-duplicate the storage fallback block
        └── tests/
            ├── platform_analytics.rs         # NEW: summary endpoint + range validation
            ├── platform_health.rs            # NEW: thresholds, fallback, partial failure
            ├── platform_ai_providers.rs      # NEW: verdict ladder, categories, leak check
            ├── rbac.rs                       # + 3 paths in both inventories
            ├── auth.rs                       # + permission string
            └── openapi_coverage.rs           # + 3 EXPECTED entries

frontend/apps/dashboard/src/app/
├── core/
│   ├── authz/permissions.ts                  # + 'platform.analytics.view' | + PAGE_PERMISSIONS rows
│   └── router/app-paths.ts                   # + platform.systemHealth, platform.aiProviders
├── layout/topbar/platform-nav.component.ts   # + 2 destinations (permission-filtered)
├── shared/components/health-indicator/        # NEW: 3-state verdict, icon + label (FR-027)
└── features/platform/
    ├── platform.routes.ts                    # replace placeholder route; + 2 routes
    ├── overview/                              # REPLACES overview-placeholder/ (path retained)
    ├── system-health/
    └── ai-providers/
```

**Structure Decision**: Web application — Rust modular-monolith backend + Angular dashboard, matching the existing repository layout. The feature activates the reserved `platform` module crate rather than extending the tenant-scoped `analytics` crate (research R7), and reaches non-pool dependencies through the router's established `Extension` layers rather than changing `AppState`'s shape (research R3).

## Complexity Tracking

> Two deviations from a minimal footprint. Both are required by clarified requirements, not discretionary.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Adding `probe()` to the shared `ObjectStorage` trait (affects 3 implementors incl. 1 test double) | FR-017 as clarified requires a **read-only metadata/listing** probe. The trait is `put`/`get`/`delete` only — it cannot express one. | Calling `get()` on a reserved key and treating `NotFound` as success needs no trait change, but `S3Storage::get` detects `NotFound` by string-matching `"NoSuchKey"` in an SDK message no test pins, and S3 returns `403` (not `404`) for a missing key without `s3:ListBucket` — so a healthy deployment could report unhealthy. Health correctness would rest on an unpinned string. |
| Centralizing the S3-or-fallback selection currently duplicated in `main.rs:85-95` and `router.rs:998-1010` | FR-016 requires reporting the backend **actually** in use and flagging silent fallback. Today the choice exists only as a `tracing::warn!` — no value is produced, so it cannot be reported. | Reading config alone cannot detect fallback (that is the exact bug FR-016 targets). Recording the descriptor in only one of the two call sites would let the two entry points disagree about what is running. |

**Tracked risk (not a violation)**: SC-003's 2 s budget over 1,000,000 usage records with exact `percentile_cont` percentiles and no cache. Bounded by the ≤7-day window plus `ai_usage_records_created_idx`. If missed, the escalation levers are approximate percentiles or a narrower maximum window — **not** a cache, which clarification Q2 explicitly rejected. Verify with `EXPLAIN (ANALYZE, BUFFERS)` per quickstart before declaring SC-003 met.
