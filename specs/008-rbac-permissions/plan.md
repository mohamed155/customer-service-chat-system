# Implementation Plan: RBAC & Permissions

**Branch**: `008-rbac-permissions` | **Date**: 2026-07-10 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/008-rbac-permissions/spec.md`

## Summary

Introduce a central permission catalog and role→permission matrix covering the five platform roles and five tenant roles, enforce it server-side on every `/api/v1` route (deny by default, evaluated per request against the live database role), and expose the effective permission set through `GET /me` so the dashboard can hide navigation, guard routes, and gate per-page actions without duplicating role logic. Platform-staff capabilities inside a tenant are environment-aware (full in non-production, least-privilege in production).

Technical approach: a new `authz` module crate owns the `Permission` enum, `TenantRole` enum, and the static matrix (code-defined, single source of truth). The existing `tenant_context_middleware` is extended to resolve the caller's tenant role and compute an effective permission set per request; a `require_permission` route-layer helper enforces declared permissions with a fail-closed route-registration convention. The frontend gains a `core/authz` layer (PermissionsService signals, `permissionGuard`, `*appHasPermission` directive) fed exclusively by server-computed permissions from `/me`.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum, Tokio, SQLx (PostgreSQL); Angular Router, NgRx 21 (existing `tenantContext` slice), Taiga UI 5

**Storage**: PostgreSQL — no schema changes. Roles already persist in `users.platform_role` and `tenant_memberships.role`; the permission matrix is code-defined (static per release, per spec FR-002 / Assumptions)

**Testing**: `cargo test` (unit + live-gated integration tests in `backend/crates/server/tests/`, pattern from `tenancy.rs`); Vitest via `pnpm ng test dashboard`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — Cargo workspace backend + pnpm/Angular frontend

**Performance Goals**: Permission evaluation adds no extra DB round-trips beyond the existing per-request principal + membership lookups (the membership query is widened to also return `role`); matrix lookup is in-memory constant-time (SC-004)

**Constraints**: Deny by default (FR-003); role changes effective on the very next request server-side (FR-011) — satisfied because the principal and membership are already resolved from the DB per request, never from JWT claims alone; 403 (`unauthorized`) must stay distinguishable from 401 (`unauthenticated`) — already the `kernel::ApiError` contract

**Scale/Scope**: 10 roles × ~25 permissions; 8 tenant dashboard areas + platform area; ~6 existing endpoints re-declared + enforcement plumbing; no new tables

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | New `authz` crate under `backend/crates/modules/` with a documented public interface (catalog, matrix, guards); `identity` and `tenancy` consume it through that interface only | ✅ Pass |
| II. Multi-Tenant Isolation | Builds on the existing tenant-context middleware; tenant-role evaluation happens at the data-access layer per request; frontend checks are presentation-only (FR-010) | ✅ Pass |
| III. Zero-Trust Security & RBAC | This feature *implements* the principle: all `/api/v1` routes require declared permissions, deny by default; existing access-denied audit records retained and extended with a `permission_denied` reason | ✅ Pass |
| IV. AI Provider Independence | Not touched | ✅ N/A |
| V. API-First & Contract Consistency | `/me` extension and 403 semantics documented in `contracts/`; standard error envelope reused | ✅ Pass |
| VI. Observability by Default | Denials recorded on the tracing span (`authz.denied_permission`) and audited via the existing `audit_logs` append-only path | ✅ Pass |
| VII. Test-First & Regression Discipline | Role × operation allow/deny matrix tests (backend integration), guard/service/directive specs (frontend) required by FR-012/SC-005 | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | No schema change; existing role columns and CHECK constraints reused unchanged | ✅ Pass |
| IX. Design System Discipline | No new visual components; sidebar/nav reuse existing components with permission filtering | ✅ Pass |
| X. Performance & Efficiency | No added queries (widened existing membership query); in-memory matrix; no N+1 | ✅ Pass |

**Initial gate**: PASS — no violations, Complexity Tracking not required.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no deviations; the `authz` crate documents Purpose/Responsibilities/Public Interfaces/Dependencies/Data Model/Extension Points per the Documentation requirement.

## Project Structure

### Documentation (this feature)

```text
specs/008-rbac-permissions/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── permissions.md   # Permission catalog + role→permission matrix (canonical)
│   └── rest-api.md      # /me extension, 403 contract, enforcement declarations
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── crates/
│   ├── modules/
│   │   ├── authz/                      # NEW crate — permission catalog & enforcement
│   │   │   └── src/
│   │   │       ├── lib.rs              # Module docs, re-exports, RequireExt helpers
│   │   │       ├── permission.rs       # Permission enum (+ Display/FromStr, serde codes)
│   │   │       ├── role.rs             # TenantRole enum (owner/admin/manager/agent/viewer)
│   │   │       ├── matrix.rs           # tenant_role_permissions / platform_role_permissions /
│   │   │       │                       #   staff_tenant_permissions(role, is_production)
│   │   │       └── guard.rs            # require_permission route-layer + PermissionSet extension
│   │   ├── identity/                   # UNCHANGED types; consumed by authz
│   │   └── tenancy/
│   │       └── src/
│   │           ├── lib.rs              # TenantContext gains tenant_role + effective PermissionSet
│   │           ├── authorize.rs        # has_active_membership → fetch_membership_role
│   │           └── routes.rs           # /me returns permission arrays; routes declare permissions
│   └── server/
│       ├── src/router.rs               # Route groups declare required permission (fail-closed builder)
│       └── tests/
│           └── rbac.rs                 # NEW — role × operation allow/deny matrix (live-gated)
frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts        # MeResponse gains permission arrays
│   ├── authz/                          # NEW
│   │   ├── permissions.ts              # Permission string-literal union + APP_PERMISSIONS constants
│   │   ├── permissions.service.ts      # Signal-based effective-permission set + has(permission)
│   │   ├── permission.guard.ts         # CanMatch guard reading route data.requiredPermission
│   │   └── has-permission.directive.ts # *appHasPermission structural directive
│   ├── http/api-error.interceptor.ts   # On 403 `unauthorized`: trigger permission refresh
│   └── tenant/current-user.service.ts  # Exposes permission data from /me
├── layout/sidebar/                     # Nav items filtered by permission
└── features/tenant/tenant.routes.ts    # Routes declare requiredPermission data + guard
```

**Structure Decision**: Web application layout (existing Cargo workspace + Angular pnpm workspace). The only new top-level unit is the `backend/crates/modules/authz` crate; the frontend adds one new `core/authz` folder inside the established `core/` layer (singletons, no feature deps — consistent with spec 002 layering).

## Complexity Tracking

No constitution violations — table intentionally empty.
