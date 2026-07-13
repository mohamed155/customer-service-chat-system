# Data Model: Tenant Team Management

**Feature**: 011-tenant-team-management | **Date**: 2026-07-12

Two schema changes, both by migration (Constitution VIII). Existing tables (`users`, `tenants`, `audit_logs`) are unchanged.

## 1. `tenant_memberships` — add `status` (migration `0018_membership_status.sql`)

```sql
ALTER TABLE tenant_memberships
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active',
    ADD CONSTRAINT tenant_memberships_status_check CHECK (status IN ('active', 'disabled'));
```

| Column | Type | Notes |
|--------|------|-------|
| status | TEXT NOT NULL DEFAULT 'active' | `active` \| `disabled`. Disable/re-enable flips this; `role` is untouched, so re-enable restores the prior role by construction. |

Semantics:

- **Authorization**: `authorize::fetch_membership_role` adds `AND status = 'active'` — a disabled member's next request fails tenant authorization (spec FR-010, SC-005). `deleted_at IS NULL` filtering is unchanged.
- **Distinct from soft delete**: `deleted_at` means "membership gone" (excluded from the active-unique index, used by cascades); `status = 'disabled'` means "present in roster, access off". Disabled rows still occupy the `(tenant_id, user_id)` active-unique slot — intentionally, so a disabled person cannot re-enter via a fresh invitation (edge case: refused with 409).
- **Existing rows**: backfilled `active` by the DEFAULT.
- **Guards**: the `membership_guard_deleted_parent` trigger (0011) fires on `UPDATE OF user_id, tenant_id, deleted_at` — a status flip does not touch those columns, so no trigger interaction.

Invariants enforced in the handler transaction (research R2/R3):

- Last-owner: the only membership with `role = 'owner' AND status = 'active' AND deleted_at IS NULL` in a tenant can be neither demoted nor disabled (409).
- Self-guard: actor cannot change own role or status.
- Rank: actor rank strictly above target rank (Owner-on-Owner exception; see contracts/permissions.md).

## 2. `tenant_invitations` — new table (migration `0019_tenant_invitations.sql`)

```sql
CREATE TABLE tenant_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    email CITEXT NOT NULL,
    role TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    invited_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ NULL,
    accepted_user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    revoked_at TIMESTAMPTZ NULL,
    revoked_by UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tenant_invitations_email_format CHECK (position('@' in email::text) > 1),
    CONSTRAINT tenant_invitations_role_check CHECK (
        role IN ('owner', 'admin', 'manager', 'agent', 'viewer')
    ),
    CONSTRAINT tenant_invitations_status_check CHECK (
        status IN ('pending', 'accepted', 'revoked', 'expired')
    ),
    CONSTRAINT tenant_invitations_accepted_shape CHECK (
        (status = 'accepted') = (accepted_at IS NOT NULL AND accepted_user_id IS NOT NULL)
    ),
    CONSTRAINT tenant_invitations_revoked_shape CHECK (
        (status = 'revoked') = (revoked_at IS NOT NULL AND revoked_by IS NOT NULL)
    ),
    CONSTRAINT tenant_invitations_expired_shape CHECK (
        status <> 'expired' OR expires_at <= updated_at
    )
);

CREATE UNIQUE INDEX tenant_invitations_token_hash_uniq
    ON tenant_invitations (token_hash);

CREATE UNIQUE INDEX tenant_invitations_pending_email_uniq
    ON tenant_invitations (tenant_id, email)
    WHERE status = 'pending';

CREATE INDEX tenant_invitations_tenant_idx
    ON tenant_invitations (tenant_id, status);

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenant_invitations
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
```

| Column | Type | Notes |
|--------|------|-------|
| tenant_id | UUID FK | Tenant-owned (Constitution II); RESTRICT like sibling tables |
| email | CITEXT | Case-insensitive by type — matches `users.email` semantics |
| role | TEXT | Same five-role vocabulary as memberships |
| token_hash | TEXT, unique | SHA-256 hex of a 256-bit random token; raw token never stored (research R5) |
| status | TEXT | `pending` \| `accepted` \| `revoked` \| `expired`. Expiry is initially derived from an elapsed `expires_at`; replacement persists the explicit `expired` transition so it is not represented as admin revocation. |
| invited_by | UUID FK | Issuer, for audit/display |
| expires_at | TIMESTAMPTZ | `created_at + interval '7 days'` (spec assumption) |
| accepted_user_id | UUID FK NULL | Set on acceptance; ties invitation → resulting member |

Key rules:

- **One pending per address per tenant**: the partial unique index makes the duplicate-invite race a clean 409 (FR-006). Replacing an elapsed invitation first transitions it atomically from `pending` to `expired`, freeing the index slot.
- **Expiry shape**: persisted `expired` rows require `expires_at <= updated_at`, preventing a future-dated invitation from being prematurely marked expired.
- **Single use**: acceptance flips `pending → accepted` with `UPDATE … WHERE status = 'pending' AND expires_at > now()` inside the acceptance transaction — a second accept sees zero rows and gets 410.
- **No cascade needed**: tenant soft-delete already blocks membership creation via the 0011 guard; acceptance also re-checks tenant liveness (`fetch_tenant`).

### Invitation lifecycle

```text
            create (members.manage, rank-checked)
                      │
                      ▼
                  [pending] ──────── expires_at passes ────────▶ [expired]
                   │     │                                          │
        accept (token +  │ revoke (members.manage,                  │ accept → 410
        email match)     │         rank-checked)                    │ explicit admin revoke
                   ▼     ▼                                          ▼
            [accepted] [revoked] ◀──────────────────────────── [revoked]
```

Acceptance transaction (research R6): validate token → validate email binding → (anonymous mode: create user with `email = invitation.email`) → insert membership `(tenant_id, user_id, role, status='active')` → mark invitation accepted → audit `member.invitation_accepted` → (anonymous mode: issue session).

## 3. Entity → API projection

| Spec entity | Storage | API resource |
|-------------|---------|--------------|
| Team Member | `tenant_memberships` ⋈ `users` | `TeamMember { id, user_id, display_name, email, role, status: active\|disabled, joined_at }` |
| Invitation | `tenant_invitations` | `TenantInvitation { id, email, role, status: pending\|accepted\|revoked\|expired, invited_by_name, created_at, expires_at }`; elapsed pending and persisted expired rows have identical API semantics |
| Tenant Role | CHECK vocabulary + `authz::TenantRole` | string union `owner\|admin\|manager\|agent\|viewer` (display names per 008: Support Agent for `agent`) |
| Audit Record | `audit_logs` (existing) | not exposed by this feature (write-only here) |

## 4. Audit actions written (all in-transaction, app-side)

| Action | resource_type | details |
|--------|---------------|---------|
| `member.invited` | invitation | `{email, role}` |
| `member.invitation_revoked` | invitation | `{email, role}` |
| `member.invitation_accepted` | invitation | `{email, role, user_id}` |
| `member.role_changed` | membership | `{previous_role, new_role}` |
| `member.disabled` | membership | `{role}` |
| `member.enabled` | membership | `{role}` |
