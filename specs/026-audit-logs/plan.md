# Implementation Plan: Audit Logs

**Branch**: `026-audit-logs` | **Date**: 2026-07-19 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/026-audit-logs/spec.md`

## Summary

Expose the existing append-only `audit_logs` table (migration `0006_audit_logs.sql`, DB-trigger-enforced immutability) through read-only APIs and a dashboard UI. The write side already exists across modules (identity, tenancy, customers, escalations, tools-config, knowledge, ai); this feature activates the placeholder `audit` module crate as the read service, closes the one real recording gap (tool executions), adds two new permissions (`audit.view` for tenant Owner/Admin, `platform.audit.view` for all platform roles), and builds tenant + platform audit pages (table, filters, detail drawer) reusing the Helix shared components.

## Technical Context

**Language/Version**: Backend Rust (workspace, Axum 0.8, Tokio, SQLx, utoipa); Frontend Angular 22 (standalone components, signals, NgRx SignalStore), TypeScript

**Primary Dependencies**: Axum + utoipa (`routes!` co-registration), SQLx/PostgreSQL, serde; Angular + Taiga-wrapped shared components (`data-table`, `select-filter`, `dialog-shell`, `empty-state`, `loading-state`, `status-badge`)

**Storage**: PostgreSQL — existing `audit_logs` table (columns: `id`, `actor_user_id NULL→users`, `action`, `resource_type`, `resource_id NOT NULL`, `tenant_id NULL→tenants`, `details JSONB`, `created_at`, `updated_at`; append-only trigger `audit_logs_append_only`). No new tables; one new index migration.

**Testing**: `cargo test` (server integration tests gated by `require_db_tests()`, `rbac.rs` matrix tests, `openapi_coverage.rs` inventory); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application (Rust modular-monolith backend + Angular dashboard)

**Performance Goals**: Audit list responds < 2 s at tens of thousands of rows per tenant (SC-005); single SQL query per page (join `users` for actor display — no N+1)

**Constraints**: Read-only API surface (immutability already DB-enforced); tenant isolation via `TenantContext` in every query; cursor pagination consistent with conversations inbox (`encode_cursor(created_at, id)` opaque cursor)

**Scale/Scope**: 2 new endpoints + 2 new permissions + 1 new audit writer (tool executions); 2 dashboard pages + 2 shared presentational components; ~1 migration

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Modular Monolith | PASS | Read side lives in the (currently placeholder) `audit` crate; existing per-module writers are untouched. No cross-module data access added — `audit` reads its own table only. |
| II. Multi-Tenant Isolation | PASS | Tenant endpoint filters `tenant_id = ctx.tenant_id` at data-access layer; platform endpoint is mounted only under `platform_routes` (platform-permission middleware). Enforcement is server-side (FR-008). |
| III. Zero-Trust & RBAC | PASS | New `Permission::AuditView` (Owner/Admin) and `Permission::PlatformAuditView` (all 5 platform roles) guard the routes via `require_permission`; rbac.rs matrix tests extended. This feature *is* the audit surface principle III mandates. |
| IV. AI Provider Independence | PASS (n/a) | No LLM interaction. |
| V. API-First & Contracts | PASS | utoipa-documented endpoints, error envelope, cursor pagination matching existing contract conventions; `openapi_coverage.rs` EXPECTED inventory extended. Read-only GETs are inherently idempotent. |
| VI. Observability | PASS | Handlers use `tracing::error!` on failure like analytics; audit-write failures already log (`FR-012`). |
| VII. Test-First & Regression | PASS | DB-gated integration tests for isolation/filters/cursor; rbac + openapi coverage tests; frontend store/component specs. |
| VIII. DB Integrity & Migrations | PASS | No schema mutation of `audit_logs`; one additive index migration for actor filtering. Append-only trigger already enforces FR-003. |
| IX. Design System Discipline | PASS | Table/filters/drawer built from existing shared components (`data-table`, `select-filter`, `dialog-shell`); new presentational pieces go in `shared/components/` for reuse by both tenant and platform pages. |
| X. Performance & Efficiency | PASS | Single joined query per page; indexed access paths (`audit_logs_tenant_created_idx`, `audit_logs_created_idx`, new actor index). |

**Post-Phase-1 re-check**: PASS — design artifacts introduce no deviations; Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/026-audit-logs/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── audit-api.md     # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 00XX_audit_read_indexes.sql          # NEW: actor-filter index (next free number)
└── crates/
    ├── modules/
    │   ├── audit/src/
    │   │   ├── lib.rs                       # activate placeholder crate (pub mod wiring)
    │   │   ├── model.rs                     # NEW: DTOs, query params, category mapping
    │   │   ├── queries.rs                   # NEW: list queries (tenant + platform), cursor codec
    │   │   └── routes.rs                    # NEW: GET /tenant/audit-logs, GET /platform/audit-logs
    │   ├── authz/src/
    │   │   ├── permission.rs                # + AuditView, PlatformAuditView (ALL 28→30)
    │   │   └── matrix.rs                    # + grants: TENANT, TENANT_ADMIN, all 5 platform arrays
    │   └── tools/src/
    │       ├── audit.rs                     # + record_execution (action "tool.executed")
    │       └── executor.rs                  # call audit write at execution completion
    └── server/
        ├── src/router.rs                    # register routes in tenant_routes + platform_routes (+ test routes)
        ├── src/openapi.rs                   # register audit DTO schemas/tag if needed
        └── tests/
            ├── rbac.rs                      # + /test/tenant/audit/view, /test/platform/audit/view rows
            ├── openapi_coverage.rs          # + 2 EXPECTED entries
            └── audit_logs.rs                # NEW: DB-gated integration tests

frontend/apps/dashboard/src/app/
├── core/router/                             # APP_PATHS + PAGE_PERMISSIONS entries (tenant + platform audit)
├── shared/
│   ├── components/
│   │   ├── audit-log-table/                 # NEW presentational table (wraps data-table)
│   │   └── audit-detail-drawer/             # NEW presentational drawer (dialog-shell pattern)
│   └── fixtures/audit.fixtures.ts           # NEW typed fixtures for specs
└── features/
    ├── tenant/audit-logs/
    │   ├── audit-logs.component.ts|spec.ts  # NEW routed page
    │   ├── audit-logs-api.service.ts        # NEW
    │   └── audit-logs.store.ts|spec.ts      # NEW SignalStore
    └── platform/audit-logs/
        ├── platform-audit-logs.component.ts|spec.ts  # NEW routed page (reuses shared table/drawer)
        └── platform-audit-logs.store.ts|spec.ts      # NEW (own api service or shared via core — decide in tasks)
```

**Structure Decision**: Web application layout already in place (`backend/` Rust workspace + `frontend/` Angular monorepo). Backend read side activates the existing placeholder `audit` crate; existing per-module audit *writers* stay where they are (moving them is churn with no behavioral gain and would touch 7+ modules). Frontend follows the feature-area convention: routed pages per area (`features/tenant/…`, `features/platform/…`) with reusable presentational pieces in `shared/components/` (Principle IX; also avoids cross-lazy-area imports).

## Complexity Tracking

No constitution violations — table intentionally empty.
