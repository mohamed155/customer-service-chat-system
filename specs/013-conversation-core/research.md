# Research: Conversation Core

**Feature**: 013-conversation-core | **Date**: 2026-07-13

No NEEDS CLARIFICATION markers remained in the Technical Context (all five spec clarifications were resolved in `/speckit-clarify`). Research below records the design decisions that shape Phase 1, each grounded in the existing codebase.

## 1. Status vocabulary migration (`escalated` → spec set)

**Decision**: Migration 0033 replaces the `conversations_status_check` CHECK (`open|escalated|closed` from 0026) with the spec's fixed set `open|pending|resolved|closed`, remapping existing `escalated` rows to `open` in the same migration before the new CHECK is applied.

**Rationale**: The spec (Clarification session + FR-003) fixes the status set; `escalated` was a placeholder from the 012 summary record. Escalation is a distinct concept owned by the future `escalations` module (a placeholder crate already exists) and should not be conflated with workflow status. Existing `escalated` rows are dev/test-seeded only — an escalated conversation is by definition one needing attention, so `open` is the faithful remap. `closed` rows keep their value (`closed` exists in both sets).

**Alternatives considered**: Keeping `escalated` as a fifth status — rejected: contradicts the clarified fixed set and pre-empts the escalations module's design. Remapping `escalated` → `pending` — rejected: `pending` means "waiting on the customer", which escalated conversations are not.

## 2. Message storage model (kinds, senders, logged-by)

**Decision**: One `messages` table with `kind TEXT CHECK (kind IN ('customer','reply','note'))`:

- `customer` — customer-authored. `sender_membership_id IS NULL`; `logged_by_membership_id` nullable: `NULL` for seeded/future-inbound messages, set when a member manually logs it (Q4 clarification).
- `reply` — member-authored, customer-facing. `sender_membership_id NOT NULL`, `logged_by_membership_id IS NULL`.
- `note` — member-authored, internal-only. Same column rules as `reply`; the kind itself is the permanent internal marker (FR-005).

CHECK constraints enforce kind↔column consistency. Sender/logged-by reference `tenant_memberships(id)` (membership, not raw user) so authorship stays meaningful per-tenant and display can join to member names.

**Rationale**: A single table with a kind discriminator keeps the timeline one query and one ordering domain (FR-007). Referencing memberships mirrors how 011 models tenant actors and makes the composite-FK isolation pattern applicable. Recording `logged_by` as a separate column (rather than a fourth kind) keeps "who said it" (`customer`) orthogonal from "who recorded it" (member) — exactly the Q4 authorship-integrity requirement.

**Alternatives considered**: Separate `internal_notes` table — rejected: two-source timeline requires a merge sort in every read and duplicates isolation machinery. A `visibility` column orthogonal to `author_kind` — rejected: allows unrepresentable-in-spec combinations (customer-authored internal note); the three-value kind enumerates exactly the legal states.

## 3. Deterministic timeline ordering

**Decision**: `messages.seq BIGINT GENERATED ALWAYS AS IDENTITY`; timeline ordering is `ORDER BY created_at, seq` (ascending display; the API serves keyset pages newest-first via `ORDER BY created_at DESC, seq DESC`). The keyset cursor encodes `(created_at, seq)`.

**Rationale**: FR-007 demands stable, deterministic order for identical timestamps. `gen_random_uuid()` (v4) ids don't sort by insertion; an identity column does, costs nothing, and gives an always-unique keyset tie-breaker. Insertion order is the correct tie-break for same-instant messages.

**Alternatives considered**: Tie-break on UUID — rejected: stable but arbitrary (not insertion order) and misleading to readers. UUIDv7 primary keys — rejected: changes the platform-wide id convention for one feature.

## 4. Participants: derived, not stored

**Decision**: No participants table. The detail endpoint computes participants as: the conversation's customer + `SELECT DISTINCT` member ids from `messages.sender_membership_id` (and `logged_by_membership_id`), joined to membership/user names.

**Rationale**: FR-004 requires *tracking* who has been involved; the immutable message log already records this losslessly, so a table would be a denormalized cache with sync obligations (insert trigger or app-side upsert per message). The distinct-scan runs only on the detail read, bounded by the conversation's messages via the timeline index.

**Alternatives considered**: `conversation_participants` table maintained on message insert — rejected: adds write-path complexity and a second isolation surface for data derivable in one indexed query; revisit only if participant-based inbox filtering ("involving me") arrives in a later spec.

## 5. Assignment representation and validation

**Decision**: `conversations.assigned_membership_id UUID NULL` + composite FK `(tenant_id, assigned_membership_id) REFERENCES tenant_memberships(tenant_id, id)`. Assignment writes validate app-side that the target membership is `active`; DB guarantees same-tenant. `NULL` = unassigned. If an assignee is later deactivated, the column keeps its value (past assignment stays readable per the spec edge case); new assignments to inactive members are rejected 422; the detail/inbox payloads carry the assignee's membership status so the UI can flag "needs reassignment".

**Rationale**: Composite parent-tenant FKs are the established isolation pattern (migration 0027); membership (not user) is the tenant-scoped actor identity. Keeping the stale reference honors "keeps a readable record of the past assignment" while the active-only rule (FR-013) is a write-time business rule, not a DB invariant (deactivation must not break rows).

**Alternatives considered**: FK to `users(id)` — rejected: users are cross-tenant; would need separate tenant validation on every write. An `assignments` history table — rejected: the audit log already records every assignment change with actor/time (FR-017); current-state-on-row + audit history covers all spec needs.

## 6. Inbox query shape (filters, pagination, preview)

