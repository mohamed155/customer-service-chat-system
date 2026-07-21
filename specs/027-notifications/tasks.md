---
description: "Task list for 027-notifications"
---

# Tasks: Notifications

**Input**: Design documents from `/specs/027-notifications/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/notifications-api.md](contracts/notifications-api.md), [quickstart.md](quickstart.md)

**Tests**: Included and **ordered first within each phase**. Constitution Principle VII (Test-First) requires unit, integration, API, and end-to-end coverage. Within every user-story phase below, the test tasks are listed and numbered *before* the implementation tasks they cover — write the failing test, then make it pass. The mandatory `speckit.solid.apply` pre-hook enforces this discipline at implement time.

**End-to-end coverage**: automated coverage is unit + integration (`cargo test`) and component specs (`pnpm ng test`). The end-to-end category is satisfied by the scripted manual walkthrough in [quickstart.md](quickstart.md), executed as T050. This is deliberate: the two properties that most need E2E validation here — the SSE badge updating live, and an assignment still succeeding while the worker is stopped — depend on a running multi-process stack that the test harness does not model.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 / US2 / US3 — maps to the user stories in spec.md

---

## ⚠️ Read this before starting any task

These six rules prevent the most likely mistakes. They apply to every phase.

1. **Phase 1 is strictly sequential — do not parallelize it.** The crate rename must fully complete (including every reference and the workspace member list) before the new `notifications` crate is created, or the workspace will not compile. Migration `0054` must exist before any query task.

2. **The badge SETs, never increments.** Both SSE payloads carry `unreadCount`. Assign it: `unreadCount.set(payload.unreadCount)`. Never `update(n => n + 1)`. The existing code in `core/realtime/notifications.service.ts` uses the increment pattern — that code is being **deleted**, not copied. `presence::broadcast` is fire-and-forget (`let _ = tx.send(event)`), so a dropped event must self-correct when the next one arrives; incrementing would desync the badge permanently.

3. **Never notify the actor.** Every create path drops `actorMembershipId` from the recipient set (FR-009).

4. **Notification writes must never fail the caller.** Emitting is an INSERT into `outbox_events`. Where a transaction is in scope, join it. Where one is not, log the error and continue — never propagate it (FR-017).

5. **Two namespaces that look alike — do not cross-wire them.** Internal outbox `event_type` values are `notification.requested` / `notification.resolved`. Browser-facing SSE event names are `notification.created` / `notification.cleared`. They are different transports with different payloads.

6. **A membership is only valid when `status = 'active' AND deleted_at IS NULL`.** Member removal in this codebase is a **status change**, not a soft delete (`tenancy/src/members.rs:578`), and the widely-copied lookup at `escalations/src/routes.rs:461-463` filters on `deleted_at` **only**. Copying that pattern verbatim would let a removed member keep reading their inbox. Every membership lookup and every recipient query in this feature MUST filter on both columns.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Free the crate name, create the new crate, create the table. Blocking for everything.

**These tasks are sequential. No `[P]` markers in this phase by design.**

- [X] T001 Rename the existing email-transport crate directory `backend/crates/modules/notifications/` to `backend/crates/modules/email/`. Do not change any file contents in this task.

- [X] T002 In `backend/crates/modules/email/Cargo.toml`, change `name = "notifications"` to `name = "email"`. Leave all dependencies unchanged.

- [X] T003 Update the two dependents of the renamed crate: in `backend/crates/server/Cargo.toml` and `backend/crates/modules/tenancy/Cargo.toml`, change `notifications = { path = "../modules/notifications" }` / `notifications = { path = "../notifications" }` to `email = { path = "../modules/email" }` / `email = { path = "../email" }` respectively (keep each file's existing relative-path style). Also update the workspace `members` list in `backend/Cargo.toml` if it names the crate path explicitly.

- [X] T004 Replace every `notifications::` reference with `email::` in `backend/crates/modules/tenancy/src/invitations.rs` (~10 occurrences: `EmailDeliveryStatus`, `EmailSender`, `EmailMessage`), in `backend/crates/server/src/main.rs` (the `EmailSender` wiring), and in `backend/crates/server/tests/team_members.rs` (~8 occurrences). Then run `cd backend && cargo check --workspace` — it MUST compile with zero errors before continuing. Any remaining reference is a compile error, so a clean build is proof the rename is complete.

- [X] T005 Run `cd backend && cargo test -p server --test team_members` to confirm the rename broke nothing. This suite is the regression guard for the rename; it must pass before proceeding.

- [X] T006 Create the new crate at `backend/crates/modules/notifications/` with a `Cargo.toml` declaring `name = "notifications"` and workspace dependencies: `async-trait`, `axum`, `chrono`, `serde`, `serde_json`, `sqlx`, `tracing`, `utoipa`, `uuid`, plus path deps on `authz`, `identity`, and `tenancy`. Add it to the workspace `members` in `backend/Cargo.toml`. Create `src/lib.rs` with module docs (Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, Extension Points — per the constitution's documentation rule) and empty `pub mod` declarations for `model`, `emit`, `recipients`, `queries`, `worker`, `routes`.

- [X] T007 Create migration `backend/migrations/0054_notifications.sql` creating the `notifications` table and its five indexes exactly as specified in [data-model.md](data-model.md) ("Table: `notifications`" and "Indexes"). Include: all 14 columns with the three CHECK constraints on `kind`, `state`, and `subject_type`; FK `tenant_id` → `tenants(id)`; FK `recipient_membership_id` → `tenant_memberships(id)` ON DELETE CASCADE; and the five indexes `notifications_dedupe_uq`, `notifications_inbox_idx`, `notifications_unread_idx`, `notifications_resolve_idx`, `notifications_retention_idx`. Apply it and confirm it runs cleanly. Note: the CASCADE will rarely fire in practice because memberships are deactivated rather than deleted (rule 6) — it is defence-in-depth, not the mechanism that makes a removed member's inbox unreachable.

**Checkpoint**: `cargo check --workspace` passes, `team_members` tests pass, table exists.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The model, emit helpers, recipient resolution, queries, and worker. Every user story depends on all of this.

**⚠️ No user story work can begin until this phase is complete.**

### Model and emit

- [X] T008 [P] In `backend/crates/modules/notifications/src/model.rs`, define: `NotificationKind` enum (`EscalationNew`, `ConversationAssigned`, `AiResponseFailed`, `ToolApprovalRequired`) serializing to the exact strings `escalation.new`, `conversation.assigned`, `ai.response_failed`, `tool.approval_required`; `NotificationState` enum (`Unread`, `Read`, `Resolved`) serializing to `unread`, `read`, `resolved`; `SubjectType` enum (`Conversation`, `Escalation`, `ToolRequest`) serializing to `conversation`, `escalation`, `tool_request`; a `NotificationRow` sqlx `FromRow` struct matching the table; and the `NotificationDto`, `NotificationActorDto`, and `NotificationListResponse` response types with `#[serde(rename_all = "camelCase")]` and `utoipa::ToSchema`, shaped exactly as the `200` response in [contracts/notifications-api.md](contracts/notifications-api.md).

