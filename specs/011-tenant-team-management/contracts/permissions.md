# Permissions Contract: Tenant Team Management

**Feature**: 011-tenant-team-management

## Catalog delta: none

The 008 catalog (26 codes) already contains everything this feature needs. **No new permission codes, no matrix changes.**

| Permission | Granted to (tenant roles) | Used here for |
|------------|---------------------------|---------------|
| `members.view` | Owner, Admin, Manager | GET roster, GET invitations; Team page + sidebar item |
| `members.manage` | Owner, Admin, Manager | PATCH member, POST/DELETE invitation |
| `owner.assign` | Owner only | Assigning/transferring the `owner` role (in-handler check on top of `members.manage`) |

Platform staff inside a tenant follow the existing `staff_tenant_permissions`: non-production → full tenant set (all three); production → Super Admin full; Developer/Sales/Finance hold `members.view` only; Support holds neither `members.manage` nor `owner.assign` beyond its defined set (list per matrix.rs).

## Rank model (in-handler, `tenancy::members`)

| Role | Rank |
|------|------|
| owner | 5 |
| admin | 4 |
| manager | 3 |
| agent | 2 |
| viewer | 1 |

Rules applied inside handlers, after the route permission gate:

1. **Manage-below**: actor may act on a target only if `actor_rank > target_rank`; **exception**: an Owner actor may act on Owner targets (FR-008 — only the Owner changes an Owner's role; enables ownership transfer + co-owner demotion).
2. **Assign-at-or-below**: `new_role_rank ≤ actor_rank`; `owner` additionally requires `owner.assign`.
3. **Self-guard**: no self role change, no self disable.
4. **Last-owner guard**: the only active Owner can be neither demoted nor disabled (409).
5. **Invitations**: creating/revoking an invitation requires the invitation's role to satisfy rule 2 for the actor.

**Staff rank derivation** (no membership row): `owner.assign` in effective permissions → rank 5; else `members.manage` → rank 4. Consequence: production platform Support (no `members.manage` in `STAFF_PRODUCTION_SUPPORT`) cannot manage members; non-production staff and production Super Admin act as rank 5.

## Route → permission map (all registered via `.guarded()` / deny-by-default)

| Route | Method | Permission |
|-------|--------|------------|
| `/tenant/members` | GET | `members.view` |
| `/tenant/members/{id}` | PATCH | `members.manage` |
| `/tenant/members/invitations` | GET | `members.view` |
| `/tenant/members/invitations` | POST | `members.manage` |
| `/tenant/members/invitations/{id}` | DELETE | `members.manage` |
| `/invitations/{token}` | GET | public (token capability) |
| `/invitations/{token}/accept` | POST | public (token capability) |

## Frontend page permissions

| Surface | Gate |
|---------|------|
| `PAGE_PERMISSIONS` — tenant team page (`/t/…/team`) | `members.view` |
| Sidebar "Team" item | `members.view` |
| Invite button / row actions (role change, disable, enable) | `members.manage` (presentation only — server re-checks) |
| Owner option in role selector | own role is `owner` (proxy for `owner.assign`; presentation only) |
| Role selector options | ranks ≤ own rank (presentation only) |
| `/invite/:token` acceptance page | public route (no authGuard/guestGuard — usable signed-in and anonymous) |

No frontend role→permission mapping is introduced (008 FR-010): the client consumes the server-provided permission list from `/me`; rank-based option filtering uses the user's own role string from the same response, and every decision is re-enforced server-side.
