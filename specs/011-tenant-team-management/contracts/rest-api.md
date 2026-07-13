# REST API Contract: Tenant Team Management

**Feature**: 011-tenant-team-management | **Base**: `/api/v1`

Conventions inherited from `specs/001-ai-customer-service-platform/contracts/rest-api.md`: `ApiResponse` envelope, `ApiError` error body with `code`/`message`/(optional) `details[]`, `X-Request-Id` echo, cursor pagination (`limit`, `cursor`, `next_cursor`). Tenant-scoped routes require the session cookie **and** `X-Tenant-ID`; the tenant-context middleware resolves the effective role before any handler runs. All refusals of cross-tenant references use `404 not_found` (never confirm existence).

## Tenant-scoped endpoints

### GET `/tenant/members` — permission `members.view`

Roster of memberships joined with users. Single-statement query.

Query params: `q` (optional; ILIKE on display_name/email), `status` (optional; `active` | `disabled`), `limit` (default 25, max 100), `cursor`.

```json
200 → {
  "items": [
    {
      "id": "…membership uuid…",
      "user_id": "…",
      "display_name": "Amina Hassan",
      "email": "amina@acme.com",
      "role": "admin",
      "status": "active",
      "joined_at": "2026-07-01T09:00:00Z"
    }
  ],
  "next_cursor": null
}
```

### PATCH `/tenant/members/{id}` — permission `members.manage`

Partial update; body carries **exactly one** of `role` or `status` (422 otherwise).

```json
{ "role": "manager" }        // role change
{ "status": "disabled" }     // disable
{ "status": "active" }       // re-enable (role on the row is restored implicitly)
```

Handler guards (in-transaction, target row `FOR UPDATE`; see contracts/permissions.md for the rank model):

| Refusal | Status/code |
|---------|-------------|
| Target membership not in current tenant / unknown | 404 `not_found` |
| Actor rank not strictly above target (and not Owner-on-Owner) | 403 `forbidden` |
| New role above actor rank | 403 `forbidden` |
| Assigning `owner` without `owner.assign` | 403 `forbidden` |
| Actor targets self | 403 `forbidden` |
| Would demote/disable the last active Owner | 409 `conflict` |
| Unknown role/status value, or both/neither fields present | 422 `validation_failed` |

`200` returns the updated `TeamMember`. Audits `member.role_changed` (`{previous_role, new_role}`), `member.disabled`, or `member.enabled`. Effect is immediate (per-request role resolution).

### GET `/tenant/members/invitations` — permission `members.view`

Open invitations, newest first. `status` filter optional (`pending` and the default view include both elapsed pending invitations and persisted `expired` transitions).

```json
200 → { "items": [ {
  "id": "…", "email": "new@acme.com", "role": "agent",
  "status": "pending",                  // pending | accepted | revoked | expired
  "emailDeliveryStatus": "sent",        // unconfigured | queued | sent | failed
  "invitedByName": "Amina Hassan",
  "createdAt": "…", "expiresAt": "…"
} ], "nextCursor": null, "hasMore": false }
```

### POST `/tenant/members/invitations` — permission `members.manage`

```json
{ "email": "new@acme.com", "role": "agent" }
```

Guards: valid email (422); role assignable by actor's rank, `owner` additionally requires `owner.assign` (403); address not an active **or disabled** member of this tenant (409 `conflict`, message states why); no pending invitation for the address (409 — also enforced by the partial unique index under race).

```json
201 → {
  "invitation": {
    "id": "…", "email": "new@acme.com", "role": "agent", "status": "pending",
    "invitedByName": "Amina Hassan", "createdAt": "…", "expiresAt": "…",
    "emailDeliveryStatus": "queued"
  },
  "acceptUrl": "https://…/invite/<raw-token>",
  "emailSent": false,
  "emailDeliveryStatus": "queued"
}
```

`acceptUrl` embeds the raw single-use token. The invitation stores only its hash; the transactional outbox stores the delivery payload until processing completes so startup recovery can resume queued work. Configured delivery and its outbox event are committed atomically as `queued` before the response. Delivery itself is deliberately non-atomic: a supervised worker commits a short claim transaction, sends through `EmailSender` without a database transaction, then uses the claim token in a separate finalization transaction to update the tenant/id-scoped invitation and outbox event. Unconfigured delivery is persisted as `unconfigured`. `emailSent` is true only when the persisted status is `sent`, so it is false for the immediate `queued` response. Delivery latency or failure never blocks invitation creation. Audits `member.invited`.

### DELETE `/tenant/members/invitations/{id}` — permission `members.manage`