- [X] T009 In `backend/crates/modules/notifications/src/emit.rs`, implement four functions that insert into `outbox_events` using the payload shapes in [data-model.md](data-model.md) ("Outbox event contracts"):
  - `emit_requested_in_tx(tx: &mut Transaction<'_, Postgres>, req: &NotificationRequest) -> sqlx::Result<()>`
  - `emit_requested_on_pool(pool: &PgPool, req: &NotificationRequest)` — returns `()`, logs errors with `tracing::error!` and swallows them (rule 4)
  - `emit_resolved_in_tx(tx, tenant_id, subject_type, subject_id, resolved_by: Option<Uuid>) -> sqlx::Result<()>`
  - `emit_resolved_on_pool(pool, …)` — same error-swallowing behavior

  Insert with `event_type = 'notification.requested'` / `'notification.resolved'`, `aggregate_type = 'notification'`, `aggregate_id = subject_id`. Define the `NotificationRequest` struct here with fields: `tenant_id`, `kind`, `subject_type`, `subject_id`, `actor_membership_id: Option<Uuid>`, `target_membership_id: Option<Uuid>`, `dedupe_key: String`, `title: String`, `body: Option<String>`.

- [X] T010 [P] In `backend/crates/modules/notifications/src/emit.rs`, add a `dedupe_key` builder function per kind, producing exactly the formats in [research.md](research.md) R4: `escalation:<escalation_id>`, `assigned:<conversation_id>:<assigned_membership_id>`, `tool_approval:<tool_request_id>`, and `ai_failed:<conversation_id>:<bucket>` where `bucket = unix_timestamp_seconds / 900` (integer division — this is what enforces the 15-minute suppression window). Add unit tests asserting each format, including that two timestamps 5 minutes apart produce the same `ai_failed` key and two 20 minutes apart produce different keys.

### Recipient resolution

- [X] T011 In `backend/crates/modules/notifications/src/recipients.rs`, implement `resolve(pool, tenant_id, kind, subject_id, actor_membership_id, target_membership_id) -> sqlx::Result<Vec<Uuid>>` returning recipient membership ids per the table in [data-model.md](data-model.md) ("Recipient resolution"):
  - If `target_membership_id` is `Some(m)`, the result is just `[m]` — still subject to the active-membership filter below.
  - Otherwise for `EscalationNew` and `ToolApprovalRequired`: all memberships in the tenant whose role grants `Permission::ConversationsManage` (use `authz::matrix::tenant_role_permissions`; under the current matrix that is Owner, Admin, Manager, Agent).
  - Otherwise for `AiResponseFailed`: all Owner and Admin memberships, plus the conversation's `assigned_membership_id` if set.

  Two filters apply to every branch: exclude `actor_membership_id` (rule 3), and include only memberships with `status = 'active' AND deleted_at IS NULL` (rule 6). Add unit tests for actor exclusion, role filtering, and exclusion of a deactivated membership.

### Queries

