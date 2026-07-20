# Implementation Plan: Notifications

**Branch**: `027-notifications` | **Date**: 2026-07-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/027-notifications/spec.md`

## Summary

Add a persistent, tenant-scoped, per-member notification inbox surfaced by the topbar bell (which exists today only as a visual placeholder backed by an ephemeral counter). Four event types — new escalation, conversation assigned, AI response failed, tool approval required — are turned into notification rows by a new outbox consumer worker, delivered live over the existing `/tenant/events` SSE stream, and read/marked through a new REST surface.

The single most important design constraint, discovered in the existing code: the shared `outbox_events` table is consumed with **claim-and-delete** semantics (`escalations::events`, `ai::agent_responder`), so exactly one consumer sees each row. A notifications worker therefore **cannot** re-consume `conversation.assignment_changed` — those rows are already claimed and deleted by the escalations consumer. Instead, trigger sites emit two new private event types, `notification.requested` and `notification.resolved`, which only the notifications worker claims. This keeps existing pipelines untouched and gives at-least-once delivery with failure isolation (FR-017).

The placeholder `notifications` crate is an email-transport abstraction, not a stub for this feature; it is renamed to `email` to free the name.

## Technical Context

**Language/Version**: Backend Rust (workspace, Axum 0.8, Tokio, SQLx, utoipa); Frontend Angular 22 (standalone components, signals, NgRx SignalStore), TypeScript

**Primary Dependencies**: Axum + utoipa (`routes!` co-registration), SQLx/PostgreSQL, serde, tokio broadcast (SSE fan-out via `escalations::presence::Runtime`); Angular + Taiga-wrapped shared components, existing fetch-based `RealtimeService`

**Storage**: PostgreSQL — one new table `notifications`; reuses existing `outbox_events` (no schema change to it)

**Testing**: `cargo test` (DB-gated server integration tests, `rbac.rs` matrix, `openapi_coverage.rs` inventory); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application (Rust modular-monolith backend + Angular dashboard)

**Performance Goals**: Unread count reflected within 5 s of the event (SC-002/SC-009) — the worker's idle poll is 1 s, matching the existing escalation consumer; notification list < 1 s at 1,000 rows per member (SC-004) via a covering index

**Constraints**: No change to `outbox_events` schema or to existing consumers' claim predicates; tenant isolation enforced in every query; notification creation must not fail the originating action (FR-017)

**Scale/Scope**: 1 new table + 1 migration; 4 REST endpoints; 1 new worker; 9 emit/resolve call sites (5 escalation sites behind 2 shared helpers, 1 assignment, 2 AI, 1 tool decision); 1 crate rename; ~4 frontend units + topbar rewire

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Modular Monolith | PASS | New `notifications` crate owns its table and worker. Trigger modules emit an outbox event — a domain event, not a cross-module call — so no module reads another's data. The one deliberate exception is recipient resolution, which reads `tenant_memberships`; see Complexity Tracking. |
| II. Multi-Tenant Isolation | PASS | `notifications.tenant_id` NOT NULL; every read/write filters on `TenantContext` **and** the caller's own membership. Fan-out resolves recipients from memberships of that tenant only. |
| III. Zero-Trust & RBAC | PASS | Endpoints sit behind auth + tenant context. Per FR-012a no new permission gates the inbox — the row-level filter is `recipient_membership_id = caller`, which is stricter than a role check. `rbac.rs` gains rows asserting all tenant roles reach their own inbox. |
| IV. AI Provider Independence | PASS (n/a) | No LLM interaction; the AI-failure trigger reads an already-recorded outcome. |
| V. API-First & Contracts | PASS | utoipa-documented endpoints, standard error envelope, cursor pagination matching the audit/conversations convention. Mark-read is idempotent by construction. |
| VI. Observability | PASS | Worker logs claim/fan-out/failure with `tracing`; FR-018 satisfied by counters on notifications created and worker errors. |
| VII. Test-First & Regression | PASS | DB-gated integration tests per trigger and for isolation, dedup, and auto-resolve; frontend store/component specs. |
| VIII. DB Integrity & Migrations | PASS | One additive migration (`0054`). Unique index on `(recipient_membership_id, dedupe_key)` enforces FR-010 in the database rather than in application logic. Fan-out is a single set-based `INSERT … SELECT` — no N+1. |
| IX. Design System Discipline | PASS | Bell/panel/list built as `shared/components/` units so the tenant page and topbar share them; no raw Taiga in feature pages. |
| X. Performance & Efficiency | PASS | Set-based fan-out and set-based resolve; partial indexes for the unread count and the resolve lookup; SSE push instead of polling. |

**Post-Phase-1 re-check**: PASS — design artifacts introduce no new deviations. Complexity Tracking has one entry, unchanged from the pre-Phase-0 evaluation.

## Key Design Decisions

Full rationale in [research.md](research.md). The load-bearing ones:

1. **Private outbox event types** (`notification.requested`, `notification.resolved`) rather than re-consuming existing events — forced by claim-and-delete semantics.
2. **Dedupe key in the database**, not the application: `dedupe_key` + unique index + `ON CONFLICT DO NOTHING` gives FR-010 for free, and gives the 15-minute AI-failure suppression window (spec Assumptions) by putting a time bucket in the key.
3. **Auto-resolve needs its own signal** — resolution fires at the claim/decide sites, which the worker cannot otherwise observe. Hence `notification.resolved`.
4. **`origin` already discriminates the double-notify case** — `assign_in_tx` records an `origin` field, and escalation-driven assignment passes `"escalations"`. FR-009a is implemented by skipping emission when `origin == "escalations"`, mirroring how the escalations consumer already filters that same value.
5. **Crate rename** `notifications` → `email`, freeing the name for this feature.

## Known Limitation (recorded, accepted)

Two of the five creation triggers — AI failure and tool approval — are written via a **pool, not a transaction** (`generation_record::insert(pool, …)`; the tool-request insert in `ai::engine`). Their `notification.requested` emission is therefore a separate statement, not atomic with the domain write: a process crash in the window between them loses that one notification. The other three triggers are transactional and lose nothing.

This is accepted rather than fixed: making those paths transactional means restructuring the AI engine's persistence, which is far beyond this feature's blast radius, and the failure mode (a missed notification about an already-recorded failure, still visible in the conversation itself) is proportionate. Recorded here so it is a known trade-off and not an undiscovered bug.

## Project Structure

### Documentation (this feature)

```text
specs/027-notifications/
├── plan.md                          # This file
├── research.md                      # Phase 0 output
├── data-model.md                    # Phase 1 output
├── quickstart.md                    # Phase 1 output
├── contracts/
│   └── notifications-api.md         # Phase 1 output
├── checklists/
│   └── requirements.md              # From /speckit-specify
└── tasks.md                         # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0054_notifications.sql               # NEW: notifications table + indexes
└── crates/
    ├── modules/
    │   ├── email/                           # RENAMED from notifications/ (email transport)
    │   │   ├── Cargo.toml                   # package name notifications → email
    │   │   └── src/{lib,smtp,noop}.rs       # unchanged contents
    │   ├── notifications/                   # NEW crate (name freed by the rename)
    │   │   ├── Cargo.toml
    │   │   └── src/
    │   │       ├── lib.rs                   # module docs, re-exports
    │   │       ├── model.rs                 # Notification DTOs, kind enum, state enum
    │   │       ├── emit.rs                  # emit_requested_in_tx / _on_pool, emit_resolved_*
    │   │       ├── recipients.rs            # recipient resolution per kind
    │   │       ├── queries.rs               # list / unread count / mark read / fan-out / resolve
    │   │       ├── worker.rs                # claim → fan out → broadcast → delete loop
    │   │       └── routes.rs                # 4 endpoints
    │   ├── conversations/src/queries.rs     # assign_in_tx: emit unless origin == "escalations"
    │   ├── escalations/src/
    │   │   ├── routing.rs                   # 2 helpers (queued / assigned) called from 5 sites:
    │   │   │                                #   route(assigned), route(queued), claim_in_tx,
    │   │   │                                #   drain_one_for_membership_in_tx, drain_any_in_tx
    │   │   └── presence.rs                  # + Event::NotificationCreated / NotificationCleared
    │   │                                    #   (primitive fields only — no dep on notifications)
    │   ├── ai/src/engine.rs                 # emit on awaiting_approval + on GenerationOutcome::Failed
    │   └── tools/src/approval.rs            # decide(): emit notification.resolved
    └── server/
        ├── Cargo.toml                       # notifications → email dep; + new notifications dep
        ├── src/main.rs                      # spawn notifications worker
        ├── src/router.rs                    # mount 4 routes (+ test routes for rbac)
        ├── src/openapi.rs                   # register DTO schemas
        └── tests/
            ├── notifications.rs             # NEW: DB-gated integration tests
            ├── rbac.rs                      # + inbox-reachable-by-every-role rows
            ├── openapi_coverage.rs          # + 4 EXPECTED entries
            └── team_members.rs              # notifications:: → email:: (rename fallout)

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts             # + NotificationWire, NotificationListWire, mappers
│   ├── notifications/
│   │   ├── notifications.api.ts             # NEW: typed HTTP client
│   │   └── notifications.store.ts           # NEW: SignalStore (list, unreadCount, mark read)
│   ├── realtime/notifications.service.ts    # REWRITTEN: drives store from SSE, no local counter
│   └── router/{app-paths,page-title}.ts     # + notifications route
├── shared/
│   ├── components/
│   │   ├── notification-bell/               # NEW: bell + badge (presentational)
│   │   └── notification-list/               # NEW: list + item, used by panel and page
│   └── fixtures/notification.fixtures.ts    # NEW
├── layout/topbar/topbar.component.ts        # bell wired to store; ephemeral counter removed
└── features/tenant/notifications/           # NEW: full-page list route
```

**Structure Decision**: Web application layout, matching every prior feature in this repo. Backend work is a new `notifications` module crate plus small emit-site edits in four existing crates; frontend follows the 026 audit-logs shape (shared presentational components + a feature page + a core store), with the addition that the topbar consumes the same store.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `notifications` recipient resolution queries `tenant_memberships` (owned by tenancy/identity), a cross-module read that Principle I discourages | Fan-out must turn "everyone who can claim this" into a concrete recipient set, which is by definition a membership+role query. Every trigger would otherwise have to compute and pass the full recipient list, pushing notification policy into escalations, ai, and tools. | A `tenancy` application service exposing `members_with_permission(tenant, permission)` is the clean fix and is the intended follow-up. It is deferred here only because it means adding a public interface to tenancy that no other caller needs yet; the query is read-only, tenant-filtered, and confined to `recipients.rs` so extracting it later is a one-file change. |
