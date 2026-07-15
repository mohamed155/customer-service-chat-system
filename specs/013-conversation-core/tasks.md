---

description: "Task list for Conversation Core (013)"
---

# Tasks: Conversation Core

**Input**: Design documents from `/specs/013-conversation-core/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/rest-api.md](./contracts/rest-api.md), [contracts/permissions.md](./contracts/permissions.md), [quickstart.md](./quickstart.md)

**Tests**: Included — the spec (FR-019), constitution Principle VII, and the plan's Testing strategy all require a dedicated integration suite, an rbac matrix extension, schema tests, and Vitest specs.

**Organization**: Tasks are grouped by user story (spec.md priorities P1–P5) so each story is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: Maps the task to US1–US5
- Every task names an exact file path

## Path Conventions

Existing Cargo workspace (`backend/crates/`) + Angular pnpm workspace (`frontend/apps/dashboard/`), per [plan.md § Project Structure](./plan.md#project-structure).

---

## Phase 1: Setup

**Purpose**: Prerequisite dependency for the audit helpers added in Phase 2.

- [x] T001 Add a non-dev `serde_json` dependency to `backend/crates/modules/conversations/Cargo.toml` (needed for audit `detail` JSON payloads; mirrors `customers/Cargo.toml`'s `serde_json.workspace = true` under `[dependencies]`)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema and module scaffolding shared by all five user stories.

**⚠️ CRITICAL**: No user story task may start until this phase is complete.

- [x] T002 [P] Migration `backend/migrations/0033_conversation_core.sql`: `UPDATE conversations SET status = 'open' WHERE status = 'escalated'`; drop and re-add `conversations_status_check` as `status IN ('open','pending','resolved','closed')`; add `assigned_membership_id UUID NULL` with composite FK `(tenant_id, assigned_membership_id) REFERENCES tenant_memberships(tenant_id, id)`; add inbox index `(tenant_id, status, last_activity_at DESC, id DESC) WHERE deleted_at IS NULL` and assignee index `(tenant_id, assigned_membership_id, last_activity_at DESC) WHERE deleted_at IS NULL`
- [x] T003 [P] Migration `backend/migrations/0034_messages.sql`: create `messages` table per [data-model.md § Message](./data-model.md#entity-message-messages--new-in-0034) — `id`, `tenant_id`, `conversation_id` (composite FK `(tenant_id, conversation_id) → conversations(tenant_id, id)`), `kind` CHECK `IN ('customer','reply','note')`, `sender_membership_id`/`logged_by_membership_id` (composite FKs to `tenant_memberships(tenant_id, id)`) with the kind-consistency CHECK enumerating the three legal combinations, `body` CHECK `char_length(body) BETWEEN 1 AND 10000`, `seq BIGINT GENERATED ALWAYS AS IDENTITY`, `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`, no `updated_at`/`deleted_at` (append-only per Complexity Tracking); add timeline index `(tenant_id, conversation_id, created_at DESC, seq DESC)`
- [x] T004 Add schema assertions for 0033 and 0034 to `backend/crates/shared/db/tests/schema.rs`: status CHECK accepts only `open|pending|resolved|closed`, `assigned_membership_id` FK/nullability, both new `conversations` indexes, `messages` table columns/CHECKs (kind, body length, kind-consistency), composite FKs, `seq` identity behavior, and the timeline index
- [x] T005 [P] Define core types in `backend/crates/modules/conversations/src/model.rs`: `ConversationStatus`, `MessageKind` enums (serde string (de)serialization matching the wire vocabulary — reuse the existing `channel` string convention); `Conversation` (inbox item), `ConversationDetail` (adds `participants`), `Assignee`, `Participant`, `LastMessagePreview`, `Message` response DTOs matching [contracts/rest-api.md § Resource representations](./contracts/rest-api.md#resource-representations); request payload structs (`CreateConversationPayload`, `AddMessagePayload`, `PatchConversationPayload`) with the validation rules from [data-model.md § Validation rules](./data-model.md#validation-rules-write-paths)
- [x] T006 [P] Implement audit helpers in `backend/crates/modules/conversations/src/audit.rs`: `record_conversation_created` (`customer_id`, `channel`), `record_status_changed` (`from`, `to`, `auto: bool`), `record_assignment_changed` (`from_membership_id`, `to_membership_id`) — each calling `tenancy::audit::record_in_tx` inside the caller's transaction, mirroring `backend/crates/modules/customers/src/audit.rs`
- [x] T007 Implement shared query helpers in `backend/crates/modules/conversations/src/queries.rs` (depends on T005): `conversation_row_in_tx` (fetch one conversation scoped to `(tenant_id, id)`, `deleted_at IS NULL`, for 404-safe reuse across detail/patch/add-message), `active_membership_exists_in_tx` (tenant-scoped, `status = 'active'` check for assignment validation), `participants_in_tx` (customer + `DISTINCT` `sender_membership_id`/`logged_by_membership_id` from `messages`, joined to membership/user display names, per [research.md §4](./research.md))
- [x] T008 Rewrite module docs in `backend/crates/modules/conversations/src/lib.rs` to document Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, and Extension Points for full conversation/message ownership as required by Constitution Documentation & Future Readiness; add `pub mod model; pub mod audit; pub mod queries; pub mod routes;`; keep `ConversationSummary`/`list_recent_for_customer(_in_tx)`/`get_conversation_history` in place unchanged; create `backend/crates/modules/conversations/src/routes.rs` as a scaffold file (imports, `use super::{model, queries, audit};`) ready for the handlers added in US1–US5

**Checkpoint**: Migrations applied, module scaffolding compiles — user story implementation can begin.

---

## Phase 3: User Story 1 - Work the Conversation Inbox (Priority: P1) 🎯 MVP

**Goal**: Tenant members see a filterable, tenant-isolated, paginated conversation inbox ordered by recent activity.

**Independent Test**: Seed two tenants with distinct conversations in varied statuses/channels/assignments; sign in as a member of each and confirm the inbox lists only that tenant's conversations, ordered correctly, with each filter narrowing results.

### Tests for User Story 1

- [x] T009 [P] [US1] Integration tests in `backend/crates/server/tests/conversations.rs` (new file): default view shows only `open` conversations (Q2), `status=all` returns every status, `status`/`assignee=me|unassigned|uuid`/`channel` filters individually and combined, unknown filter values → `422`, keyset pagination (`has_more`, no duplicates/gaps across pages), empty-filter-match returns `data: []`, and per-tenant isolation (tenant B never sees tenant A's rows in list/count/pagination) — FR-019
- [x] T010 [P] [US1] Vitest spec for `list()` in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.spec.ts` (new): request params for each filter combination, cursor pass-through, wire→model mapping
- [x] T011 [P] [US1] Rewrite `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.store.spec.ts` for the live inbox store: default `status=open`, filter changes reset the cursor and reload, loading/error state, empty-result state