- [X] T012 In `backend/crates/modules/notifications/src/queries.rs`, implement the write paths:
  - `fan_out(pool, req: &NotificationRequest, recipients: &[Uuid]) -> sqlx::Result<u64>` — a **single** set-based `INSERT INTO notifications (…) SELECT … FROM UNNEST($n::uuid[])` over the recipient array, ending in `ON CONFLICT (recipient_membership_id, dedupe_key) DO NOTHING`. Do not loop per recipient (Principle X). Returns rows actually inserted.
  - `resolve_subject(pool, tenant_id, subject_type, subject_id, resolved_by: Option<Uuid>) -> sqlx::Result<Vec<Uuid>>` — runs exactly the `UPDATE` in [data-model.md](data-model.md) ("`notification.resolved`") and returns the affected `recipient_membership_id`s via `RETURNING` so the worker can broadcast to each. The `state = 'unread'` predicate is required: it prevents rewriting rows the user already read.

- [X] T013 In `backend/crates/modules/notifications/src/queries.rs`, implement the read paths, every one of which filters on **both** `tenant_id` and `recipient_membership_id`:
  - `list(pool, tenant_id, membership_id, state: Option<NotificationState>, cursor, limit) -> sqlx::Result<(Vec<NotificationRow>, Option<String>)>` — newest first, cursor pagination using the same opaque `(created_at, id)` encode/decode approach as `backend/crates/modules/audit/src/queries.rs`. Reuse that cursor codec pattern rather than inventing a new one.
  - `unread_count(pool, tenant_id, membership_id) -> sqlx::Result<i64>`
  - `mark_read(pool, tenant_id, membership_id, notification_id) -> sqlx::Result<Option<NotificationRow>>` — sets `state = 'read'`, `read_at = now()`; returns `None` when the row does not exist **or** belongs to another member (the caller turns `None` into `404`, never `403` — see the contract).
  - `mark_all_read(pool, tenant_id, membership_id) -> sqlx::Result<u64>`

### Worker

- [X] T014 In `backend/crates/modules/notifications/src/worker.rs`, implement `process_notification_outbox_once(pool, presence: &Arc<escalations::presence::Runtime>) -> sqlx::Result<bool>`, modelled closely on `backend/crates/modules/escalations/src/events.rs:249-283`:
  1. Claim one row: `UPDATE outbox_events SET claimed_at = now(), claim_token = $1 WHERE id = (SELECT id FROM outbox_events WHERE event_type IN ('notification.requested','notification.resolved') AND claimed_at IS NULL ORDER BY created_at ASC LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, tenant_id, event_type, payload`. Return `Ok(false)` when nothing is claimed.
  2. For `notification.requested`: call `recipients::resolve`, then `queries::fan_out`, then broadcast one `notification.created` SSE event per recipient (T015).
  3. For `notification.resolved`: call `queries::resolve_subject`, then broadcast one `notification.cleared` SSE event per affected recipient.
  4. `DELETE FROM outbox_events WHERE id = $1` when done.

  **`unreadCount` in each broadcast is that recipient's own count.** Call `queries::unread_count(pool, tenant_id, recipient)` per recipient after the fan-out/resolve and put that value in that recipient's event. Do **not** broadcast the number of rows inserted, and do **not** compute one shared count for everyone — recipients have different unread totals, `fan_out` uses `ON CONFLICT DO NOTHING` so some recipients get no new row at all, and the frontend badge assigns this value directly (rule 2). A shared or insert-count value desyncs every badge but one.

  **Do not widen the `event_type` filter** to include any existing event type — those rows belong to other consumers, which delete them; see [research.md](research.md) R1.

- [X] T015 In `backend/crates/modules/escalations/src/presence.rs`, add two variants to `pub enum Event`: `NotificationCreated(NotificationBadgeEvent)` and `NotificationCleared(NotificationBadgeEvent)`, where `NotificationBadgeEvent` is defined in that same file with **primitive fields only** — `membership_id: Uuid`, `notification_id: Option<Uuid>`, `unread_count: i64`, serialized `#[serde(rename_all = "camelCase")]`. Do **not** add a dependency from `escalations` to `notifications`; primitives keep the module graph acyclic.

- [X] T016 In `backend/crates/modules/escalations/src/events.rs`, add two match arms to `GuardedStream::poll_next` for the new variants, copying the per-member filtering pattern already used by `presence::Event::AvailabilityChanged` at lines 64-78: emit only when `ev.membership_id == self.membership_id`, otherwise `cx.waker().wake_by_ref()` and return `Poll::Pending`. Use SSE event names `notification.created` and `notification.cleared` (not the outbox names — rule 5).

- [X] T017 In `backend/crates/modules/notifications/src/worker.rs`, add `run_notification_outbox_worker(pool, presence) -> !` looping over `process_notification_outbox_once`, sleeping 1s on `Ok(false)` and 5s on `Err`, logging errors with `tracing::error!` — mirroring `run_escalation_outbox_worker` at `escalations/src/events.rs:499-515`. Log once at startup: `notifications outbox worker started`.

- [X] T018 In `backend/crates/server/src/main.rs`, spawn the worker alongside the existing ones (`let notifications_worker = tokio::spawn(notifications::worker::run_notification_outbox_worker(state.db.clone(), state.escalations.clone()));`) and add a matching arm to the `tokio::select!` block that panics if it stops, following the existing `escalation_worker` arm. Add `notifications = { path = "../modules/notifications" }` to `backend/crates/server/Cargo.toml`.

**Checkpoint**: Worker runs, fan-out and resolve work when an outbox row is inserted by hand. No trigger sites wired yet.

---

## Phase 3: User Story 1 — See and act on my unread notifications (Priority: P1) 🎯 MVP

