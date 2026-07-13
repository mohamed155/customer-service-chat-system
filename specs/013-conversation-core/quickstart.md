# Quickstart Validation: Conversation Core

Runnable checks proving the feature end-to-end. Contracts: [rest-api.md](./contracts/rest-api.md), [permissions.md](./contracts/permissions.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running with the backend's standard env configuration (same setup as the existing live-gated suites, e.g. `server/tests/customers.rs`); migrations 0001–0034 applied by the test harness/startup.
- `cargo` (backend), `pnpm` (frontend, run from `frontend/`).
- Two seeded tenants with memberships covering an Agent and a Viewer, plus seeded customers (the integration suite provisions its own).

## Backend validation

```bash
cd backend

# Schema: 0033 status remap/CHECK + assignee FK + inbox indexes; 0034 messages table,
# kind-consistency CHECK, composite FKs, seq identity, timeline index
REQUIRE_DB_TESTS=1 cargo test -p db --test schema

# Feature suite: inbox list/filters/default-open, create, detail, timeline ordering,
# compose (reply/note/logged), auto-reopen, status/assignment, audit, isolation matrix
REQUIRE_DB_TESTS=1 cargo test -p server --test conversations

# RBAC matrix: all six conversation routes across all roles
REQUIRE_DB_TESTS=1 cargo test -p server --test rbac

# Full backend gate
REQUIRE_DB_TESTS=1 cargo test
```

**Expected outcomes** (each maps to a spec item):

| Check | Expectation |
|-------|-------------|
| Isolation (FR-016/FR-019, SC-003) | Every list/view/timeline/create/message/patch call from tenant B against tenant A's conversations returns `404 not_found` or an empty list — zero cross-tenant rows in any response |
| Ordering (FR-007, SC-004) | Timeline returns identical order across repeated reads, including seeded same-`created_at` messages (tie-broken by `seq`); load-older pages never reorder or duplicate |
| Default view (FR-008, Q2) | Unfiltered `GET /tenant/conversations` returns only `open` conversations; `?status=all` returns every status |
| Filters (FR-009) | `status`/`assignee=me|unassigned|uuid`/`channel` narrow correctly, individually and combined; unknown values → `422` |
| Compose (FR-011) | Reply/note/logged-customer message appears in the timeline with correct kind, sender, and `logged_by`; `last_activity_at` bumps; empty and >10,000-char bodies → `422` |
| Auto-reopen (FR-012, Q1) | Reply or logged customer message on a `resolved`/`closed` conversation → response shows `status: open` + `conversation.status_changed` audit row with `auto: true`; a note leaves status unchanged |
| Status/assignment (FR-012/FR-013) | Any→any status PATCH works; assignment to an active member and unassignment work; inactive or cross-tenant membership → `422` with `assigned_membership_id` field detail |
| Creation (FR-014, Q3) | Create → `open`, unassigned, first message present; a second open conversation for the same customer+channel succeeds |
| RBAC (FR-015) | Viewer: GETs 200, POST/PATCH 403; Agent+: all 200/201 — verified per route via the rbac matrix |
| Audit (FR-017, SC-007) | Create/status/assignment each write exactly one audit row (actor, action, detail) in the write transaction; no-op PATCH values write none |
| Profile continuity (FR-018) | `GET /tenant/customers/{id}/conversations` still returns the 20-row window with the new status vocabulary |
| Performance (SC-002) | Seeded-volume check: inbox at 10,000 conversations and timeline page at 1,000 messages respond in under 1 second |

## Frontend validation

```bash
cd frontend

pnpm ng test dashboard    # API service, inbox + detail stores, inbox page, detail page, composer specs
pnpm ng build dashboard   # bundle budget respected
pnpm lint
pnpm format:check
```

**Expected outcomes**:

| Check | Expectation |
|-------|-------------|
| Inbox (US1) | Store defaults to `status=open`; filter changes reset cursor and reload; empty-filter state offers reset; badges map all four statuses |
| Detail (US2) | Timeline renders ascending with notes visually distinct; load-older prepends without reorder; participants and assignee (incl. inactive flag) shown |
| Composer (US3, Q4) | Reply/note/log modes switch; whitespace-only submit blocked client-side; submit appends entry and syncs conversation status from the response |
| Permissions (FR-015) | With only `conversations.view`: composer, status/assignee controls, and "New conversation" absent; detail route still reachable |
| Paths | Inbox → detail navigation uses `APP_PATHS.tenant.conversationDetail`; no string-literal routes in the feature |

## Manual smoke (optional, dev server)

1. Sign in as an Agent, open **Conversations** — inbox shows open conversations, newest activity first.
2. Create a conversation (existing customer, channel, first message) — appears at the top, `open`, unassigned.
3. Open it: send a reply, switch to note mode and add a note (styled differently), switch to log mode and record a customer message.
4. Set status `resolved`, then send another reply — status badge returns to `open` automatically.
5. Assign it to yourself — it appears under the "assigned to me" filter; unassign — appears under "unassigned".
6. Sign in as a Viewer — same conversation readable, no composer or controls; direct POST via devtools → `403`.
7. Open the customer's profile — the conversation appears in the history section with its current status.
