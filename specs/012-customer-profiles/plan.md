# Implementation Plan: Customer Profiles

**Branch**: `012-customer-profiles` | **Date**: 2026-07-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/012-customer-profiles/spec.md`

## Summary

Give tenant teams a real customer profile system: tenant-scoped customer records with contact information, per-channel identifiers (email, phone, web chat, WhatsApp, Telegram), free-form metadata (≤50 attributes), and a read-only conversation history section backed by a minimal conversation summary record that future messaging features will extend. Backend exposes list/search/create/view/update endpoints; frontend upgrades the fixture-driven customers page to live data and adds a customer profile page.

Technical approach: **no new permission codes and no matrix changes** — the 008 catalog already defines `customers.view`/`customers.manage`, and the matrix already grants manage to Agent-and-above with Viewer read-only, exactly matching the clarified requirement. Backend gives the placeholder `customers` module crate its first real content (handlers, queries, validation, audit helpers) and the placeholder `conversations` module crate a minimal `conversations` table plus one read query, keeping conversation data behind its future owner's module boundary. Two migrations add `customers` + `customer_channel_identifiers` (per-tenant per-channel uniqueness via a unique index) and the minimal `conversations` table. Routes are registered through the fail-closed `.guarded()` builder under `mount_tenant`, so tenant-context middleware enforces isolation before handlers run. Search is a single SQL statement (ILIKE across name/email/phone + EXISTS over identifiers, pg_trgm-indexed) with the 010 cursor pagination pattern. Creates/updates are audited with changed-field lists via the established audit_logs pattern. Frontend replaces the customers fixture page with an Observable API service + SignalStore, adds a profile route with contact/identifier/metadata/conversation sections composed from shared components, and a create/edit dialog.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum, SQLx (PostgreSQL), existing `authz`/`tenancy`/`identity` module crates and the `.guarded()` router builder; `customers` and `conversations` module crates graduate from placeholders; Angular Router, Reactive Forms, NgRx SignalStore, existing `core/authz` + shared components (data-table, search-input, status-badge, channel-badge, dialog-shell, empty-state, toolbar); RxJS operators for all new async flows (constitution v1.2.0)

**Storage**: PostgreSQL — migration `0025` creates `customers` (tenant-owned, soft-delete, CITEXT email, JSONB metadata with app-enforced 50-key cap) and `customer_channel_identifiers` (channel CHECK: email/phone/web_chat/whatsapp/telegram; unique `(tenant_id, channel, identifier)` among live rows); migration `0026` creates minimal `conversations` (tenant-owned, customer FK, channel, status CHECK open/escalated/closed, `last_activity_at`) as the extension point for future messaging features. pg_trgm GIN indexes back partial-match search; `(tenant_id, created_at, id)` btree backs cursor pagination

**Testing**: `cargo test` — new live-gated suite `backend/crates/server/tests/customers.rs` (CRUD, search, pagination, per-operation tenant isolation, duplicate-identifier 409, validation 422, viewer read-only 403, audit rows, conversation summary scoping) + `rbac.rs` matrix additions + `shared/db/tests/schema.rs` assertions for 0025/0026; Vitest for API service, SignalStore, list page, profile page, and dialog specs

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — existing Cargo workspace + Angular pnpm workspace

**Performance Goals**: List/search is one statement (customers filtered by tenant + search predicate + keyset cursor); profile is two queries (customer with identifiers/metadata; recent conversation summaries capped at 20) — no N+1; SC-002 (<1s at 10k customers/tenant) served by pg_trgm + tenant-prefixed btree indexes; writes add only the audit insert inside the same transaction

**Constraints**: Deny-by-default routing (`.guarded()` with required permission); tenant routes mounted through `mount_tenant` so isolation is enforced pre-handler; cross-tenant reads/writes answered `not_found` (never confirm existence — spec FR-011); 401/403/404/409/422 from the existing `kernel::ApiError` vocabulary with field-level details on 422; schema changes via migration only (Constitution VIII); last-write-wins concurrency (spec edge case — no version column); RxJS-first frontend async; no frontend role→permission mapping (008 FR-010); route paths only via `APP_PATHS`

**Scale/Scope**: 2 migrations; 0 new permission codes; 5 tenant-scoped endpoints; 2 module crates gain first real content; ~2 frontend pages (list upgrade, new profile) + 1 create/edit dialog + 1 API service + 2 SignalStores; audit vocabulary +2 actions (`customer.created`, `customer.updated`)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Customer logic lives in the `customers` module crate; conversation summaries live in `conversations` (their future owner) and are read through that crate's public query function — no cross-module table access; router composition stays in `server` | ✅ Pass |
| II. Multi-Tenant Isolation | All three new tables carry `tenant_id`; every query filters by the middleware-resolved tenant; identifier uniqueness is per-tenant so identical emails coexist across tenants; cross-tenant access → `not_found`; per-operation isolation tests required by FR-015 | ✅ Pass |
| III. Zero-Trust Security & RBAC | Reuses `customers.view`/`customers.manage` through deny-by-default `.guarded()` registration; server-side enforcement independent of UI hiding; creates/updates audited with actor/action/changed-fields/time (FR-017) | ✅ Pass |
| IV. AI Provider Independence | Not touched | ✅ N/A |
| V. API-First & Contract Consistency | Endpoints documented in `contracts/rest-api.md`; cursor pagination + error envelope reused; PATCH is partial-update; identifier conflict → 409 with existing-customer detail | ✅ Pass |
| VI. Observability by Default | Request-id/tracing middleware unchanged and applies to new routes; audit trail append-only | ✅ Pass |
| VII. Test-First & Regression Discipline | Dedicated integration suite with per-operation isolation matrix; rbac matrix extension; schema tests; Vitest specs per story | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration-only; FKs + CHECKs mirror existing conventions (UUID PK, timestamps, soft delete, partial unique indexes); JSONB metadata is a recorded normalization deviation (see Complexity Tracking); indexes defined for every production query path | ⚠️ Justified deviation |
| IX. Design System Discipline | List and profile pages compose existing shared components (data-table, search-input, status-badge, channel-badge, dialog-shell, empty-state); channel-badge extended once (email/phone variants) rather than duplicated; no raw Taiga styling in feature pages | ✅ Pass |
| X. Performance & Efficiency | Single-statement list/search; bounded profile queries; no N+1; keyset (not offset) pagination; audit insert shares the write transaction | ✅ Pass |

**Initial gate**: PASS — one justified deviation recorded in Complexity Tracking.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no new deviations. Two nuanced calls, both grounded in clarifications: (1) the `conversations` table ships without any write endpoint — tests seed it directly, which is the clarified scope (profile reads it, messaging features extend it); (2) the 409 conflict body names the already-holding customer, which is safe because both records are same-tenant by construction and the caller holds `customers.view` transitively (manage implies view in the matrix).

## Project Structure

### Documentation (this feature)

```text
specs/012-customer-profiles/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── rest-api.md      # Customer list/search/create/view/update + conversation history endpoints, errors, audit actions
│   └── permissions.md   # Reused permission codes, route→permission map, page permissions
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0025_customers.sql                  # NEW — customers + customer_channel_identifiers + search/cursor indexes
│   └── 0026_conversations.sql              # NEW — minimal conversations table (summary fields only)
└── crates/
    ├── modules/
    │   ├── customers/
    │   │   ├── Cargo.toml                  # MODIFIED — real dependencies (axum, sqlx, kernel, authz, serde, …)
    │   │   └── src/
    │   │       ├── lib.rs                  # MODIFIED — module exports, public customer_exists query
    │   │       ├── routes.rs               # NEW — list/search, create, get, patch handlers
    │   │       ├── model.rs                # NEW — Customer, ChannelIdentifier types, payloads, validation
    │   │       └── audit.rs                # NEW — customer.created / customer.updated audit helpers
    │   └── conversations/
    │       ├── Cargo.toml                  # MODIFIED — real dependencies
    │       └── src/
    │           └── lib.rs                  # MODIFIED — ConversationSummary type + list_recent_for_customer query + history handler
    ├── shared/
    │   └── db/tests/schema.rs              # MODIFIED — 0025/0026 schema assertions
    └── server/
        ├── src/router.rs                   # MODIFIED — /tenant/customers routes via .guarded() under mount_tenant
        └── tests/
            ├── rbac.rs                     # MODIFIED — customer operations in the role×operation matrix
            └── customers.rs                # NEW — CRUD/search/isolation/conflict/validation/audit/history suite

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts            # MODIFIED — Customer, CustomerChannel, ChannelIdentifier, ConversationSummary, payloads
│   ├── authz/permissions.ts                # MODIFIED — profile page entry (customers.view) if not covered by list entry
│   └── router/
│       ├── app-paths.ts                    # MODIFIED — tenant.customerDetail path
│       └── page-title.ts                   # MODIFIED — customer profile title
├── shared/components/channel-badge/…       # MODIFIED — email/phone channel variants
└── features/tenant/
    ├── tenant.routes.ts                    # MODIFIED — customers/:id child route (customers.view)
    └── customers/
        ├── customers-api.service.ts        # NEW — Observable API access (list/search/create/get/patch/history)
        ├── customers.store.ts              # NEW — list SignalStore: query, cursor, items, loading
        ├── customers.component.ts          # MODIFIED — fixture page → live list (search, pagination, create action)
        ├── customer-profile.store.ts       # NEW — profile SignalStore: customer + history state
        ├── customer-profile.component.ts   # NEW — contact / identifiers / metadata / conversation history sections
        └── customer-dialog.component.ts    # NEW — create/edit reactive form (contact, identifiers, metadata editor)
```

**Structure Decision**: Backend follows the module-ownership rule — customer data access lives entirely in the `customers` crate, conversation summary data access in the `conversations` crate (its long-term owner), with the profile's history read crossing that boundary only through the conversations crate's public interface. Route registration stays in `server/router.rs` using the deny-by-default builder. Frontend upgrades the existing `features/tenant/customers/` folder in place (spec-002 layering: feature-scoped service + SignalStores; shared components for all visuals), touching `core/` only for models, paths, titles, and page permissions.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| JSONB `metadata` column on `customers` (normalization deviation, Constitution VIII) | Metadata is tenant-defined, schema-free key-value data (spec assumption: no attribute schema or type system); read/written only as a whole with the profile; capped at 50 keys app-side | A `customer_metadata` child table adds a join or second query to every list/profile read and per-key DML churn on every edit, while providing relational integrity for data that has no relations, no cross-row constraints, and no standalone query path in this feature |