### Implementation for User Story 1

- [x] T012 [US1] Implement the inbox query in `backend/crates/modules/conversations/src/queries.rs`: single statement filtering `tenant_id` + `deleted_at IS NULL` + optional `status`/`assignee`/`channel` predicates, keyset pagination on `(last_activity_at DESC, id DESC)`, `LEFT JOIN LATERAL` over `messages` (ordered `created_at DESC, seq DESC LIMIT 1`) for the preview, customer display-name join — per [research.md §6](./research.md)
- [x] T013 [US1] Implement `list_conversations` handler for `GET /tenant/conversations` in `backend/crates/modules/conversations/src/routes.rs`: parses `status`/`assignee`/`channel`/`cursor`/`limit` (default 25, max 100), `422` on unknown values, maps rows to `Conversation` DTOs, returns `PaginatedResponse<Conversation>` (depends on T012)
- [x] T014 [US1] Register `.guarded("/tenant/conversations", routing::get(conversations::routes::list_conversations), Permission::ConversationsView)` in `backend/crates/server/src/router.rs` `tenant_routes()` (depends on T013)
- [x] T015 [US1] Add `GET /tenant/conversations` → `conversations.view` to the route→permission matrix in `backend/crates/server/tests/rbac.rs` (depends on T014)
- [x] T016 [P] [US1] Add `Conversation` (inbox item), `ConversationWire`, filter/query param types, and `conversationFromWire` mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` per [contracts/rest-api.md § Conversation (inbox item)](./contracts/rest-api.md#conversation-inbox-item)
- [x] T017 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` with Observable `list()` (status/assignee/channel/cursor/limit) and `listAssignableMembers()` (existing `GET /tenant/members`) methods so the inbox can resolve specific-member filters without a cross-feature import (depends on T016, T014)
- [x] T018 [US1] Rewrite `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.store.ts` as the live inbox `SignalStore`: filters (default `status: 'open'`), cursor, items, loading/error, replacing `ConversationFixture`/`CustomerFixture` state (depends on T017)
- [x] T019 [US1] Update `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.component.ts` to drive the live store: status, channel, and assignee controls populated through `listAssignableMembers()` with mine/unassigned/any-specific-member options, pagination, "New conversation" action placeholder, and empty-state with filter reset (depends on T017, T018)
- [x] T020 [US1] Update `frontend/apps/dashboard/src/app/features/tenant/conversations/inbox-list.component.ts` to render live rows: customer, channel badge, status badge, assignee (with inactive flag), latest-message preview, last-activity time (depends on T018)
- [x] T021 [US1] Extend the conversation status→badge-tone mapping in `frontend/apps/dashboard/src/app/features/tenant/conversations/inbox-list.component.ts` to cover `pending`/`resolved` (replacing the old `escalated` mapping), reusing `app-status-badge` (depends on T020)

