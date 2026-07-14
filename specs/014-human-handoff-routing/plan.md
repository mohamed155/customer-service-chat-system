# Implementation Plan: Human Handoff & Routing

**Branch**: `014-human-handoff-routing` | **Date**: 2026-07-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/014-human-handoff-routing/spec.md`

## Summary

Give the platform its AI→human handoff: an escalation capability that marks a conversation as escalated (orthogonal to the 013 status set) with a reason and optional required skills, a routing engine that assigns the best available agent (most matched skills → lowest load → least-loaded fallback → tenant queue), a claimable/auto-draining escalation queue, per-agent availability with a presence-aware auto-revert, tenant skill catalogs, and full routing-reason transparency. The clarifications pulled real-time delivery into scope: agents are notified of assignments the moment they happen.

Technical approach: the placeholder `escalations` module crate becomes the owner of four new tables (`skills`, `agent_skills`, `agent_availability`, `escalations` — migrations 0035–0037) and the routing service. Routing runs as a single SQL candidate-selection statement (skill-match count, then open/pending load) inside a per-tenant Postgres advisory transaction lock, so concurrent escalations, claims, and queue drains serialize per tenant and a conversation can never double-assign. Claims are compare-and-set updates (409 on loss). Escalations calls one-way into `conversations` public tx interfaces (set assignee, set/clear the escalated flag); conversation-side changes flow back as transactional-outbox domain events (`0002` pattern) that escalations consumes to close out queued escalations on resolve/close and to relabel routing reasons on manual reassignment — no circular module dependency. Real-time delivery is a new `GET /tenant/events` SSE endpoint (fetch-based client, since `X-Tenant-ID` forbids native EventSource) fanned out by an in-process tokio broadcast registry that doubles as the presence source: when an agent's last stream drops past a grace window, availability auto-reverts to away. No new permission codes: queue/claim/availability reuse `conversations.view`/`conversations.manage`; skills reuse `members.view`/`members.manage`. Frontend adds a `features/tenant/escalations/` queue page, a topbar availability toggle wired to a new `core/realtime` SSE service, browser-notification + in-app assignment alerts, an escalated inbox filter, and an escalation banner + routing-reason display on the conversation detail page.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum (incl. `axum::response::sse` for the event stream), Tokio (`sync::broadcast` for per-tenant fan-out), SQLx (PostgreSQL, advisory locks, CAS updates), existing `authz`/`tenancy`/`identity` crates and the deny-by-default `.guarded()` router builder, existing transactional outbox (0002/0022) for conversations→escalations domain events; `escalations` module crate graduates from placeholder to feature owner; `conversations` crate consumed only through new public tx interfaces (assign, set/clear escalated flag) plus its emitted events. Frontend: NgRx SignalStore, RxJS-first streams (fetch-based SSE wrapped in an Observable with retry/backoff), Notification API for browser notifications, existing shared components (status-badge, channel-badge, data-table, empty-state, loading-state, dialog-shell, toolbar, section-header, inline-alert, avatar)

**Storage**: PostgreSQL — migration `0035_agent_skills.sql` (`skills` tenant catalog + `agent_skills` join, composite FKs to `tenant_memberships(tenant_id, id)`); `0036_agent_availability.sql` (`agent_availability` per-membership state, default away); `0037_escalations.sql` (`escalations` table: reason, `required_skill_names TEXT[]` snapshot + `required_skill_ids`, status `queued|assigned|closed`, routing reason enum CHECK, composite FK to `conversations(tenant_id, id)`; partial unique index = one active escalation per conversation; queue index ordered by `escalated_at`; adds `conversations.escalated_at TIMESTAMPTZ NULL` flag column maintained via the conversations public interface; load-count index on conversations `(tenant_id, assigned_membership_id) WHERE status IN ('open','pending') AND deleted_at IS NULL`). Redis unused for v1 fan-out (single-process monolith; extraction path documented in research)

**Testing**: `cargo test` — new live-gated suite `backend/crates/server/tests/escalations.rs` covering every routing branch per FR-024 (skill match, most-skills ranking, load tie-break, load fallback, queue placement, skill-aware drain order, one-at-a-time drain, claim contention, duplicate escalation 409, cross-tenant 404 matrix, audit rows) plus unit tests on the ranking logic in the escalations crate; `rbac.rs` route→permission additions; `shared/db/tests/schema.rs` assertions for 0035–0037; SSE integration test (connect, receive assignment event, presence-revert on disconnect); Vitest for realtime service, availability toggle, queue store/page, banner, routing-reason display, notification handling

**Target Platform**: Linux server (backend), evergreen browsers (dashboard; Notification API degrades gracefully when permission is denied)

**Project Type**: Web application — existing Cargo workspace + Angular pnpm workspace

**Performance Goals**: SC-003 escalation→assignment/queue decision well under 5 s (target: one advisory-locked transaction, single candidate-selection statement, <100 ms typical); SC-008 assignment notification to a connected agent <5 s (target: same-transaction commit → broadcast, sub-second); routing candidate selection and load counts are single statements over dedicated partial indexes — no N+1; SSE heartbeat every ~20 s keeps proxies from severing idle streams

**Constraints**: Deny-by-default `.guarded()` routing under `mount_tenant`; cross-tenant access answered `not_found`; no new permission codes (matrix untouched); schema changes via migrations only (Constitution VIII); modules communicate only via public interfaces + outbox events (Constitution I — no cross-module table access: the inbox `escalated` filter reads `conversations.escalated_at`, not the escalations table); routing correctness under concurrency is a hard requirement (FR-011): per-tenant advisory lock + CAS claims; AI-silence after escalation (FR-002a) enforced by the escalation flag being readable by the future AI subsystem through the escalations public interface; RxJS-first frontend async; route paths only via `APP_PATHS`; no raw Taiga styling in feature pages

**Scale/Scope**: 3 migrations; 4 new tables + 1 column; 0 new permission codes; ~9 new tenant endpoints + 1 SSE stream + 2 extended payloads (conversation detail gains `escalation`, members list gains skills/availability); backend: `escalations` crate fully implemented (routes, routing service, presence registry, event consumer), `conversations` crate gains two public tx interfaces + event emission; frontend: 1 new feature area (escalation queue), 1 new core service (realtime), topbar availability toggle + notification wiring, inbox filter chip, detail-page banner + routing reason, skills management UI in the existing team page; audit vocabulary +6 actions (`escalation.created`, `escalation.assigned`, `escalation.queued`, `escalation.claimed`, `escalation.closed`, `skill.*` / `availability.changed`)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Routing, skills, availability, queue, presence, and SSE fan-out all live in the `escalations` module crate. Dependencies are one-way: escalations → conversations (public tx interfaces: `assign_in_tx`, `set_escalated_in_tx`) and escalations → tenancy (membership validity). Conversation-side changes reach escalations only as outbox domain events (`conversation.status_changed`, `conversation.assignment_changed`) — no circular dependency, and the module could be extracted with the outbox becoming a real queue | ✅ Pass |
| II. Multi-Tenant Isolation | All four new tables carry `tenant_id` with composite FKs making cross-tenant skills, availability rows, and escalations unrepresentable; routing candidate selection is scoped by the middleware-resolved tenant; the SSE registry keys streams by (tenant, membership) so events can never fan out across tenants (FR-025, SC-006); cross-tenant access → `not_found` | ✅ Pass |
| III. Zero-Trust Security & RBAC | Every new route registered via deny-by-default `.guarded()`; reuses `conversations.view/manage` and `members.view/manage` per the spec's permission mapping; availability is self-service-only (handler ignores any target other than the caller); escalations, every assignment, claims, close-outs, skill changes, and availability changes write append-only audit rows | ✅ Pass |
| IV. AI Provider Independence & Tool-Mediated Access | This feature *is* the human-escalation capability Principle IV names: the escalations application service is the tool the future AI subsystem will call (never the DB), and the escalated flag is the contract that silences the AI (FR-002a). Interim HTTP endpoint documents itself as that integration point | ✅ Pass |
| V. API-First & Contract Consistency | All endpoints in `contracts/rest-api.md` with the standard envelope, cursor pagination (queue list), and error vocabulary; claim is idempotent-safe by CAS semantics (second identical claim → 409 with the current assignee); SSE event schema versioned in `contracts/events.md` | ✅ Pass |
| VI. Observability by Default | Routing decisions produce structured audit rows (reason, matched skills, candidate load) forming the inspectable escalation-decision timeline Principle VI requires; SSE connects/disconnects and presence reverts traced; request-id middleware applies to all new routes | ✅ Pass |
| VII. Test-First & Regression Discipline | FR-024 makes routing-branch coverage a functional requirement; plan includes unit tests on ranking, integration tests per branch incl. concurrency (contended claim) and the cross-tenant matrix, schema tests, and Vitest specs per story | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migrations 0035–0037 only; UUID PKs, timestamps, composite parent-tenant FKs per 0027 convention; partial unique index enforces one active escalation per conversation; every production query path (candidate selection, load count, queue page, skills lookup) gets a dedicated index; `agent_skills` join table omits soft-delete like other pure join/append tables (see Complexity Tracking) | ⚠️ Justified deviation |
| IX. Design System Discipline | Queue page, banner, toggle, and skill chips compose existing shared components; new visuals (availability dot, escalation banner) built once as shared/project components before feature use; no raw Taiga styling in feature pages | ✅ Pass |
| X. Performance & Efficiency | Candidate selection is one statement (skill-match count + load in a single ranked query); queue drain assigns one conversation per free agent per pass, re-evaluating load (spec edge case); SSE uses streaming (Principle X names streaming where the pattern benefits); no polling loops — drain is triggered by the events that change eligibility | ✅ Pass |

**Initial gate**: PASS — one justified deviation recorded in Complexity Tracking.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no new deviations. Nuanced calls, all grounded in clarifications: (1) presence lives in process memory, not the DB — after a restart the registry is empty, agents reconnect within the SSE retry window, and a startup sweep reverts stale `available` rows, keeping FR-017a truthful; (2) `required_skill_names` is denormalized onto escalations as a history snapshot (FR-019) while live matching uses `required_skill_ids ∩ agent_skills` — deletion semantics stay clean without losing audit fidelity; (3) the `escalated` inbox filter reads `conversations.escalated_at`, written only through the conversations crate's own interface, preserving module ownership of the inbox statement.

## Project Structure

### Documentation (this feature)

```text
specs/014-human-handoff-routing/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── rest-api.md      # Escalation/queue/claim/skills/availability endpoints, payloads, errors, audit actions
│   ├── events.md        # SSE stream contract, event schemas, presence semantics, outbox event contract
│   └── permissions.md   # Reused permission codes, route→permission map, page permissions
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0035_agent_skills.sql               # NEW — skills catalog + agent_skills join + indexes
│   ├── 0036_agent_availability.sql         # NEW — per-membership availability state (default away)
│   └── 0037_escalations.sql                # NEW — escalations table, one-active partial unique index,
│                                           #        conversations.escalated_at, load-count index, outbox event types
└── crates/
    ├── modules/
    │   ├── escalations/
    │   │   ├── Cargo.toml                  # MODIFIED — real deps (axum, sqlx, serde, tokio, conversations, tenancy, authz, kernel)
    │   │   └── src/
    │   │       ├── lib.rs                  # MODIFIED — module docs (Purpose/Responsibilities/Interfaces/Extension points), exports
    │   │       ├── model.rs                # NEW — Escalation, Skill, Availability, RoutingReason, payloads, validation
    │   │       ├── routing.rs              # NEW — routing service: candidate selection, ranking, advisory-lock txn, queue drain
    │   │       ├── presence.rs             # NEW — per-(tenant,membership) connection registry, grace-window auto-revert, startup sweep
    │   │       ├── events.rs               # NEW — SSE handler + broadcast fan-out; outbox consumer for conversation events
    │   │       ├── routes.rs               # NEW — escalate, queue list, claim, availability get/put, skills CRUD, member-skills put
    │   │       ├── queries.rs              # NEW — SQL: candidate selection, load counts, queue page (keyset), skills, availability
    │   │       └── audit.rs                # NEW — escalation.* / skill.* / availability.changed audit helpers
    │   └── conversations/
    │       └── src/
    │           ├── lib.rs                  # MODIFIED — exports new public tx interfaces
    │           ├── queries.rs              # MODIFIED — assign_in_tx, set_escalated_in_tx, escalated inbox filter predicate
    │           ├── routes.rs               # MODIFIED — inbox `escalated` filter param; detail embeds escalation via escalations public query
    │           └── outbox.rs               # NEW — emit conversation.status_changed / assignment_changed events in write txns
    ├── shared/
    │   └── db/tests/schema.rs              # MODIFIED — 0035–0037 schema assertions (FKs, CHECKs, partial unique, indexes)
    └── server/
        ├── src/
        │   ├── router.rs                   # MODIFIED — escalations routes via .guarded()/.guarded_with_methods() under mount_tenant; SSE route
        │   └── state.rs                    # MODIFIED — escalations runtime (broadcast registry + presence) added to AppState
        └── tests/
            ├── rbac.rs                     # MODIFIED — new routes in the route→permission map
            └── escalations.rs              # NEW — full routing-branch, queue, claim-contention, presence, SSE, isolation, audit suite

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts            # MODIFIED — Escalation, Skill, Availability, RoutingReason, queue/event types
│   ├── realtime/
│   │   ├── realtime.service.ts             # NEW — fetch-based SSE Observable (credentials + X-Tenant-ID), retry/backoff, event demux
│   │   └── notifications.service.ts        # NEW — browser Notification permission/display + in-app notification signals
│   └── router/
│       ├── app-paths.ts                    # MODIFIED — tenant.escalations path
│       └── page-title.ts                   # MODIFIED — escalation queue title
├── layout/topbar/
│   └── availability-toggle.component.ts    # NEW — available/away control + presence-fed state (composed into topbar)
├── shared/components/
│   └── availability-dot/…                  # NEW — small presence/availability indicator used by topbar, queue, member lists
└── features/tenant/
    ├── tenant.routes.ts                    # MODIFIED — escalations route (conversations.view)
    ├── escalations/
    │   ├── escalations-api.service.ts      # NEW — queue list, claim, availability, skills endpoints (Observable)
    │   ├── escalation-queue.store.ts       # NEW — queue SignalStore (entries, waiting times, claim state, live updates)
    │   ├── escalation-queue.component.ts   # NEW — queue page (data-table, reason/skills/waiting, claim action, empty state)
    │   └── escalation-banner.component.ts  # NEW — banner for conversation detail (escalated when/why + routing reason)
    ├── conversations/
    │   ├── conversations.component.ts      # MODIFIED — `escalated` filter chip
    │   ├── conversations.store.ts          # MODIFIED — escalated filter state
    │   └── conversation-detail.component.ts# MODIFIED — embeds escalation banner + routing reason near assignee control
    └── team/
        ├── team-api.service.ts             # MODIFIED — skills catalog + member-skills endpoints
        ├── skills-manager.component.ts     # NEW — catalog CRUD + per-agent skill assignment (members.manage)
        └── team-list.component.ts          # MODIFIED — skill chips + availability dot per member