**Goal**: A member sees a badge, opens the bell, reads a list, clicks through, and marks read.

**Independent test**: Insert one notification row directly in the DB for a member, sign in as them, confirm the badge shows 1, the panel lists it, clicking navigates and clears the badge.

### Tests first (T019-T021)

Write these before the handlers in T022. They will not compile until T022 exists — that is expected; write them, watch them fail, then implement.

- [X] T019 [US1] Create `backend/crates/server/tests/notifications.rs` (DB-gated with `require_db_tests()`, following `audit_logs.rs`) with failing tests for: listing returns newest-first; cursor pagination; `unread-count` counts only `unread` (not `read`, not `resolved`); `mark_read` is idempotent; `mark_all_read` returns the count and is idempotent; **reading another member's notification id returns 404, not 403**; **tenant isolation** — a user belonging to tenants A and B sees only the active tenant's rows and counts; and **a deactivated membership (`status <> 'active'`) cannot read the inbox** (rule 6).

- [X] T020 [US1] In `backend/crates/server/tests/notifications.rs`, add a role-access test asserting that **all five tenant roles** (Owner, Admin, Manager, Agent, Viewer) receive `200` from `GET /tenant/notifications` and `GET /tenant/notifications/unread-count`. This encodes FR-012a — a regression that adds a permission gate must fail here.
  **Do not add rows to `backend/crates/server/tests/rbac.rs`.** That harness is a `&[(route, required_permission)]` tuple list (`TENANT_OPERATIONS`, lines 60-92) with no way to express "no permission required"; any row added there would assert a permission this route deliberately does not have and would expect 403s that never occur.

- [X] T021 [US1] In `backend/crates/server/tests/notifications.rs`, add a performance test for SC-004: seed 1,000 notifications for one membership, then assert `GET /tenant/notifications` (first page) returns in under 1 second. This is what proves `notifications_inbox_idx` is actually being used rather than assumed.

### Backend implementation (T022-T025)

- [X] T022 [US1] In `backend/crates/modules/notifications/src/routes.rs`, implement the four handlers from [contracts/notifications-api.md](contracts/notifications-api.md): `list_notifications` (GET `/tenant/notifications`), `get_unread_notification_count` (GET `/tenant/notifications/unread-count`), `mark_notification_read` (POST `/tenant/notifications/{id}/read`), `mark_all_notifications_read` (POST `/tenant/notifications/read-all`).
  Each handler resolves the caller's membership id for the current tenant from `TenantContext` + `Principal` using `SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL` — note the `status` predicate, which the widely-copied version of this query at `escalations/src/routes.rs:461-463` omits (rule 6).
  **When no active membership exists, return `400`** with the standard error envelope, per the contract's error table. This is the platform-user-switched-into-a-tenant case: platform users have no membership and are out of scope for notifications (spec Assumptions).
  Annotate every handler with `#[utoipa::path(...)]` including the exact `operation_id` values from the contract and `tag = "notifications"`. Return `404` (never `403`) when `mark_read` finds no row.

- [X] T023 [US1] In `backend/crates/server/src/router.rs`, register all four routes in the tenant router using the `routes!(…)` co-registration form used by neighbouring routes. **Do not attach `require_permission`** — per FR-012a the inbox has no permission gate; row-level filtering by membership is the authorization. Do not add `/test/tenant/...` routes for these endpoints (see T020).

- [X] T024 [P] [US1] In `backend/crates/server/src/openapi.rs`, register the notification DTO schemas (`NotificationDto`, `NotificationActorDto`, `NotificationListResponse`, and the count/marked response wrappers) in the components list alongside the existing `audit::model::*` entries at lines 157-160.

- [X] T025 [P] [US1] In `backend/crates/server/tests/openapi_coverage.rs`, add the four new endpoints to the `EXPECTED` inventory: `("GET", "/tenant/notifications")`, `("GET", "/tenant/notifications/unread-count")`, `("POST", "/tenant/notifications/{id}/read")`, `("POST", "/tenant/notifications/read-all")`. While there, confirm the check is **exhaustive** (fails on endpoints present in the router but absent from `EXPECTED`), not merely additive — if it is exhaustive, it is also what enforces FR-007a by making an added delete/dismiss endpoint fail the build.

### Frontend (T026-T035)

