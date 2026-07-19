# Implementation Plan: Customer Feedback

**Branch**: `024-customer-feedback` | **Date**: 2026-07-19 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/024-customer-feedback/spec.md`

## Summary

Collect post-conversation feedback from widget customers: a 1–5 star rating with optional comment, prompted once a conversation has ended, immutable once submitted, one per conversation. Feedback is stored in a new `feedback` backend module as an append-only, tenant-scoped, analytics-ready fact table that snapshots attribution (channel, AI agent configuration, assigned human agent at end). Surfaces: a feedback prompt + passive entry point in the widget (`frontend/apps/widget`), rating/comment display and a satisfaction badge in the dashboard conversation detail and inbox list, and a single tenant-wide average/count summary card. Public submission rides the existing `/widget/v1` session-token + origin-check + rate-limit stack; tenant reads ride the existing `/tenant` RBAC stack.

**Trigger mechanism (corrected after codebase verification — see research.md R5)**: there is no SSE "closed" event and `GET /widget/v1/conversation` returns null for ended conversations, so the widget learns about pending feedback from a new session-keyed `GET /widget/v1/feedback/pending` lookup, called on widget open and on the existing `409 conversation_closed` response from send. No existing endpoint or SSE behavior changes.

## Technical Context

**Language/Version**: Backend: Rust (stable, workspace at `backend/`), Axum + Tokio + SQLx. Frontend: Angular 22 standalone components, TypeScript, Signals + RxJS.

**Primary Dependencies**: Axum, SQLx (PostgreSQL), utoipa (OpenAPI), Serde, Tracing; Angular SignalStores per `frontend/CLAUDE.md`, existing `widget-api.service` / `widget-sse.client` in the widget app, `conversations-api.service` in the dashboard.

**Storage**: PostgreSQL via migration `backend/migrations/0051_customer_feedback.sql`; no object storage, no Redis needs.

**Testing**: `cargo test` (module unit tests + server integration tests in `backend/crates/server/tests`); frontend spec files (`*.spec.ts`) colocated per existing convention.

**Target Platform**: Existing modular-monolith server + browser (dashboard SPA, embeddable widget iframe).

**Project Type**: Web application (backend + two frontend apps).

**Performance Goals**: Feedback submission is a single-row insert on an indexed table — no measurable impact on widget message latency; badge/summary data joins are index-backed and must not introduce N+1 patterns in conversation list queries.

**Constraints**: Tenant isolation on every query (Constitution II); duplicate prevention must hold under concurrent submissions (DB uniqueness, not application checks); feedback survives conversation archival (append-only, no soft-delete cascade); public endpoints must enforce widget session ownership + origin allowlist + rate limiting like existing `/widget/v1` routes. Wire casing differs by surface: `/widget/v1` is camelCase, `/tenant` is snake_case (research.md R6).

**Scale/Scope**: One new backend module crate, one migration, 2 public + 2 tenant endpoint touches, ~3 new widget components, ~3 dashboard touch points. Rating 1–5 integer, comment ≤ 2,000 chars.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Enterprise Modular Monolith | PASS | New isolated `feedback` module crate; talks to conversations/widgets data only via its own tenant-scoped queries and route composition in `server`; extractable later. |
| II | Multi-Tenant Isolation | PASS | `tenant_id` on the feedback table; every query tenant-filtered; widget submission authorized by session-owns-conversation check, not client claims. |
| III | Zero-Trust & RBAC | PASS | Public endpoints require widget session token + origin check; tenant endpoints reuse existing tenant RBAC (conversation view permission). No new sensitive ops requiring audit beyond insert provenance (record carries session + timestamps). |
| IV | AI Provider Independence | PASS (N/A) | No LLM interaction in this feature. |
| V | API-First & Contract Consistency | PASS | Endpoints follow existing `/widget/v1` and `/tenant` conventions, utoipa-documented; submission is idempotent (duplicate submit returns existing record as success). |
| VI | Observability | PASS | Handlers use `tracing` spans + request ID propagation like existing widget routes. |
| VII | Test-First & Regression | PASS | Module unit tests, server integration tests for both surfaces, frontend spec files planned per story. |
| VIII | DB Integrity & Migrations | PASS w/ justified deviation | Migration-only schema change; uniqueness enforced by DB index. Deviation: denormalized attribution snapshot columns and no `deleted_at` — see Complexity Tracking. |
| IX | Design System Discipline | PASS | Satisfaction badge is a shared dashboard component (sibling of `status-badge`/`ai-confidence-badge`); widget star-rating reuses widget theme tokens. |
| X | Performance & Efficiency | PASS | Single-insert writes; list badge data fetched in the existing list query (join/column), not per-row requests. |

**Post-design re-check (after Phase 1)**: PASS — no new violations introduced by the data model or contracts; the two Principle VIII deviations remain justified in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/024-customer-feedback/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── feedback-api.md  # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0051_customer_feedback.sql        # conversation_feedback table
└── crates/
    ├── modules/feedback/                  # NEW module crate
    │   └── src/
    │       ├── lib.rs                     # module docs: purpose/interfaces/deps
    │       ├── model.rs                   # rows, DTOs, payloads
    │       ├── queries.rs                 # tenant-scoped SQL
    │       ├── public_routes.rs           # /widget/v1 submit + read
    │       └── tenant_routes.rs           # /tenant feedback summary
    ├── modules/conversations/src/         # extend detail/list DTOs with rating
    │   ├── model.rs
    │   └── queries.rs
    └── server/
        ├── src/router.rs                  # mount feedback routes (widget CORS + tenant)
        └── tests/                         # feedback API integration tests

frontend/apps/widget/src/
├── components/
│   ├── feedback-prompt.component.ts       # NEW: prompt + collapsed entry point + thank-you
│   ├── star-rating.component.ts           # NEW: 1–5 star input
│   └── chat-window.component.ts           # render the prompt
└── core/
    ├── widget-api.service.ts              # submitFeedback + getPendingFeedback
    ├── widget.store.ts                    # feedback state, pending lookup, dismissal
    ├── feedback-dismissal.store.ts        # NEW: localStorage dismissal flags
    └── models.ts                          # feedback types

frontend/apps/dashboard/src/app/
├── shared/components/satisfaction-badge/  # NEW shared badge component
└── features/tenant/conversations/
    ├── conversation-detail.component.ts   # feedback display section
    ├── conversation-detail.store.ts
    ├── inbox-list.component.ts            # badge on rows
    ├── conversations.component.ts         # satisfaction summary card
    ├── conversations-api.service.ts       # summary endpoint call
    └── conversations.store.ts
```

**Structure Decision**: New `backend/crates/modules/feedback` crate mirrors the `widgets` module split (public vs tenant routes, model/queries separation) and keeps feedback extractable per Principle I. The `analytics` placeholder crate is deliberately NOT used — feedback capture is its own domain; a future analytics feature consumes the table. Dashboard surfaces stay inside the existing `conversations` feature (real data already flows there), with the badge promoted to `shared/components` for reuse; the tenant-wide summary card lives on the Conversations page because Overview/Analytics pages are still fixture-driven (see research.md R6).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Denormalized snapshot columns (`channel`, `assigned_membership_id`, `agent_configuration_id`) on `conversation_feedback` (Principle VIII: normalized by default) | Feedback is an analytics fact recording attribution *as it was at conversation end*; conversation rows mutate (reassignment, archival) and would falsify history | Joining live `conversations` at query time reports current state, not state-at-feedback; FR-005/FR-012 and SC-002/SC-006 require the historical snapshot |
| No `deleted_at` column (repo convention: soft delete) | Feedback is immutable and append-only (FR-012, FR-013); no delete operation exists in any flow | Adding an unused soft-delete column implies a lifecycle that must never occur and every query would need a dead `deleted_at IS NULL` filter |
