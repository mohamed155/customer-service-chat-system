# Implementation Plan: Conversation Core

**Branch**: `013-conversation-core` | **Date**: 2026-07-13 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/013-conversation-core/spec.md`

## Summary

Turn the minimal conversation summary shipped by 012 into the real conversation system: tenant-scoped conversations with the fixed status set (open/pending/resolved/closed), single-assignee assignment, immutable plain-text messages in three kinds (customer-authored — optionally manually logged by a member —, team reply, internal note), a filterable inbox, a chronological timeline, and a composer. Backend adds six tenant endpoints (list, create, view, timeline, add message, patch status/assignment); frontend upgrades the fixture-driven `features/tenant/conversations/` area to live data with an inbox, detail page, timeline, and mode-switching composer.

Technical approach: **no new permission codes and no matrix changes** — the 008 catalog already defines `conversations.view`/`conversations.manage` and the matrix already grants manage to Agent-and-above with Viewer read-only, exactly matching the clarified requirement. The `conversations` module crate (currently one read-only history query) gains routes, models, queries, and audit helpers. Migration `0033` evolves the existing `conversations` table: status CHECK widened to the spec vocabulary (`escalated` rows remapped to `open`), a nullable `assigned_membership_id` with a composite FK to `tenant_memberships(tenant_id, id)` (cross-tenant assignment impossible at the DB level), and inbox indexes. Migration `0034` creates the append-only `messages` table (kind CHECK, composite FK to `conversations(tenant_id, id)`, `seq` identity column for deterministic ordering, 10k-char body CHECK). Inbox list is a single keyset-paginated statement with a LATERAL latest-message preview; the timeline is keyset-paginated newest-first. Auto-reopen on customer-facing message and all status/assignment changes write audit rows in the same transaction. The 012 customer-profile history endpoint keeps its shape (only the status vocabulary changes). Frontend replaces conversation fixtures with an Observable API service + two SignalStores (inbox, detail), a `conversations/:id` route, and composes existing shared components; the assignee picker reuses the existing `GET /tenant/members` endpoint.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum, SQLx (PostgreSQL), existing `authz`/`tenancy`/`identity` crates and the deny-by-default `.guarded()` router builder; `conversations` module crate graduates from its single read query (012) to the feature's owner; `customers` crate used only through its public `customer_exists_in_tx` interface; Angular Router, Reactive Forms, NgRx SignalStore, existing `core/authz` + shared components (status-badge, channel-badge, select-filter, empty-state, loading-state, avatar, dialog-shell, toolbar, section-header); RxJS operators for all new async flows (constitution v1.2.0)

**Storage**: PostgreSQL — migration `0033` alters `conversations` (status CHECK → `open|pending|resolved|closed` with `escalated`→`open` remap; adds `assigned_membership_id UUID NULL` + composite FK `(tenant_id, assigned_membership_id) → tenant_memberships(tenant_id, id)`; inbox index `(tenant_id, status, last_activity_at DESC, id DESC) WHERE deleted_at IS NULL` + assignee variant); migration `0034` creates `messages` (tenant-owned, composite FK `(tenant_id, conversation_id) → conversations(tenant_id, id)`, `kind` CHECK `customer|reply|note`, sender/logged-by membership references with kind-consistency CHECKs, `body` CHECK 1–10,000 chars, `seq BIGINT GENERATED ALWAYS AS IDENTITY` for stable ordering, timeline index `(tenant_id, conversation_id, created_at DESC, seq DESC)`)

**Testing**: `cargo test` — new live-gated suite `backend/crates/server/tests/conversations.rs` (inbox list/filters/default-open, create, detail, timeline ordering + same-instant stability, reply/note/logged-message, auto-reopen, status/assignment changes + audit rows, inactive-assignee 422, viewer 403, per-operation cross-tenant 404 matrix per FR-019) + `rbac.rs` route→permission additions + `shared/db/tests/schema.rs` assertions for 0033/0034; Vitest for API service, both SignalStores, inbox page, detail page, and composer specs

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — existing Cargo workspace + Angular pnpm workspace

**Performance Goals**: Inbox is one statement (tenant + status/assignee/channel predicates + keyset cursor + LATERAL latest-message preview over the timeline index) — no N+1; timeline is one keyset-paginated statement; SC-002 (<1s at 10k conversations/tenant, recent timeline <1s at 1k messages) served by the two new partial btree indexes; writes add only the audit insert (and, for auto-reopen, one status UPDATE) inside the same transaction

**Constraints**: Deny-by-default routing (`.guarded()` with required permission) under `mount_tenant` so tenant-context middleware enforces isolation pre-handler; cross-tenant reads/writes answered `not_found` (FR-016 — never confirm existence); 401/403/404/422 from the existing `kernel::ApiError` vocabulary with field-level details on 422; messages immutable (no UPDATE/DELETE paths); schema changes via migration only (Constitution VIII); last-write-wins concurrency for status/assignment (spec edge case — no version column); RxJS-first frontend async; route paths only via `APP_PATHS`; no raw Taiga styling in feature pages

**Scale/Scope**: 2 migrations; 0 new permission codes; 6 tenant-scoped endpoints (5 new + 1 patched existing contract); 1 module crate gains full ownership; frontend: 1 fixture page upgraded (inbox), 1 new detail page, 1 new-conversation dialog, 1 API service, 2 SignalStores; audit vocabulary +3 actions (`conversation.created`, `conversation.status_changed`, `conversation.assignment_changed`)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | All conversation/message logic lives in the `conversations` module crate; customer existence is checked only via the `customers` crate's public interface (unchanged one-way dependency); member/assignee validation queries `tenant_memberships` through a query owned by the conversations module scoped by the middleware tenant; router composition stays in `server` | ✅ Pass |
| II. Multi-Tenant Isolation | `messages` carries `tenant_id`; composite FKs make cross-tenant messages and cross-tenant assignments unrepresentable at the DB level; every query filters by the middleware-resolved tenant; cross-tenant access → `not_found`; per-operation isolation tests required by FR-019 | ✅ Pass |
| III. Zero-Trust Security & RBAC | Reuses `conversations.view`/`conversations.manage` through deny-by-default `.guarded()` registration; server-side enforcement independent of UI hiding; creation, status changes (including auto-reopen), and assignment changes audited with actor/action/time (FR-017) | ✅ Pass |
| IV. AI Provider Independence | Not touched — no AI participation in this feature (spec assumption) | ✅ N/A |
| V. API-First & Contract Consistency | Endpoints documented in `contracts/rest-api.md`; cursor pagination + error envelope reused; PATCH is partial-update; message POST is append-only and idempotency-safe per its semantics (client retries create new immutable entries by design, documented) | ✅ Pass |
| VI. Observability by Default | Request-id/tracing middleware unchanged and applies to new routes; audit trail append-only; auto-reopen recorded as a status-change audit row so the timeline of state changes is inspectable | ✅ Pass |
| VII. Test-First & Regression Discipline | Dedicated integration suite with per-operation isolation matrix and ordering-stability tests; rbac matrix extension; schema tests for both migrations; Vitest specs per story | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration-only; FKs + CHECKs mirror existing conventions (UUID PK, composite parent-tenant FKs per 0027); indexes defined for every production query path; `messages` omits `updated_at`/`deleted_at` as an append-only table (see Complexity Tracking) | ⚠️ Justified deviation |
| IX. Design System Discipline | Inbox and detail pages compose existing shared components (status-badge, channel-badge, select-filter, empty-state, loading-state, avatar, dialog-shell); status-badge gains the pending/resolved variants once; composer built as a project component; no raw Taiga styling in feature pages | ✅ Pass |
| X. Performance & Efficiency | Single-statement inbox with LATERAL preview (no N+1); keyset (not offset) pagination for inbox and timeline; audit insert and auto-reopen share the write transaction; timeline index serves both newest-first pages and the LATERAL preview | ✅ Pass |

**Initial gate**: PASS — one justified deviation recorded in Complexity Tracking.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no new deviations. Three nuanced calls, all grounded in clarifications: (1) `escalated` status rows are remapped to `open` in 0033 — escalation semantics belong to the future escalations module, and existing rows are dev/test-seeded only; (2) participants are derived from message senders plus the customer (no participants table) — FR-004 requires tracking who was involved, which the message log already records losslessly; (3) manually logged customer messages record both the customer as author and the logging member (`logged_by_membership_id`), satisfying the Q4 authorship-integrity clarification without a separate message kind.

## Project Structure

### Documentation (this feature)

```text
specs/013-conversation-core/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── rest-api.md      # Conversation/message endpoints, representations, errors, audit actions
│   └── permissions.md   # Reused permission codes, route→permission map, page permissions
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0033_conversation_core.sql          # NEW — status vocabulary + assignee column/FK + inbox indexes
│   └── 0034_messages.sql                   # NEW — append-only messages table + timeline index
└── crates/
    ├── modules/
    │   └── conversations/
    │       ├── Cargo.toml                  # MODIFIED — adds argon-free deps already used by sibling modules (serde_json for audit, etc.)
    │       └── src/
    │           ├── lib.rs                  # MODIFIED — module docs rewritten (full owner), exports; history handler stays
    │           ├── model.rs                # NEW — Conversation, ConversationDetail, Message, payloads, status/kind enums, validation
    │           ├── routes.rs               # NEW — inbox list, create, detail, timeline, add-message, patch handlers
    │           ├── queries.rs              # NEW — SQL: inbox w/ LATERAL preview, timeline keyset, participants, active-member check
    │           └── audit.rs                # NEW — conversation.created / status_changed / assignment_changed helpers
    ├── shared/
    │   └── db/tests/schema.rs              # MODIFIED — 0033/0034 schema assertions (CHECKs, FKs, indexes, identity column)
    └── server/
        ├── src/router.rs                   # MODIFIED — /tenant/conversations routes via .guarded() under mount_tenant
        └── tests/
            ├── rbac.rs                     # MODIFIED — conversation routes in the route→permission map
            └── conversations.rs            # NEW — inbox/filters/create/timeline/compose/status/assign/isolation/audit suite

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts            # MODIFIED — Conversation, ConversationDetail, Message, payloads, filter/query types
│   └── router/
│       ├── app-paths.ts                    # MODIFIED — tenant.conversationDetail path
│       └── page-title.ts                   # MODIFIED — conversations subtitle de-fixtured; conversation detail title
├── shared/components/status-badge/…        # MODIFIED — pending/resolved conversation-status variants (once)
└── features/tenant/
    ├── tenant.routes.ts                    # MODIFIED — conversations/:id child route (conversations.view)
    └── conversations/
        ├── conversations-api.service.ts    # NEW — Observable API access (list/create/get/timeline/message/patch + members lookup)
        ├── conversations.store.ts          # MODIFIED — fixture SignalStore → live inbox store (filters, cursor, items, loading)
        ├── conversations.component.ts      # MODIFIED — fixture page → live inbox (filters, badges, pagination, new-conversation action)
        ├── inbox-list.component.ts         # MODIFIED — fixture rows → live rows (preview, assignee, badges, empty state)
        ├── conversation-detail.store.ts    # NEW — detail SignalStore (conversation, timeline pages, composer state)
        ├── conversation-detail.component.ts# NEW — header (status/assignee controls) + timeline + composer layout
        ├── conversation-thread.component.ts# MODIFIED — fixture thread → live timeline (kinds, note styling, load-older)
        ├── composer.component.ts           # NEW — reply / internal note / log-customer-message modes, validation
        └── new-conversation-dialog.component.ts # NEW — customer + channel + first message form
```

**Structure Decision**: Backend follows the module-ownership rule established in 012 — the `conversations` crate becomes the sole owner of conversation and message data access, keeping its one-way dependency on `customers` (existence checks only) and adding no reverse dependencies. Route registration stays in `server/router.rs` using the deny-by-default builder under `mount_tenant`. Frontend upgrades the existing `features/tenant/conversations/` folder in place (spec-002 layering: feature-scoped service + SignalStores; shared components for all visuals), touching `core/` only for models, paths, and titles; the old `customer-panel.component.ts` fixture sidebar is folded into the detail page using live customer data from the conversation payload.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `messages` omits `updated_at`, `deleted_at`, and the `set_updated_at` trigger (deviation from the 005 table conventions) | Messages are immutable once created (spec FR-006) and are never edited or deleted in this feature; the table is append-only by design, mirroring the existing `audit_logs` precedent | Carrying soft-delete and update-tracking columns on a table with no update or delete path invites accidental mutation paths and misleads readers about the row lifecycle; adding the columns later via migration is trivial if a future feature introduces redaction/editing |