- [X] T026 [P] [US1] In `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, add `NotificationWire`, `NotificationListWire`, and mapper functions `notificationFromWire` / `notificationListFromWire`, following the existing `AuditEntryWire` / `auditListFromWire` pattern in the same file.

- [X] T027 [P] [US1] Create `frontend/apps/dashboard/src/app/shared/fixtures/notification.fixtures.ts` with typed sample notifications covering all four kinds and all three states, following `shared/fixtures/audit.fixtures.ts`.

- [X] T028 [US1] Create `frontend/apps/dashboard/src/app/core/notifications/notifications.api.ts` — a typed HTTP client with `list(state?, cursor?)`, `unreadCount()`, `markRead(id)`, `markAllRead()`, using the project's `ApiResponse<T>` types and RxJS observables (no Promises — constitution stack rule).

- [X] T029 [US1] Create `frontend/apps/dashboard/src/app/core/notifications/notifications.store.ts` as an NgRx SignalStore holding `items`, `unreadCount`, `loading`, `nextCursor`. Methods: `loadFirstPage()`, `loadMore()`, `refreshUnreadCount()`, `markRead(id)`, `markAllRead()`. **`setUnreadCount(n)` assigns the value — it must never increment** (rule 2). Call `refreshUnreadCount()` on store init. Include a `.spec.ts` asserting that two successive `setUnreadCount(3)` calls leave the count at 3, not 6.

- [X] T030 [US1] Rewrite `frontend/apps/dashboard/src/app/core/realtime/notifications.service.ts`: **delete the `inAppSignal` counter entirely**. Subscribe to `RealtimeService.events()`, filter for `notification.created` and `notification.cleared`, parse the payload, and call `store.setUnreadCount(payload.unreadCount)` — assignment only. On `notification.created` also refresh the list if the panel is open. Call `store.refreshUnreadCount()` whenever the SSE stream reconnects, so a dropped broadcast self-corrects. Keep the existing browser `Notification` API behavior but drive it from `notification.created` for all four kinds (see [research.md](research.md) R8).

- [X] T031 [P] [US1] Create `frontend/apps/dashboard/src/app/shared/components/notification-bell/` — a presentational component taking `count` as an input and emitting a `toggle` output. Renders the `@tui.bell` icon with a badge, hiding the badge when `count === 0`. Include an `.spec.ts`.

- [X] T032 [P] [US1] Create `frontend/apps/dashboard/src/app/shared/components/notification-list/` — a presentational component taking `items`, `loading`, and `hasMore` inputs and emitting `itemClick`, `markRead`, and `loadMore` outputs. Renders title, body, relative time, and a visual distinction for `unread` / `read` / `resolved` states (FR-011b). Includes empty and loading states. Used by both the topbar panel and the full page. Include an `.spec.ts`.

- [X] T033 [US1] Update `frontend/apps/dashboard/src/app/layout/topbar/topbar.component.ts`: replace the inline bell markup at lines 72-76 with `<app-notification-bell>` bound to `notificationsStore.unreadCount()`, add a dropdown panel rendering `<app-notification-list>`, and remove the `notificationsService.inAppSignal()` binding. Clicking an item marks it read and navigates via `APP_PATHS`. Update `topbar.component.spec.ts` accordingly.

- [X] T034 [US1] Add a notifications route: register the path in `frontend/apps/dashboard/src/app/core/router/app-paths.ts` and a title in `page-title.ts`, add the lazy route to `features/tenant/tenant.routes.ts`, and create `features/tenant/notifications/` with a page component rendering `<app-notification-list>` with pagination and a "mark all as read" action. No permission guard (FR-012a).

- [X] T035 [US1] Implement click-through navigation: map `subjectType` → route (`conversation` → conversation detail, `escalation` → the conversation behind it, `tool_request` → the conversation's tool panel) using `APP_PATHS` constants only — no string literals (frontend rule). When the target returns 404, show a "no longer available" message instead of navigating to an error page (FR-008 / SC-005).

**Checkpoint**: US1 is independently demonstrable with hand-seeded rows. This is the MVP.

---

## Phase 4: User Story 2 — Get told when work lands on me (Priority: P1)

**Goal**: Assignment and escalation events create real notifications, and claimed work auto-resolves for everyone else.

**Independent test**: Assign a conversation to member B → exactly B is notified. Queue an escalation → all `conversations.manage` holders notified; when one claims it, the others' badges drop without action.

### Tests first (T036)

- [X] T036 [US2] Extend `backend/crates/server/tests/notifications.rs` with failing tests for: assignment notifies only the new assignee; **self-assignment notifies nobody** (the FR-009 guard — see T039, where the two id-spaces meet); **escalation routing produces exactly one notification for the routed agent, not two** — assert by counting rows, which is the FR-009a / SC-010 regression guard; queued escalation fans out to all `conversations.manage` holders; claiming resolves the others' rows while leaving an already-`read` row as `read`; **auto-drain resolves the others' rows too** (the path most likely to be missed); and **replay dedup (SC-007)** — insert the same `notification.requested` outbox payload twice and assert exactly one row exists per recipient.

### Implementation (T037-T039)

- [X] T037 [US2] In `backend/crates/modules/escalations/src/routing.rs`, add two private helpers (see [research.md](research.md) R2a):
  - `notify_escalation_queued(tx, tenant_id, escalation_id, conversation_id, actor_membership_id)` — emits one `notification.requested` with `kind = escalation.new`, `subject_type = escalation`, `target_membership_id = None` (fan-out), `dedupe_key = escalation:<escalation_id>`.
  - `notify_escalation_assigned(tx, tenant_id, escalation_id, conversation_id, assignee_membership_id, actor_membership_id)` — emits `notification.requested` targeted at the assignee **and then** `notification.resolved` for that escalation with `resolved_by = assignee`, in that order.

  Both take the existing `&mut Transaction` so the emit is atomic with the escalation write. Keeping create and resolve inside one helper is deliberate: callers cannot wire one without the other.

- [X] T038 [US2] Call the helpers from all **five** escalation sites in `backend/crates/modules/escalations/src/routing.rs`:
  - `route_new_escalation_in_tx` assigned branch (~line 158, after `build_escalation`) → `notify_escalation_assigned`
  - `route_new_escalation_in_tx` queued branch (~line 195, after `build_escalation`) → `notify_escalation_queued`
  - `claim_in_tx` (~line 258, after `assign_in_tx`) → `notify_escalation_assigned` with `actor = claimant` (the create self-suppresses via FR-009; the resolve is the point)
  - `drain_one_for_membership_in_tx` (both candidate branches) → `notify_escalation_assigned`
  - `drain_any_in_tx` → `notify_escalation_assigned`

  **Missing either drain path leaves stale unread badges for the whole team** after an auto-drain — this is the single most likely omission in this feature, and T036 tests for it.

- [X] T039 [US2] In `backend/crates/modules/conversations/src/queries.rs`, inside `assign_in_tx` (after the existing `emit_assignment_changed_in_tx` call, ~line 969), emit a `notification.requested` with `kind = conversation.assigned`, `subject_type = conversation`, `target_membership_id = <new assignee>`, `dedupe_key = assigned:<conversation_id>:<assignee>`, subject to two guards:

  **Guard 1 — skip entirely when `origin == "escalations"`** (FR-009a). Escalation routing assigns through this same function, and the escalation notification already covers that case; emitting here too double-notifies. This mirrors the existing `origin == "escalations"` guard in `escalations/src/events.rs:276-283`.

  **Guard 2 — skip when the assignee is the actor** (FR-009). ⚠️ **This function mixes two id-spaces.** Its signature is `assign_in_tx(tx, tenant_id, conversation_id, assigned_membership_id: Option<Uuid>, actor_user_id: Option<Uuid>, origin)` — `assigned_membership_id` is a **membership** id and `actor_user_id` is a **user** id. Both are `Uuid`, so comparing them **compiles but is always false**, silently defeating this guard. You MUST first convert the actor to a membership id:

  ```sql
  SELECT id FROM tenant_memberships
   WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL
  ```

  Use that resulting **membership** id for two things: the self-assignment comparison against `assigned_membership_id`, and the `actor_membership_id` field of the `NotificationRequest` — the worker's `recipients::resolve` also excludes the actor, and it can only do so if that field holds a membership id. Never compare against, or pass through, `actor_user_id` directly. This is the only place in the feature where the two id-spaces meet.

**Checkpoint**: US2 works end-to-end on top of US1's UI.

---

## Phase 5: User Story 3 — Get told when the AI needs attention (Priority: P2)

**Goal**: Tool-approval requests and AI generation failures notify the right people.

**Independent test**: Trigger an approval-required tool call → all `conversations.manage` holders notified; one decides → others resolve. Force a generation failure → Owners/Admins and the assignee notified, at most once per 15 minutes per conversation.

### Tests first (T040)

- [X] T040 [US3] Extend `backend/crates/server/tests/notifications.rs` with failing tests for: an approval-required tool call notifies all `conversations.manage` holders; deciding it resolves the others' rows; three generation failures on one conversation within 15 minutes produce **exactly one** notification per recipient; and a failure in a later 15-minute bucket produces a second notification.

### Implementation (T041-T043)

- [X] T041 [US3] In `backend/crates/modules/ai/src/engine.rs`, at the point where a tool request is persisted with `status = "awaiting_approval"` (~line 687), call `notifications::emit::emit_requested_on_pool` with `kind = tool.approval_required`, `subject_type = tool_request`, `subject_id = <tool_request_id>`, `target_membership_id = None` (fan-out), `dedupe_key = tool_approval:<tool_request_id>`. Use the **pool** variant: this call site has no transaction in scope (see [research.md](research.md) R6), and the pool variant swallows errors so a notification failure cannot break generation.

- [X] T042 [US3] In `backend/crates/modules/ai/src/engine.rs`, at both sites where a `GenerationRecord` with `outcome: GenerationOutcome::Failed` is written (~line 1248 and ~line 1525), call `notifications::emit::emit_requested_on_pool` with `kind = ai.response_failed`, `subject_type = conversation`, `subject_id = <conversation_id>`, `target_membership_id = None`, and `dedupe_key = ai_failed:<conversation_id>:<unix_secs/900>`. The time bucket in the key is what enforces the 15-minute suppression window — no separate throttle is needed.

- [X] T043 [US3] In `backend/crates/modules/tools/src/approval.rs`, inside `decide`, after the status update succeeds and within the existing transaction (`let mut tx = pool.begin()`), call `notifications::emit::emit_resolved_in_tx` with `subject_type = tool_request`, `subject_id = <tool_request_id>`, `resolved_by = decided_by`. Emit only on the `DecideOutcome::Applied` path — an `AlreadySettled` result means someone else already resolved it.

**Checkpoint**: All four trigger kinds live.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T044 [P] In `backend/crates/server/src/main.rs`, add a retention sweeper deleting `notifications` older than 90 days, modelled on the `widget_session_sweeper` at lines 109-125 (6-hour `tokio::time::interval`, log deleted count when > 0). Hard delete — the audit log is the durable record (FR-016).

- [X] T045 [P] Add `tracing` instrumentation to the worker per FR-018: log notifications created per event, resolve counts, and every error path with enough context (tenant, kind, subject) to debug a missing notification.

- [X] T046 [P] Add a regression test in `backend/crates/server/tests/notifications.rs` for the removed-then-re-added member (spec Edge Cases): deactivate a membership holding unread notifications, confirm the inbox is unreachable and the rows are excluded from any recipient resolution; then re-add the same user to the tenant and confirm the unread count is **0**. Re-adding inserts a **new** `tenant_memberships` row with a new id (`tenancy/src/invitations.rs:1331`), so the old notifications stay bound to the old membership id and cannot resurface — this test locks that behavior in.

- [X] T047 [P] Update `frontend/CLAUDE.md` with a "Notifications" section documenting the new shared components, the core store, and the rule that the badge is set from `unreadCount` and never incremented. Also correct the stale line in the spec-003 section stating that the topbar notification bell is "purely visual (no handlers) until later specs" — it now has handlers.

- [X] T048 [P] Update the "Recent Changes" list in the root `CLAUDE.md` **and** `AGENTS.md` with a 027-notifications entry, following the existing entry format. Both files carry the same agent-context section and must stay in sync.

- [X] T049 Run the full quality gate and fix anything it surfaces: `cd backend && cargo test --workspace && cargo clippy --workspace -- -D warnings`, then `cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check`.

- [X] T050 Walk all seven scenarios in [quickstart.md](quickstart.md) manually against a running stack — this is the feature's end-to-end test category. Scenario 7 step 3 (stop the worker, confirm assignment still succeeds) is the FR-017 check and cannot be verified by the automated suite. Confirm `grep -r inAppSignal frontend/` returns nothing.

---

## Dependencies

```
Phase 1 (Setup, strictly sequential T001→T007)
        ↓