```

**Structure Decision**: Backend follows the module-ownership rule: the `escalations` crate owns all new tables and the routing/presence/SSE runtime, depending one-way on `conversations` (two new public tx interfaces) and consuming conversation changes via the existing transactional outbox — `conversations` never imports `escalations`; the conversation-detail handler embeds escalation data through an escalations-owned public query function passed the open transaction. The SSE endpoint lives under `mount_tenant` so tenant-context middleware applies unchanged. Frontend follows spec-002 layering: transport-level realtime in `core/realtime` (singleton, no feature deps), the queue as a lazy feature area, availability toggle in `layout/topbar`, and all new visuals as shared/project components before feature use (Constitution IX).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `agent_skills` join table omits `updated_at`/`deleted_at`/`set_updated_at` (deviation from 005 table conventions) | Rows are pure links that are only inserted and hard-deleted (FR-018/FR-019: skill removal takes effect immediately; history is preserved by audit rows and the `required_skill_names` snapshot, not by soft-deleted links) | Soft-deleting links would make every routing candidate-selection query carry `deleted_at IS NULL` re-add semantics (re-assigning a skill would need undelete-or-insert logic) for no reader benefit; mirrors the `messages`/`audit_logs` precedent that append/link tables carry only the columns their lifecycle uses |
| In-process presence registry + tokio broadcast instead of Redis pub/sub (stack lists Redis as the shared-state tool) | The backend is a single-process modular monolith today; an in-process registry gives sub-second FR-025 delivery with zero new operational surface, and the escalations module hides it behind its own interface | Redis pub/sub + presence keys would add cross-service machinery the deployment shape doesn't need yet; the registry is confined to `escalations::presence`/`events`, so swapping in Redis when the module is extracted is an implementation change behind the same interface (documented in research.md) |
