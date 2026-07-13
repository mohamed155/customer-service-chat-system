# Quickstart: Tenant Team Management

**Feature**: 011-tenant-team-management — validation guide (contracts in [contracts/](./contracts/), schema in [data-model.md](./data-model.md))

## Prerequisites

- PostgreSQL with migrations applied through `0019` (`backend/migrations/`); live-DB tests expect `DATABASE_URL` (see `backend/crates/server/tests/` conventions — suites are live-gated).
- Backend: `cd backend && cargo build`. Frontend: `cd frontend && pnpm install`.
- Optional for the email path: set the SMTP settings (`SMTP_URL`, `SMTP_FROM` — names per `shared/config`) to any dev SMTP sink (e.g., Mailpit on `smtp://localhost:1025`). Without them, invitation creation still works and responses carry `email_sent: false`.

## Automated validation

```powershell
# Backend — schema, RBAC matrix, team suite
cd backend
cargo test -p db --test schema           # 0018/0019 assertions
cargo test -p server --test rbac         # role×operation matrix incl. new member/invitation ops
cargo test -p server --test team_members # roster, invite lifecycle, hierarchy, guards, immediacy, audit

# Frontend — service/store/page/dialog specs + gates
cd ../frontend
pnpm ng test dashboard
pnpm ng build dashboard
pnpm lint
pnpm format:check
```

Expected: all pass. `team_members.rs` covers each user story's acceptance scenarios; the rbac matrix asserts allow/deny for all five tenant roles (and staff variants) on the five tenant-scoped operations.

## Manual end-to-end walkthrough (maps to user stories)

Seed or reuse a tenant with an Owner account; sign in as an Admin of that tenant (dev identity header or real login per 007).

1. **US1 — roster**: open the dashboard → sidebar shows **Team** (Owner/Admin/Manager only) → page lists members with role + status badges; search narrows by name/email; a Viewer session gets no Team item and `GET /api/v1/tenant/members` returns 403.
2. **US2 — invite**: Team → *Invite* → enter a fresh email, pick *Support Agent* → dialog result shows the copyable accept link and whether the email was sent; the address appears under *Pending invitations*. Open the accept link in a private window → preview names the tenant and role → register (email field fixed) → land signed-in inside the tenant as Support Agent. Re-inviting the same address → clear duplicate error. Revoke a pending invite → its link now shows "invitation not found".
3. **US3 — role change**: as Admin, change the new member to *Manager* → roster updates; in the member's open session the next navigation reflects Manager access without re-login. Attempt to give someone *Owner* as Admin → refused; as a Manager, attempt to change an Admin → refused (hierarchy).
4. **US4 — disable**: disable the member → their very next request in the open session is refused (dashboard drops to tenant-select/safe state); roster shows *Disabled*; re-enable → access restored with the same role. Attempt to disable yourself or the only Owner → refused.
5. **Audit (SC-002)**: `SELECT action, actor_user_id, details, created_at FROM audit_logs WHERE tenant_id = '<tenant>' ORDER BY created_at DESC;` → one row per action above (`member.invited`, `member.invitation_accepted`, `member.role_changed` with previous/new, `member.disabled`, `member.enabled`, `member.invitation_revoked`).
6. **Isolation (SC-003)**: repeat `GET /tenant/members` with the other tenant's `X-Tenant-ID` using this tenant's session → 403/404, no data; a crafted `PATCH /tenant/members/{other-tenant-membership-id}` → 404.

## cURL smoke (contract sanity)

```bash
# Roster (session cookie + tenant header assumed)
curl -s -b "$COOKIES" -H "X-Tenant-ID: $TENANT" localhost:8080/api/v1/tenant/members | jq

# Invite
curl -s -b "$COOKIES" -H "X-Tenant-ID: $TENANT" -H "Content-Type: application/json" \
  -d '{"email":"new@acme.test","role":"agent"}' \
  localhost:8080/api/v1/tenant/members/invitations | jq '.accept_url, .email_sent'

# Public preview (no auth)
curl -s localhost:8080/api/v1/invitations/$RAW_TOKEN | jq

# Accept anonymously (creates account + session)
curl -s -i -H "Content-Type: application/json" \
  -d '{"display_name":"New Agent","password":"correct-horse-battery"}' \
  localhost:8080/api/v1/invitations/$RAW_TOKEN/accept
```

Expected statuses per [contracts/rest-api.md](./contracts/rest-api.md) (404 unknown/revoked token, 410 expired, 403 email mismatch, 409 duplicate/last-owner/disabled-membership, 422 validation).