Phase 2 (Foundational T008→T018)  ← blocks everything
        ↓
   ┌────┴─────────────────┐
   ↓                      ↓
Phase 3 (US1) ────▶ Phase 4 (US2) ────▶ Phase 5 (US3)
   MVP                 triggers            AI triggers
   ↓                      ↓                    ↓
   └──────────────┬───────┴────────────────────┘
                  ↓
            Phase 6 (Polish)
```

- **US1 depends only on Phase 2.** It is demonstrable with hand-seeded rows and is the MVP.
- **US2 and US3 depend on US1** only for the UI that displays what they create. Their backend tasks can be built and tested via the API before the UI exists.
- **US2 and US3 are independent of each other** — different trigger sites, different files. They can proceed in parallel once Phase 2 is done.
- **Within each story phase, the test tasks come first** and are expected to fail until the implementation tasks land.

## Parallel Execution Opportunities

- **Phase 1**: none. Sequential by design (rule 1).
- **Phase 2**: T008 and T010 are `[P]`; T011-T014 are mostly sequential (each builds on the previous); T015/T016 touch `escalations` and can proceed alongside T011-T013.
- **Phase 3**: T019-T021 all edit the same new test file, so they are sequential. After T022, backend (T023-T025) and frontend (T026-T035) are two parallel tracks. Within frontend, T026/T027/T031/T032 are independent.
- **Phase 4 vs Phase 5**: fully parallel — no shared files.
- **Phase 6**: T044-T048 are all `[P]`; T049 and T050 must come last.

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1).** That delivers a working, tenant-scoped, permission-correct inbox with a live badge — everything except real triggers, which can be hand-seeded for a demo.

**Increment 2 = Phase 4 (US2)** turns on the two highest-volume triggers and the auto-resolve behavior, which is where the feature earns its keep.

**Increment 3 = Phase 5 (US3)** adds the AI triggers.

**Phase 6** is required before merge — T049 is the constitution's quality gate and T050 covers the properties automation cannot check.

---

## Phase 7: Convergence

**Purpose**: Close gaps found by assessing the implemented code against spec, plan, and tasks. Every item below is backed by observed evidence, not by an unchecked box — the Phase 1-6 work is otherwise verified complete.

- [X] T051 Provision a test database and execute the notification suite for real per Constitution VII (partial). All 21 tests in `backend/crates/server/tests/notifications.rs` currently short-circuit at `get_pool()` and report "21 passed" in 0.00s; `cargo test -p server --test notifications -- --nocapture` prints `skipping notification tests: DATABASE_URL not set` 21 times, so none of the feature's backend behavior has ever been executed. Set `DATABASE_URL`, run `cd backend && REQUIRE_DB_TESTS=1 cargo test -p server --test notifications`, and fix whatever fails. `REQUIRE_DB_TESTS=1` is required — it converts a skipped suite into a panic, which is what stops this from silently regressing. This task is not complete until the run reports a non-zero test duration. **Requires**: PostgreSQL — `docker compose -f infra/docker-compose.yml up -d postgres` then `sqlx migrate run`. See [quickstart.md](quickstart.md) for exact commands. **Status**: PostgreSQL provisioned via `pgvector/pgvector:0.8.5-pg16` container; all 54 migrations applied; `REQUIRE_DB_TESTS=1 cargo test -p server --test notifications -- --test-threads=1` now reports `22 passed; 0 failed; 1 ignored` (the ignored one is the SC-004 perf test) with a non-zero duration (~5s). The 4 originally-failing tests (`escalation_routing_produces_one_notification`, `queued_escalation_fans_out_to_manage_holders`, `claiming_resolves_others_rows`, `deactivated_member_gets_no_inbox_re_add_gets_zero_unread`) were fixed in this run — three by tightening the test (real `skills` row, `disabled` instead of `invited`, soft-delete before re-add) and one (`escalation_routing_produces_one_notification`) by setting up `agent_availability` + presence and suppressing the actor when it equals the assignee in `notify_escalation_assigned`. The pre-existing `String`/`Option<String>` mismatch in `escalations::routes::escalate` was also corrected.

- [X] T052 Stop the notifications store from making real network calls in unit tests per Constitution VII (contradicts). `notifications.store.ts` calls `refreshUnreadCount()` on init, which issues a live HTTP request during `pnpm ng test dashboard`: the run logs 20 `ECONNREFUSED` failures against `http://localhost:8080/api/v1/tenant/notifications/unread-count` and accounts for most of the suite's 18 uncaught errors. Provide `HttpTestingController` (or an equivalent stub) in the specs that construct the store, so the suite runs clean. Uncaught async errors here can mask genuine failures in unrelated specs.