Revokes an open invitation. Guards: invitation in current tenant (else 404); invitation's role assignable by actor rank (403); status pending or persisted expired (409 `conflict` if already accepted/revoked). Revoking either an elapsed pending invitation or a persisted expired invitation explicitly transitions it to `revoked`. `204`. Audits exactly one `member.invitation_revoked`.

### GET `/tenant/members/invitations/{id}/delivery` — permission `members.view`

Tenant-scoped targeted status lookup used by the inviter UI; cross-tenant or unknown IDs return `404`.

```json
200 → { "emailDeliveryStatus": "queued" }
```

The UI polls once per second while queued. Each poll allows at most three consecutive transient request failures, one second apart; a successful response resets that failure sequence. Polling stops on `unconfigured`, `sent`, or `failed`, dialog reset, or tenant switch, and exposes an operation error after retry exhaustion.

Delivery processing uses a five-minute outbox claim lease and at most three SMTP attempts. SMTP occurs outside database transactions. Poison events and third-attempt failures are terminally dead-lettered and project `failed`; an expired third-attempt claim is recovered directly to terminal `failed` without a fourth send. Earlier stale claims become eligible after the lease. Delivery is bounded at-least-once: process loss after SMTP acceptance but before token-guarded finalization can cause a later duplicate attempt.

## Public endpoints (token capability, no session required)

CSRF: the origin middleware only applies to authenticated requests; the accept endpoint is additionally safe because the token is single-use and unguessable (256-bit).

### GET `/invitations/{token}`

Preview for the acceptance page. Token is matched by hash.

```json
200 → {
  "tenant_name": "Acme Corp",
  "email": "new@acme.com",
  "role": "agent",
  "expires_at": "…",
  "account_exists": true      // an active user with this email exists → UI shows sign-in path
}
404 not_found  → token unknown or revoked (indistinguishable by design)
410 gone       → token valid but expired, whether derived from `expires_at` or persisted as `expired` ("ask your admin for a new invitation")
```

### POST `/invitations/{token}/accept`

Two modes:

- **Signed-in** (session cookie present): empty body `{}`. Principal's email must equal the invited email (CITEXT equality) → 403 `forbidden` on mismatch ("This invitation was issued to a different email address").
- **Anonymous**: `{ "display_name": "…", "password": "…" }` — creates the account with `email = invitation.email` (Argon2id, same policy as 007), then the membership, then sets the `app_session` cookie in the response.

```json
200 → { …MeResponse (same shape as POST /auth/login)… }
```

| Refusal | Status/code |
|---------|-------------|
| Token unknown / revoked | 404 `not_found` |
| Expired | 410 `gone` |
| Signed-in email mismatch | 403 `forbidden` |
| Anonymous, but an active account already exists for the invited email | 409 `conflict` ("sign in to accept") |
| Existing membership in this tenant: disabled → 409 `conflict` ("membership disabled — ask an admin to re-enable"); active → 409 `conflict` (already a member) | 409 |
| Weak/missing password or display_name (anonymous mode) | 422 `validation_failed` |
| Tenant soft-deleted or suspended since invite | 404 `not_found` |

Acceptance is transactional and single-use (`UPDATE … WHERE status='pending' AND expires_at > now()` guards the race). Audits `member.invitation_accepted` with the accepting user as actor.

## Error vocabulary used

`unauthenticated` (401), `forbidden` (403), `not_found` (404), `conflict` (409), `gone` (410), `validation_failed` (422) — all from the existing `kernel::ApiError` constructors (`gone` added if not yet present; single constructor addition, same envelope).

## RBAC test matrix additions (server/tests/rbac.rs)

| Operation | owner | admin | manager | agent | viewer |
|-----------|-------|-------|---------|-------|--------|
| GET /tenant/members | ✅ | ✅ | ✅ | ❌ | ❌* |
| PATCH /tenant/members/{id} | ✅ | ✅ | ✅ | ❌ | ❌ |
| GET /tenant/members/invitations | ✅ | ✅ | ✅ | ❌ | ❌* |
| POST /tenant/members/invitations | ✅ | ✅ | ✅ | ❌ | ❌ |
| DELETE /tenant/members/invitations/{id} | ✅ | ✅ | ✅ | ❌ | ❌ |

\* Route gate is `members.view`, which the 008 matrix grants only to owner/admin/manager (plus staff per environment) — the spec (FR-001, as amended during planning) follows the 008 matrix as the single source of truth, so agent/viewer get 403 on the API and no Team nav item. Platform staff rows follow `staff_tenant_permissions` (full in non-production; in production Support/Developer/Sales/Finance hold `members.view` → list only, no writes; Super Admin full).
