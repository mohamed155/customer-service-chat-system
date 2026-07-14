# Quickstart Validation: Human Handoff & Routing

Runnable checks proving the feature end-to-end. Contracts: [rest-api.md](./contracts/rest-api.md), [events.md](./contracts/events.md), [permissions.md](./contracts/permissions.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running with the backend's standard env configuration (same setup as the existing live-gated suites); migrations 0001–0037 applied by the test harness/startup.
- `cargo` (backend), `pnpm` (frontend, run from `frontend/`).
- Seeded fixture per suite: two tenants; tenant A with ≥3 agent-capable members (varied skills/loads), a Viewer, seeded customers and open conversations (the integration suite provisions its own).

## Backend validation

```bash
cd backend

# Schema: 0035 skills + agent_skills (case-insensitive unique, composite FKs, cascade),
# 0036 agent_availability (state CHECK, default away), 0037 escalations
# (one-active partial unique, status/routing CHECKs, queue + load-count indexes, escalated_at)
REQUIRE_DB_TESTS=1 cargo test -p db --test schema

# Routing unit tests: ranking (most-matched-skills, load tie-break, membership-id determinism)
cargo test -p escalations

# Feature suite: every routing branch + queue + claim + presence + SSE + isolation + audit
REQUIRE_DB_TESTS=1 cargo test -p server --test escalations

# RBAC matrix: all new routes across all roles
REQUIRE_DB_TESTS=1 cargo test -p server --test rbac

# Full backend gate
REQUIRE_DB_TESTS=1 cargo test
```

**Expected outcomes** (each maps to a spec item):

| Check | Expectation |
|-------|-------------|
| Skill match (FR-005, SC-002) | Escalation requiring `billing` with one available billing agent → `201` status `assigned`, routing reason `skill_match`, `matchedSkills: ["billing"]` |
| Most-skills ranking (FR-005) | Two required skills; agent matching both beats agent matching one, regardless of load |
| Load tie-break (FR-005) | Two equally-matching agents → lower open/pending count wins; equal load → deterministic (membership id) |
| Load fallback (FR-006) | No available agent matches any required skill (or no skills given) → least-loaded available agent, reason `load_fallback` |
| Queue placement (FR-003/FR-008) | No available+present agents → status `queued`; queue lists it ordered `escalated_at ASC` with reason/skills/waiting time |
| Duplicate guard (FR-002) | Second escalate on an active escalation → `409 escalation_active`; after close-out, a new escalation succeeds (new row) |
| Skill-aware drain (FR-014, Q2) | Queue [older: needs `arabic`, newer: needs `billing`]; billing-skilled agent toggles available → gets the *newer* entry (`queue_auto`); agent matching neither → gets the *oldest* |
| One-at-a-time drain (edge case) | Three queued + one agent becomes available → exactly one auto-assignment per capacity re-evaluation, not all three at once |
| Claim + contention (FR-011/FR-013, US2-3) | Two concurrent claims on one queued escalation → exactly one `200` (reason `manual_claim`), other gets `409 already_claimed` with winner's membership id; away agents can claim |
| Availability (FR-016/FR-017) | Default away; toggle available → eligible; toggle away → existing assignments untouched, no new routing; Viewer has no availability surface (`403`/`422`) |
| Presence revert (FR-017a, Q5) | Drop an available agent's SSE connection(s) → after grace window their DB state is `away`, `availability.changed { cause: "presence_timeout" }` emitted; routing never selects an available-but-absent agent |
| SSE delivery (FR-025, SC-008) | Connected target agent receives `escalation.assigned` (with reason + routing reason) within 5 s of assignment; other members receive `escalation.queued`/`escalation.removed`; no event ever crosses tenants |
| Close-out (FR-015) | Resolving/closing a queued conversation removes it from the queue, escalation → `closed`, `conversations.escalated_at` cleared (outbox consumer) |
| Manual reassignment (FR-023) | PATCH assignment on an escalated conversation → routing reason becomes `manual_reassignment` (origin loop-guard: engine's own assignments don't relabel) |
| Skills CRUD (FR-018/FR-019) | Create/rename/delete with case-insensitive uniqueness (`409` on "Billing" vs "billing"); delete strips links + queued `requiredSkillIds` but escalation `requiredSkills[].name` snapshots persist |
| Isolation (FR-020, SC-006) | Tenant B's calls against tenant A's escalations/skills/queue → `404`/empty; tenant B's available agents are never candidates for tenant A's escalations |
| Audit (FR-004, SC-001) | Every escalation, assignment (with reason + matched skills), claim, close-out, skill change, and availability change writes exactly one append-only audit row; reconciliation: every escalation row is `assigned`, `queued`, or `closed` — never dangling |
| Inbox filter (FR-001a) | `GET /tenant/conversations?escalated=true` returns exactly the conversations with an active escalation, combinable with status/channel filters |

## Frontend validation

```bash
cd frontend

pnpm ng test dashboard    # realtime service, notifications service, availability toggle,
                          # queue store/page, escalation banner, routing reason, inbox filter specs
pnpm ng build dashboard   # bundle budget respected
pnpm lint
pnpm format:check
```

**Expected outcomes**:

| Check | Expectation |
|-------|-------------|
| Realtime service (R1) | Sends cookie + `X-Tenant-ID`; parses event types; reconnects with backoff; store refetch on reconnect |
| Availability toggle (US3) | Visible in topbar for `conversations.manage` holders only; reflects server state incl. `availability.changed` auto-revert events; toggle-on requests browser-notification permission once |
| Queue page (US2, SC-004) | Entries show reason, skill chips, customer, channel, live waiting time, longest-waiting first; claim ≤2 interactions; `409` on claim race shows "already claimed" and removes the row; empty state renders |
| Notifications (FR-025) | `escalation.assigned` → in-app toast/badge always; browser notification only when permission granted and tab unfocused |
| Banner + reason (US5) | Escalated conversation shows banner (when + reason); routing reason rendered in plain language for all five reasons; never-escalated conversation shows neither |
| Inbox filter (FR-001a) | "Escalated" chip narrows the inbox and combines with existing filters |
| Skills manager (US4) | Catalog CRUD + per-agent assignment behind `members.manage`; read-only chips for `members.view`; Viewer memberships not assignable |
| Paths | Queue navigation uses `APP_PATHS.tenant.escalations`; no string-literal routes |

## Manual smoke (optional, dev server, two browsers)

1. Browser 1: sign in as Agent A (skill `billing`), toggle **Available** — accept the notification permission prompt.
2. Terminal: escalate an open conversation requiring `billing` (`POST /tenant/conversations/{id}/escalate`) — Agent A gets a browser/in-app notification within seconds; conversation shows the escalation banner and "matched skills: billing".
3. Toggle Agent A **Away**, escalate another conversation with no skills — it assigns to the least-loaded *other* available agent (`load_fallback`), or queues if none.
4. With all agents away, escalate a third conversation — it appears on the **Escalations** queue page with reason and waiting time.
5. Browser 2: sign in as Agent B (away), open the queue, **Claim** it — row disappears in Browser 1's queue too; detail shows "claimed by Agent B".
6. Toggle Agent A available with entries still queued — the oldest skill-compatible entry auto-assigns to A (`auto-assigned from queue`).
7. Close Browser 1 entirely; after ~a minute Agent A's toggle (on re-login) shows **Away** (presence auto-revert).
8. Resolve a queued conversation — it leaves the queue on its own.
9. In the inbox, apply the **Escalated** filter — only conversations with active escalations remain.
