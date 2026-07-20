# Quickstart: Audit Logs (026) — validation guide

Proves the feature end-to-end. Contracts: [contracts/audit-api.md](contracts/audit-api.md); data model: [data-model.md](data-model.md).

## Prerequisites

- PostgreSQL up with migrations applied through `0053_audit_read_indexes.sql` (`cd backend && sqlx migrate run` or the project's usual migration flow).
- Backend: `cd backend && cargo run -p server` (API at `http://localhost:<port>/api/v1`).
- Frontend: `cd frontend && pnpm ng serve dashboard`.
- Seeded users: one tenant Owner/Admin, one tenant Agent, one platform user (any platform role). Dev identity flow per 006/007 (login sets `app_session` cookie; tenant calls need `X-Tenant-ID`).

## Automated checks (all must pass)

```bash
# Backend — unit + matrix + coverage gates (DB-gated tests need TEST_DATABASE_URL per repo convention)
cd backend && cargo test -p authz -p audit -p server

# Frontend
cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check
```

Key suites: `server/tests/audit_logs.rs` (isolation, filters, cursor, actor labeling), `server/tests/rbac.rs` (new permission rows), `server/tests/openapi_coverage.rs` (2 new EXPECTED entries), `authz` matrix tests (30-permission catalog).

## Scenario 1 — Tenant admin views & filters (US1 / SC-002)

1. As tenant Admin, perform a sensitive action (e.g., change a member's role via Members page).
2. Open Dashboard → Audit Logs (tenant area). Expect: newest-first table; the role change appears with actor, action `member.role_changed`, target, timestamp.
3. Apply category `members` + today's date range → row remains; switch category to `auth` → row disappears; clear → empty-state never errors.
4. Click the row → detail drawer shows full metadata (`from`/`to` role fields).
5. `curl` equivalent: `GET /api/v1/tenant/audit-logs?category=members` with session cookie + `X-Tenant-ID` → 200, entry present.

## Scenario 2 — RBAC denial (US1-AC4 / SC-003 / FR-008)

1. As tenant Agent (or Viewer/Manager): Audit Logs nav entry absent; direct route blocked by guard.
2. `curl GET /api/v1/tenant/audit-logs` as Agent → **403**.
3. Any write attempt is impossible by contract (no endpoint); direct SQL `UPDATE audit_logs …` → trigger exception (immutability).

## Scenario 3 — Tenant isolation (US1-AC5 / SC-004)

1. Seed actions in tenants A and B.
2. As Admin of A: list returns only A's rows; every `tenant_id` = A regardless of filters.
3. Covered by `audit_logs.rs` integration test with two tenants.

## Scenario 4 — Platform-wide view (US2)

1. As platform user: open platform Audit Logs page. Expect rows from multiple tenants **and** tenant-less platform rows (e.g., `platform.tenant_created`).
2. Filter `tenant_id=<A>` → only A's rows.
3. As tenant-only user: `GET /api/v1/platform/audit-logs` → **403**.

## Scenario 5 — Coverage & attribution (US3 / SC-001)

Perform one action per category and verify a correctly attributed entry appears:

| Action to perform | Expected `action` |
|---|---|
| Fail a login (wrong password) | `auth.login_failed`, actor `system` |
| Update tenant settings/profile | `platform.tenant_updated` or `tenant.*` |
| Change a member role | `member.role_changed` |
| Save a new prompt version (AI Agent → Prompt) | `agent_prompt.version_created` |
| Set/rotate an AI credential | `ai_credential.set` |
| Trigger a tool execution in a conversation | `tool.executed` (**new writer**) |

Billing: no billing actions exist yet — category is reserved; nothing to verify (documented assumption).

## Scenario 6 — Staff visibility in tenant view (FR-013)

1. As platform SuperAdmin, switch into tenant A and change something audited.
2. As tenant A Admin: entry visible with the staff actor's name and a platform-staff badge.

## Scenario 7 — Edge cases

- Soft-delete a user who has audit entries → their rows still render, actor labeled deleted (FR-011).
- Filters matching nothing → empty state, no error.
- Page through > `limit` rows via `next_cursor` until `has_more: false`; no duplicates/skips across pages.

## Scenario 8 — Performance at volume (SC-005)

Automated equivalent: T046 (`cargo test -p server -- --ignored`). Manual check:

1. Seed ~50,000 audit rows for one tenant (see the bulk-insert described in T046).
2. Open the tenant Audit Logs page: the first page renders in under 2 seconds.
3. Change the category filter and the date range: each re-query returns in under 2 seconds.
4. Page through with "Load more" several times: later pages stay as fast as the first — keyset pagination does not degrade with depth the way offset paging does.