**Decision**: `GET /tenant/conversations` is a single statement: filter by `tenant_id` (+ `deleted_at IS NULL`) with optional `status` (default `open` per Q2 — an explicit `status=all` disables the status predicate), `assignee` (`me` | `unassigned` | membership uuid), and `channel` predicates; keyset pagination on `(last_activity_at DESC, id DESC)`; latest-message preview via `LEFT JOIN LATERAL (SELECT body, kind FROM messages WHERE tenant_id/conversation_id match ORDER BY created_at DESC, seq DESC LIMIT 1)`; joins customer display name. Indexes: `(tenant_id, status, last_activity_at DESC, id DESC) WHERE deleted_at IS NULL` and `(tenant_id, assigned_membership_id, last_activity_at DESC) WHERE deleted_at IS NULL`.

**Rationale**: Mirrors the 010/012 keyset pattern (over-fetch by one for `has_more`, opaque cursor). LATERAL with the timeline index is one index probe per returned row (≤ page size) — no N+1 and no denormalized `last_message_preview` column to keep in sync. `last_activity_at` is mutable, so keyset pages can shift as teammates act; the spec's coherence edge case (no duplicates/gaps hiding rows within a page) is satisfied because each page is internally consistent and the id tie-breaker is stable.

**Alternatives considered**: Denormalized preview columns on `conversations` updated per message — rejected: write-path coupling and truncation policy in the schema for a read-side concern the index already serves. Offset pagination — rejected: platform contract is cursor-based; offset degrades at 10k rows.

## 7. Auto-reopen semantics (Q1)

**Decision**: In the add-message transaction: insert message → bump `last_activity_at` → if `kind IN ('customer','reply')` AND `status IN ('resolved','closed')`, `UPDATE status = 'open'` and write a `conversation.status_changed` audit row attributing the acting member, with detail marking it as an automatic reopen. `note` never touches status. The message response returns the (possibly updated) conversation status so the UI refreshes badges without a refetch.

**Rationale**: Q1 clarification verbatim; FR-012 requires the auto-reopen to be "recorded like any status change". One transaction keeps message, activity bump, status flip, and audit atomic (mirrors 012's audit-in-transaction pattern).

**Alternatives considered**: Trigger-based reopen — rejected: business rules in triggers hide the audit actor; app-side keeps who-did-it explicit.

## 8. Audit vocabulary

**Decision**: Three new audit actions using the established `audit_logs` pattern (actor, action, resource = conversation id, detail JSON, same transaction as the write): `conversation.created` (customer, channel), `conversation.status_changed` (from, to, `auto: bool`), `conversation.assignment_changed` (from-membership, to-membership; covers assign and unassign). Message sends are **not** audited — the immutable `messages` table is itself the record (FR-017 scopes auditing to creation, status, assignment).

**Rationale**: Matches the 012 vocabulary style (`customer.created`/`customer.updated`) and the constitution's who/what/when requirement for sensitive operations.

**Alternatives considered**: A single `conversation.updated` action — rejected: status and assignment are distinct spec-level events with different detail payloads and different test assertions.

## 9. Frontend architecture (upgrade-in-place)

**Decision**: Upgrade `features/tenant/conversations/` from fixtures to live data, keeping the 003 visual composition: `conversations-api.service.ts` (Observable methods: list w/ filter params, create, get, timeline page, add message, patch, plus `listAssignableMembers()` calling the existing `GET /tenant/members` endpoint); `conversations.store.ts` rewritten as the live inbox SignalStore (filters incl. default `open`, cursor, items, loading/error); new `conversation-detail.store.ts` (conversation, timeline pages w/ load-older, composer submit state); new `conversation-detail.component.ts` route at `conversations/:id` guarded by `conversations.view`; composer as a project component with reply/note/log modes (mode visibility and controls additionally gated by `conversations.manage` via the existing permission directive/service); new-conversation dialog reusing `dialog-shell` with a customer picker backed by the 012 customers search endpoint. Status-badge shared component gains `pending`/`resolved` variants once; internal notes styled distinctly in the thread component. `APP_PATHS.tenant.conversationDetail` added; the hardcoded fixture subtitle in `page-title.ts` replaced.

**Rationale**: Spec-002 layering (feature-scoped services/stores, shared components for visuals, `core/` only for models/paths/authz) and spec-003's rule that Taiga stays wrapped. Reusing `GET /tenant/members` through the conversations feature's own service avoids a cross-feature import of `team-api.service` (features must stay independently lazy-loadable). The customer picker reuses the customers search endpoint (012) — no new backend surface.

**Alternatives considered**: Extracting a shared members-lookup service into `core/` — rejected for now: `core/` is for singletons without feature business logic; one thin service method is cheaper than a new core API surface. Deleting `customer-panel.component.ts` outright vs folding — decided during implementation; the detail page shows customer context from the conversation payload either way.

## 10. Continuity with 012 (customer profile history)

**Decision**: `GET /tenant/customers/{id}/conversations` keeps its route, permission (`customers.view`), envelope, and 20-row window; only the `status` vocabulary in responses changes to the new set (FR-018). The 012 contract's status note is superseded by this feature's contract. Frontend profile's history section maps the new statuses to badges (shared status-badge variants) and links each row to `conversations/:id`.

**Rationale**: FR-018 requires the profile section to keep working with real data "without changing what the profile shows"; the summary fields (channel, status, last-activity) are all still first-class conversation columns.

**Alternatives considered**: Migrating the history section to the new inbox endpoint filtered by customer — rejected: the existing endpoint already does exactly this job with customer-existence 404 semantics; adding a `customer_id` inbox filter is a fine future extension but not needed by any spec requirement.