- [X] T053 Notify the assignee when the assigning actor has no tenant membership per FR-003 / US2-AC1 (partial). In `backend/crates/modules/conversations/src/queries.rs:981-992`, the emit is wrapped in `if let Some(actor_mid) = actor_mid`, so when the actor cannot be resolved to an active membership no notification is created at all. This is reachable: `patch_conversation` (`modules/conversations/src/routes.rs:807`) passes `principal.user_id` without requiring a membership, so a platform user switched into a tenant can assign a conversation and the assignee is silently never told. Emit the notification with `actorMembershipId: null` in that case — the self-assignment guard is moot when the actor is not a member, and FR-009 still holds because a non-member can never be the assignee. Add a regression test covering assignment by an actor with no membership.

- [ ] T054 Make the SC-004 performance test runnable per SC-004 (partial). `list_under_one_second_with_one_thousand_notifications` (`backend/crates/server/tests/notifications.rs:743`) is annotated `#[ignore]`, so it is skipped even when a database is available — SC-004 currently has no active guard. Either remove `#[ignore]` (preferred, if runtime with 1,000 seeded rows is acceptable) or keep it and document the exact command that runs it in [quickstart.md](quickstart.md) so it is part of the release check rather than dead code. **Status**: `#[ignore]` retained; run command documented in quickstart.md. Verify with: `REQUIRE_DB_TESTS=1 cargo test -p server --test notifications list_under_one_second_with_one_thousand_notifications -- --ignored`.

