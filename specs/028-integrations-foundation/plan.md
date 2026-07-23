# Implementation Plan: Integrations Foundation

**Branch**: `028-integrations-foundation` | **Date**: 2026-07-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/028-integrations-foundation/spec.md`

## Summary

Activate the placeholder `integrations` backend crate and replace the fixture-backed dashboard integrations page with a real feature: a platform-seeded integration catalog (one connectable generic inbound-webhook integration + "coming soon" placeholders), a per-tenant connection lifecycle (connect → update/rotate secret → disconnect → reconnect on the same record), AES-256-GCM-encrypted secret storage, a public webhook intake endpoint with HMAC verification and rate limiting, derived health status, a per-connection event log with 90-day retention, and audit-trail writes for all lifecycle actions. Frontend adds an API-backed integrations list + detail page with connect/disconnect flows and status badges, following the established SignalStore/api-service/wire-mapper conventions.

## Technical Context

**Language/Version**: Backend Rust (workspace, edition per `backend/Cargo.toml`); Frontend Angular 22 + TypeScript (standalone components, signals, RxJS-first)

**Primary Dependencies**: Axum + utoipa (`OpenApiRouter`/`routes!`), SQLx/PostgreSQL, `aes-gcm` 0.10 (already a workspace dep), `sha2`, `hmac` (add if absent), Tokio workers; Angular + Taiga-wrapped shared components, NgRx SignalStore

**Storage**: PostgreSQL via migrations `backend/migrations/0056_*.sql` (next free number after 0055); no object storage needed

**Testing**: `cargo test -p integrations` (unit) + `backend/crates/server/tests/integrations_*.rs` (API/integration, DB required — run narrow suites with DB up; `cargo test --workspace` skips DB tests); frontend `pnpm ng test dashboard`, `pnpm ng build dashboard`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application (modular-monolith Rust backend + Angular dashboard)

**Performance Goals**: Webhook intake acknowledges < 2 s under normal load (SC-003); intake is store-then-ack (no synchronous downstream processing); status derivation at read time so error state is visible within 1 minute (SC-006)

**Constraints**: Secrets never leave the backend after submission (masked hint only); webhook payload cap 256 KB; per-connection intake rate limit 60/min via existing `InMemoryRateLimitStore`; rejections for unknown/inactive endpoints return the same generic 404 (no existence leak)

**Scale/Scope**: Catalog ≤ ~10 entries at launch; one connection per tenant+integration; event/delivery volume bounded by rate limit + 90-day retention sweeper (mirrors notifications sweeper)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Enterprise Modular Monolith | PASS | All logic in the existing `integrations` crate; server only mounts routes and spawns the sweeper. No cross-module data access — audit writes via the same in-crate writer pattern used by `widgets::audit`. |
| II. Multi-Tenant Isolation | PASS | Every new tenant-owned table carries `tenant_id`; all tenant queries filter by it; webhook token lookup resolves to exactly one connection and never crosses tenants; dedicated isolation test. |
| III. Zero-Trust Security & RBAC | PASS | Tenant routes guarded by existing `integrations.view` / `integrations.manage` permissions (already in authz catalog + matrix — reused unchanged per clarification). Secrets AES-256-GCM encrypted at rest, never returned, never logged. Lifecycle actions audited (who/what/when). |
| IV. AI Provider Independence | N/A | No LLM involvement. |
| V. API-First & Contract Consistency | PASS | REST endpoints registered in utoipa OpenAPI (covered by `openapi_coverage.rs`), cursor pagination for events (same `{data, pagination}` snake_case shape as audit/notifications — verified in code, see contract), standard `ApiError` envelope. Connect is idempotency-safe (duplicate connect → 409). |
| VI. Observability | PASS | Existing request-id/tracing middleware applies; intake and sweeper log structured tracing events; delivery outcomes are inspectable via the event log. |
| VII. Test-First & Regression | PASS | Unit tests (crypto, verification, status derivation) + server API tests (lifecycle, confidentiality, isolation, webhook accept/reject, RBAC) + frontend store/component specs. |
| VIII. DB Integrity & Migrations | PASS | Single migration `0056_integrations_foundation.sql`; normalized tables; indexes on every production query path (token hash, tenant+connection+created_at, retention cutoffs). |
| IX. Design System Discipline | PASS | Reuse `--app-*` tokens and existing shared components (status badge pattern, empty/loading states, drawer/table patterns); new UI stays inside `features/tenant/integrations/` + `shared/` per frontend layering. |
| X. Performance & Efficiency | PASS | Store-then-ack intake; single-query list (catalog LEFT JOIN connection); keyset pagination; rate limits; no N+1. |

**Post-design re-check (after Phase 1)**: PASS — no violations introduced; Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/028-integrations-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── integrations-api.md
└── tasks.md             # Phase 2 (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0056_integrations_foundation.sql        # catalog, connections, secrets, deliveries, events + seed
├── crates/modules/integrations/
│   ├── Cargo.toml                              # add axum, sqlx, serde, utoipa, uuid, chrono, aes-gcm, sha2, hmac, base64, kernel, tracing
│   └── src/
│       ├── lib.rs                              # module docs: purpose/responsibilities/interfaces
│       ├── model.rs                            # DTOs (utoipa ToSchema), enums (status, event type, reason)
│       ├── crypto.rs                           # AES-256-GCM seal/open + hint (pattern from ai-providers/src/crypto.rs)
│       ├── queries.rs                          # SQLx queries; keyset cursor for events (audit pattern)
│       ├── routes.rs                           # tenant handlers: list/detail/connect/update/disconnect/events
│       ├── webhook.rs                          # public intake handler: token→connection, HMAC verify, store, ack
│       ├── status.rs                           # health derivation (consecutive-failure rule)
│       ├── audit.rs                            # audit_logs writers (widgets::audit pattern)
│       └── retention.rs                        # 90-day sweep for events + deliveries
└── crates/server/
    ├── src/router.rs                           # mount /tenant/integrations/* (guarded) + public /hooks/v1/{token}
    ├── src/openapi.rs                          # register new DTOs
    ├── src/main.rs                             # spawn integration retention sweeper (notifications sweeper pattern)
    └── tests/
        ├── integrations_catalog.rs             # list/detail, coming-soon, retired-entry, derived status
        ├── integrations_rbac.rs                # Agent 403, Viewer read-only, Manager manage
        ├── integrations_lifecycle.rs           # connect/update/rotate/disconnect/reconnect, 409, dual audit+event logging
        ├── integrations_secret_confidentiality.rs  # secrets absent from every read surface + logs
        ├── integrations_webhook.rs             # HMAC accept/reject, inactive/unknown token, 413 size & 429 rate limits
        ├── integrations_events.rs              # cursor pagination + cross-tenant events isolation
        └── integrations_isolation.rs           # cross-tenant invisibility on list/detail

frontend/apps/dashboard/src/app/
├── core/api/tenant-api.models.ts               # Integration*Wire types + mappers
├── core/router/app-paths.ts                    # add tenant integration detail path (:slug)
├── features/tenant/integrations/
│   ├── integrations-api.service.ts             # typed HTTP (ApiResponse<T>, cursor pagination)
│   ├── integrations.store.ts                   # SignalStore: catalog+status list
│   ├── integrations.component.ts               # rework: fixture → store-backed list with status badges
│   ├── integration-detail.store.ts             # SignalStore: detail, connect/update/disconnect, events paging
│   ├── integration-detail.component.ts         # detail page: config form, masked secrets, webhook URL, event log
│   └── *.spec.ts                               # store/component specs
├── features/tenant/tenant.routes.ts            # add detail route (integrations.view guard)
└── shared/fixtures/                            # retire/replace integration fixture usage as needed
```

**Structure Decision**: Web application layout already in place. Backend work activates the existing placeholder `integrations` crate (mirroring how 026 activated `audit`); frontend work stays inside the existing `features/tenant/integrations/` folder plus the standard `core/api` wire-type home. Public webhook intake mounts beside the widget public router with its own rate-limit layer.

## Complexity Tracking

No constitution violations — table intentionally empty.