**Checkpoint**: US1 is fully functional and independently testable — inbox loads, filters work, tenant isolation holds.

---

## Phase 4: User Story 2 - Read a Conversation Timeline (Priority: P2)

**Goal**: Opening a conversation shows participants, status/assignee, and a stably-ordered message timeline with internal notes visually distinct.

**Independent Test**: Seed a conversation with interleaved customer messages, replies, and notes; open its detail page and verify chronological order, correct sender attribution, and note styling; confirm a cross-tenant id returns not-found.

### Tests for User Story 2

- [x] T022 [P] [US2] Integration tests in `backend/crates/server/tests/conversations.rs`: detail returns conversation + participants; timeline returns messages in `created_at DESC, seq DESC` pages with `cursor`/`has_more`, identical order across repeated reads including seeded same-`created_at` rows tie-broken by `seq`; load-older never reorders/duplicates; empty-timeline conversation shows an empty list; cross-tenant detail/timeline requests → `404` (FR-019)
- [x] T023 [P] [US2] Add Vitest cases in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.spec.ts` for `get()` and `getTimeline()`: request shape, cursor pass-through, wire→model mapping
- [x] T024 [P] [US2] Vitest spec `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.store.spec.ts` (new): loads conversation + first timeline page, `loadOlder()` prepends without reordering, tracks composer/submit state placeholders
- [x] T025 [P] [US2] Vitest spec `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.spec.ts` (new): renders header (customer/channel/status/assignee/participants), ascending timeline with notes visually distinct, and safely renders markup-like, emoji, right-to-left, and maximum-length plain text without interpreting markup or breaking the timeline

### Implementation for User Story 2

- [x] T026 [US2] Implement the detail query in `backend/crates/modules/conversations/src/queries.rs`: fetch conversation row (via `conversation_row_in_tx` from T007) + `participants_in_tx` into a `ConversationDetail` (depends on T007, T005)
- [x] T027 [US2] Implement the timeline keyset query in `backend/crates/modules/conversations/src/queries.rs`: `ORDER BY created_at DESC, seq DESC` over `(tenant_id, conversation_id)`, opaque cursor over `(created_at, seq)`, over-fetch by one for `has_more` (depends on T007)
- [x] T028 [US2] Implement `get_conversation`/`get_timeline` handlers for `GET /tenant/conversations/{id}` and `GET /tenant/conversations/{id}/messages` in `backend/crates/modules/conversations/src/routes.rs`: `404 not_found` for missing/cross-tenant/soft-deleted conversations (depends on T026, T027)
- [x] T029 [US2] Register both GET routes in `backend/crates/server/src/router.rs` `tenant_routes()` under `Permission::ConversationsView` (depends on T028)
- [x] T030 [US2] Add `GET /tenant/conversations/{id}` and `GET /tenant/conversations/{id}/messages` → `conversations.view` to the matrix in `backend/crates/server/tests/rbac.rs` (depends on T029)
- [x] T031 [P] [US2] Add `ConversationDetail`, `Message`, `Participant`, `MessageWire`, timeline query/response types, and their wire mappers to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` per [contracts/rest-api.md § Conversation (detail) / Message](./contracts/rest-api.md#conversation-detail)
- [x] T032 [US2] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` with `get(id)` and `getTimeline(id, cursor?)` (depends on T031, T029)
- [x] T033 [US2] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.store.ts`: conversation, timeline pages with `loadOlder()`, loading/error (depends on T032)
- [x] T034 [US2] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.ts`: header (customer/channel/status badge/assignee/participants) + timeline region; fold `customer-panel.component.ts`'s fixture sidebar into this page using live customer data from the conversation payload, then delete `customer-panel.component.ts` (depends on T033)
- [x] T035 [US2] Update `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-thread.component.ts` to render the live timeline ascending, style `note`-kind messages distinctly, and expose a load-older trigger (depends on T033)
- [x] T036 [US2] Add `conversationDetail: (id: string) => \`conversations/${id}\`` to `APP_PATHS.tenant` in `frontend/apps/dashboard/src/app/core/router/app-paths.ts`; add the `conversations/:id` child route (`conversations.view`-guarded) in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts`; add a `conversationDetail` entry to `PAGE_TITLES`/`PageTitleKey` in `frontend/apps/dashboard/src/app/core/router/page-title.ts`; wire inbox row clicks in `inbox-list.component.ts` to navigate to the new path (depends on T034)

**Checkpoint**: US1 and US2 both independently functional — inbox → detail navigation works, timeline is stable and isolated.

---

## Phase 5: User Story 3 - Reply and Leave Internal Notes (Priority: P3)

**Goal**: Permitted members can reply, leave internal notes, or log a customer message from the composer; resolved/closed conversations auto-reopen on a customer-facing entry.

**Independent Test**: As a permitted member, send a reply and a note in a seeded conversation; verify both appear at the end of the timeline with correct kind/sender/timestamp and that last-activity/inbox preview update. As a view-only member, verify the composer is unavailable and direct submission is refused.

### Tests for User Story 3

- [x] T037 [P] [US3] Integration tests in `backend/crates/server/tests/conversations.rs`: reply/note/logged-customer message appended with correct `kind`/`sender`/`logged_by`; `last_activity_at` bumps; empty/whitespace-only and >10,000-char bodies → `422` with field detail; reply or logged-customer message on `resolved`/`closed` → response `conversation.status == "open"` + `conversation.status_changed` audit row with `auto: true`; a `note` never changes status; Viewer → `403`; POSTing each message kind to another tenant's conversation id → indistinguishable `404` with no message, activity, status, or audit mutation (FR-016/FR-019)
- [x] T038 [P] [US3] Add Vitest mode-switching, whitespace rejection, maximum-length/plain-text safety, and permission-visibility cases in `frontend/apps/dashboard/src/app/features/tenant/conversations/composer.component.spec.ts`; add the `addMessage()` request/response case in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.spec.ts`

### Implementation for User Story 3

- [x] T039 [US3] Implement the add-message transaction in `backend/crates/modules/conversations/src/queries.rs`: insert message, bump `last_activity_at`, and — when `kind IN ('customer','reply')` and current status `IN ('resolved','closed')` — update `status = 'open'` and call `audit::record_status_changed(..., auto: true)` (T006), all in one transaction (depends on T007, T006)
- [x] T040 [US3] Implement `add_message` handler for `POST /tenant/conversations/{id}/messages` in `backend/crates/modules/conversations/src/routes.rs`: validates `kind`/body (1–10,000 chars after trim), sets `logged_by_membership_id` to the acting member for `kind: customer`, sets `sender_membership_id` to the acting member for `reply`/`note`, returns `{ message, conversation: { status, last_activity_at } }` (depends on T039)
- [x] T041 [US3] Upgrade the `/tenant/conversations/{id}/messages` registration in `backend/crates/server/src/router.rs` to `.guarded_with_methods()`, adding `POST` → `Permission::ConversationsManage` alongside the existing GET (depends on T040)
- [x] T042 [US3] Add `POST /tenant/conversations/{id}/messages` → `conversations.manage` to the matrix in `backend/crates/server/tests/rbac.rs` (depends on T041)
- [x] T043 [P] [US3] Add `AddMessagePayload` type and wire mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- [x] T044 [US3] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` with `addMessage(conversationId, payload)` (depends on T043, T041)
- [x] T045 [US3] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/composer.component.ts`: reply / internal-note / log-customer-message modes, whitespace-only client-side rejection, rendered only under `conversations.manage` (depends on T044)
- [x] T046 [US3] Wire the composer into `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.store.ts` (submit state, append returned message, sync conversation status) and `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.ts` (depends on T045, T033, T034)

**Checkpoint**: US1–US3 independently functional — composing works, auto-reopen fires, Viewer is blocked.

---

## Phase 6: User Story 4 - Manage Status and Assignment (Priority: P4)

**Goal**: Permitted members change status (any→any) and assign/unassign conversations; every change is audited.

**Independent Test**: Move a seeded conversation open → pending → resolved → open, assign it to a teammate, then unassign it; verify each change on the detail page and inbox (filters/badges) plus a who/when record. As a view-only member, verify the controls are unavailable and refused.

### Tests for User Story 4

- [x] T047 [P] [US4] Integration tests in `backend/crates/server/tests/conversations.rs`: any→any status PATCH; assignment to an active member and unassignment (`null`); inactive or cross-tenant membership → `422` with `{"field": "assigned_membership_id"}`; `conversation.status_changed`/`conversation.assignment_changed` audit rows written on change, none on no-op values; two concurrent status/assignment writers complete with a valid last-write-wins final state and preserve both actors' audit records; missing-both-fields → `422`; Viewer → `403`; cross-tenant conversation id → `404` (FR-019)
- [x] T048 [P] [US4] Add status/assignee control interaction cases in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.spec.ts`, and `patch()`/`listAssignableMembers()` cases in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.spec.ts`

### Implementation for User Story 4

- [x] T049 [US4] Implement the patch transaction in `backend/crates/modules/conversations/src/queries.rs`: lock the tenant-scoped conversation row before reading prior values, apply status and/or assignment with last-successful-transaction-wins semantics, validate assignment via `active_membership_exists_in_tx` from T007, skip no-op audits, and preserve one matching audit record per committed change under concurrent writers (depends on T007, T006)
- [x] T050 [US4] Implement `patch_conversation` handler for `PATCH /tenant/conversations/{id}` in `backend/crates/modules/conversations/src/routes.rs`: requires at least one of `status`/`assigned_membership_id` (else `422`), returns `ApiResponse<ConversationDetail>` (depends on T049)
- [x] T051 [US4] Upgrade the `/tenant/conversations/{id}` registration in `backend/crates/server/src/router.rs` to `.guarded_with_methods()`, adding `PATCH` → `Permission::ConversationsManage` alongside the existing GET (depends on T050)
- [x] T052 [US4] Add `PATCH /tenant/conversations/{id}` → `conversations.manage` to the matrix in `backend/crates/server/tests/rbac.rs` (depends on T051)
- [x] T053 [P] [US4] Add `PatchConversationPayload` type and wire mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- [x] T054 [US4] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` with `patch(id, payload)`, reusing its US1 `listAssignableMembers()` method for the assignee picker (depends on T053, T051)
- [x] T055 [US4] Add status control and assignee picker (backed by `listAssignableMembers()`, flags inactive current assignee) to `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-detail.component.ts`, rendered only under `conversations.manage` (depends on T054, T034)

**Checkpoint**: US1–US4 independently functional — status/assignment work end-to-end with audit trail.

---

## Phase 7: User Story 5 - Start a New Conversation (Priority: P5)

**Goal**: Permitted members start a new conversation for an existing customer with a first message; it appears open/unassigned in the inbox and in the customer's profile history.

**Independent Test**: Create a conversation for a seeded customer on a supported channel with a first message; verify it appears in the inbox as open/unassigned, its timeline shows the first message, and it appears in the customer's profile history.

### Tests for User Story 5

- [x] T056 [P] [US5] Integration tests in `backend/crates/server/tests/conversations.rs`: create → `open`, unassigned, first message present as `kind: reply`, `conversation.created` audit row; missing `customer_id`/`channel`/empty message → `422` with field details; unknown/cross-tenant `customer_id` → `404` (FR-016); a second concurrent open conversation for the same customer+channel succeeds (Q3); the new conversation appears via `GET /tenant/customers/{id}/conversations` (FR-018 continuity)
- [x] T057 [P] [US5] Add field-validation and submit specs in `frontend/apps/dashboard/src/app/features/tenant/conversations/new-conversation-dialog.component.spec.ts`, and a `create()` API case in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.spec.ts`

### Implementation for User Story 5

- [x] T058 [US5] Implement the create transaction in `backend/crates/modules/conversations/src/queries.rs`: `customers::customer_exists_in_tx` gate (404 on failure, per FR-016), insert conversation (`status = 'open'`, `assigned_membership_id = NULL`), insert first message (`kind = 'reply'`, sender = acting member), bump `last_activity_at`, call `audit::record_conversation_created` (T006) (depends on T007, T006)
- [x] T059 [US5] Implement `create_conversation` handler for `POST /tenant/conversations` in `backend/crates/modules/conversations/src/routes.rs`: field-level `422`s for missing/invalid `customer_id`/`channel`/`message.body`, returns `201` + `ApiResponse<ConversationDetail>` (depends on T058)
- [x] T060 [US5] Upgrade the `/tenant/conversations` registration in `backend/crates/server/src/router.rs` to `.guarded_with_methods()`, adding `POST` → `Permission::ConversationsManage` alongside the existing GET (depends on T059)
- [x] T061 [US5] Add `POST /tenant/conversations` → `conversations.manage` to the matrix in `backend/crates/server/tests/rbac.rs` (depends on T060)
- [x] T062 [P] [US5] Add `CreateConversationPayload` type and wire mapper to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- [x] T063 [US5] Extend `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` with `create(payload)` (depends on T062, T060)
- [x] T064 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/conversations/new-conversation-dialog.component.ts`: customer picker (012 customer search endpoint) + channel select + first-message field, built on `dialog-shell` (depends on T063)
- [x] T065 [US5] Wire the "New conversation" action in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations.component.ts` to open the dialog and navigate to the created conversation's detail route on success, rendered only under `conversations.manage` (depends on T064, T019, T036)

**Checkpoint**: All five user stories independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: FR-018 continuity for the pre-existing customer-profile history section, fixture cleanup, and full gate validation.

- [x] T066 [P] Update the `ConversationSummary` status union in `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts` from `'open' | 'escalated' | 'closed'` to `'open' | 'pending' | 'resolved' | 'closed'`, and update the status→badge-tone mapping in `frontend/apps/dashboard/src/app/features/tenant/customers/customer-profile.component.ts` to match (FR-018)
- [x] T067 [P] Replace the hardcoded `'Shared inbox · 6 open, 2 escalated'` subtitle for the conversations page in `frontend/apps/dashboard/src/app/core/router/page-title.ts` with copy that doesn't reference fixture counts
- [x] T068 [P] Remove conversation entries from `frontend/apps/dashboard/src/app/shared/fixtures/conversation.fixtures.ts` and the `ConversationFixture`/`ConversationStatus` fixture types that are no longer consumed by any live component (leave fixtures still used elsewhere, e.g. `overview.component.ts`, untouched)
- [x] T069 [P] Add ignored/live-gated seeded-volume integration cases to `backend/crates/server/tests/conversations.rs` that create 10,000 tenant conversations and a 1,000-message timeline, assert each target request completes under SC-002's one-second threshold, and record query plans on failure
- [x] T070 [P] Add an automated Agent/Viewer conversation journey in `frontend/e2e/conversation-core.spec.ts`: inbox default/filter/reset, inbox→detail navigation, stable load-older timeline, create/reply/note/log, auto-reopen, assign/unassign, customer-profile continuity, permission-hidden controls, and a timed inbox-to-timeline assertion under SC-001's 15-second threshold
- [x] T071 Run the backend quickstart gate from `backend/`: `REQUIRE_DB_TESTS=1 cargo test -p db --test schema`, `REQUIRE_DB_TESTS=1 cargo test -p server --test conversations`, `REQUIRE_DB_TESTS=1 cargo test -p server --test rbac`, the live-gated SC-002 performance cases, then full `REQUIRE_DB_TESTS=1 cargo test`
- [x] T072 Run the frontend quickstart and constitution gates from `frontend/`: `pnpm ng test dashboard`, `pnpm ng build dashboard`, `pnpm test:e2e`, `pnpm lint`, `pnpm format:check`
- [x] T073 Execute `specs/013-conversation-core/quickstart.md` Manual smoke steps 1–7 against the dev server and confirm each expected backend/frontend outcome

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup (T001, for T006's `serde_json` use) — BLOCKS all user stories.
- **User Stories (Phase 3–7)**: All depend on Foundational (Phase 2) completion. Within each story, `routes.rs`/`router.rs`/`rbac.rs` edits are sequential (same files across stories where two methods share a path — e.g. `/tenant/conversations` GET in US1, POST in US5); story implementation tasks are otherwise independent of other stories' implementation tasks.
- **Polish (Phase 8)**: T066–T068 can start once T004/T034 land (US2); T069 and T070 can be authored in parallel after their covered stories exist; T071–T073 depend on all prior implementation and test tasks.

### User Story Dependencies

- **US1 (P1)**: Foundational only.
- **US2 (P2)**: Foundational only; reuses `conversation_row_in_tx`/`participants_in_tx` from Foundational. Not blocked by US1, but the inbox→detail nav link (T036) is more useful once US1 exists.
- **US3 (P3)**: Foundational + US2's `conversation-detail.store.ts`/`component.ts` (T033/T034) for composer wiring (T046).
- **US4 (P4)**: Foundational + US2's detail component (T034) for control placement (T055).
- **US5 (P5)**: Foundational + US1's inbox action slot (T019) and US2's detail route (T036) for post-create navigation (T065).

### Within Each User Story

- Tests written first (and expected to fail before implementation exists).
- Backend: query helpers → handler → router registration → rbac matrix entry.
- Frontend: wire types → API service method → store → component.

### Parallel Opportunities

- T002 and T003 (the two migrations, independent files/content).
- T005 and T006 (model.rs and audit.rs, independent files).
- All `[P]`-marked test tasks within a story phase.
- T016/T031/T043/T053/T062 (frontend wire-type additions) can run alongside their story's backend implementation tasks, since both read the same contract but touch different files.

---

## Parallel Example: User Story 1

```bash
# Tests (after Foundational, before implementation):
Task: "Integration tests in backend/crates/server/tests/conversations.rs (inbox list/filters/isolation)"
Task: "Vitest spec for list() in conversations-api.service.spec.ts"
Task: "Rewrite conversations.store.spec.ts for the live inbox store"

# Frontend wire types alongside backend query/handler work:
Task: "Add Conversation model + filter types to tenant-api.models.ts"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (migrations, model/audit/queries scaffolding — CRITICAL, blocks everything)
3. Complete Phase 3: User Story 1 (inbox)
4. **STOP and VALIDATE**: run T009–T011, confirm inbox loads with correct filters and tenant isolation
5. Demo: tenant member sees their conversations, filters work, no cross-tenant leakage

### Incremental Delivery

1. Setup + Foundational → schema and module scaffolding ready
2. US1 → inbox is browsable (MVP)
3. US2 → open a conversation, read its timeline
4. US3 → reply/note/log — conversations become actionable
5. US4 → status/assignment turn the inbox into a working queue
6. US5 → outbound conversation creation completes the lifecycle
7. Polish → FR-018 continuity, fixture cleanup, full gate + manual smoke

Each story is independently testable and demoable per its Independent Test above before moving to the next.

---

## Notes

- `[P]` tasks touch different files with no dependency on an incomplete task.
- Three route paths are shared by two stories each (`/tenant/conversations`: US1 GET / US5 POST; `/tenant/conversations/{id}`: US2 GET / US4 PATCH; `/tenant/conversations/{id}/messages`: US2 GET / US3 POST) — the second story to touch a path upgrades `.guarded()` to `.guarded_with_methods()` rather than re-registering.
- `backend/crates/server/tests/conversations.rs` and `frontend/.../conversations-api.service.spec.ts` are each touched by every story (new file created in US1, extended after) — expected, not a parallelism violation.
- FR-019 (isolation) and FR-007 (ordering) are verified incrementally: isolation assertions appear in every story's integration test task for that story's operations; ordering/stability assertions are concentrated in US2 (T022) where the timeline is introduced.
- Commit after each task or logical group; stop at any checkpoint to validate a story independently.

---

## Phase 9: Convergence

- [x] T074 CRITICAL — Remove the 59 stale `#[ignore = "depends on …"]` attributes (and their now-obsolete explanatory comments) from `backend/crates/server/tests/conversations.rs` so the integration suite actually executes — the referenced handler tasks (T013/T014, T028/T029, T040/T041, T050/T051, T059/T060) are all implemented; keep `#[ignore]` only on the two SC-002 seeded-volume performance cases (`inbox_list_with_10k_conversations_stays_under_1s`, `timeline_with_1k_messages_stays_under_1s`) per T069; then run `REQUIRE_DB_TESTS=1 cargo test -p server --test conversations` and fix any failures the newly-enabled tests surface, per FR-019 / SC-003 / SC-004 / Constitution VII (partial)
- [x] T075 Add the log-customer-message mode to `frontend/apps/dashboard/src/app/features/tenant/conversations/composer.component.ts` — a third mode tab emitting `kind: 'customer'` (backend already accepts it and records `logged_by_membership_id`), with mode-appropriate placeholder copy — and add mode-switching/submit Vitest cases for it in `frontend/apps/dashboard/src/app/features/tenant/conversations/composer.component.spec.ts`, per US3/AC6 + FR-011 (missing)
- [x] T076 Add a concurrent status/assignment writers integration test to `backend/crates/server/tests/conversations.rs`: two writers PATCH the same conversation near-simultaneously, final state is a valid last-write-wins outcome (never mixed/invalid), and one audit record per committed change is preserved for both actors, per US4 + spec edge case "two members change status at nearly the same time" (missing)
- [x] T077 Add Viewer-403 coverage for conversation creation: a `viewer_403_for_create` test in `backend/crates/server/tests/conversations.rs` asserting `POST /tenant/conversations` returns 403 for a Viewer with nothing created and no audit row, completing write-path permission coverage across all six conversation routes, per SC-006 + contracts/permissions.md route→permission map (missing)
- [x] T078 Extend `frontend/e2e/conversation-core.spec.ts` to the full T070 journey: inbox filter narrowing + empty-state filter reset, load-older timeline stability, new-conversation create flow, internal-note and log-customer-message composition, auto-reopen of a resolved conversation on reply, assign/unassign, customer-profile conversation-history continuity, "New conversation" hidden for Viewer, and a timed inbox-to-timeline assertion under SC-001's 15-second threshold, per T070 + SC-001 (partial)