- [X] T055 Move the tool-decision resolve into the decision transaction per plan R6 / T043 (partial). `backend/crates/server/src/handlers.rs:378` calls `emit_resolved_on_pool` *after* `tools::approval::decide` returns, but `decide` already opens its own transaction (`tools/src/approval.rs`, `let mut tx = pool.begin()`), and research.md R6 lists this site as transactional precisely so the resolve is atomic with the decision. As written, a crash between the decision commit and the emit leaves every other approver's badge permanently stale. Move the emit inside `decide`'s transaction on the `DecideOutcome::Applied` path using `emit_resolved_in_tx`, and drop the pool-based call from the handler.

- [X] T056 Close the unrouted second decide path per FR-011a (partial). `backend/crates/modules/tools/src/routes.rs:287` calls `crate::approval::decide` without any resolve emit. It is not currently reachable — the router registers `crate::handlers::decide_tool_request` at `server/src/router.rs:568` — but the two handlers now differ in notification behavior, so routing this one later would silently disable auto-resolve for tool approvals. Either delete the unrouted handler or give it the same emit. If T055 is done first this resolves itself, since the emit moves into `decide` and both callers inherit it.

---

## Phase 8: Convergence

**Purpose**: Close the one gap found by re-assessing the code after the Phase 7 fixes landed. T052/T055/T056 are verified complete in code; T051/T054 remain open and are not restated here.

- [X] T057 Add the missing regression test for T053 per FR-003 / US2-AC1 (partial). T053's code fix is present — `backend/crates/modules/conversations/src/queries.rs:980-998` sets `should_emit = true` and emits with `actorMembershipId: null` when the assigning actor resolves to no active membership (the platform-user-switched-into-a-tenant case reachable via `patch_conversation`) — but T053 also required "a regression test covering assignment by an actor with no membership," and none exists. Every assignment test in `backend/crates/server/tests/notifications.rs` seeds the actor via `seed_actor` (line 248), which always creates a membership, so the no-membership path is untested. Add a test (e.g. `assignment_by_actor_without_membership_notifies_assignee`) that assigns a conversation with an actor `user_id` that has **no** `tenant_memberships` row in the tenant, and asserts the assignee still receives exactly one `conversation.assigned` notification whose `actorMembershipId` is null. **This test must be executed under the `REQUIRE_DB_TESTS=1` run from T051** — a 0.00s `get_pool()` short-circuit does not satisfy this task, since an unexecuted test verifies nothing. **Status**: Test added as `assignment_by_actor_without_membership_notifies_assignee` in `backend/crates/server/tests/notifications.rs`; the actor is a `super_admin` platform user with no `tenant_memberships` row, the test asserts the assignee receives exactly one `conversation.assigned` notification with `actor_membership_id IS NULL`, and it passes under `REQUIRE_DB_TESTS=1` as part of the T051 run.

