# Research: Tenant Team Management

**Feature**: 011-tenant-team-management | **Date**: 2026-07-12

All Technical Context unknowns resolved. Each decision below records what was chosen, why, and what was rejected.

## R1. Permissions: reuse the 008 catalog — zero new codes

**Decision**: Gate everything with the existing `members.view`, `members.manage`, and `owner.assign` permissions. No catalog or matrix change.

**Rationale**: Feature 008 already shipped exactly this vocabulary: `members.view`/`members.manage` are granted to Owner/Admin/Manager (and to platform staff per environment via `staff_tenant_permissions`), `owner.assign` is Owner-exclusive (asserted by the existing `tenant_role_hierarchy_and_owner_exclusives_hold` test). The 26-code parity test and frontend `Permission` union already contain them. Adding codes would duplicate meaning.

**Alternatives considered**: A dedicated `members.invite` code — rejected: inviting is a member-management act; splitting it adds matrix surface with no differing grant set.

## R2. Rank model and the Owner-on-Owner exception

**Decision**: Numeric rank owner=5, admin=4, manager=3, agent=2, viewer=1, defined once in `tenancy::members`. Rules enforced in-handler (after the route's `members.manage` gate):

1. Actor may act on a target member only if `actor_rank > target_rank` (strict), **except** an Owner actor may also act on Owner targets (rank-equal) — required by FR-008 ("only the Owner may change an Owner's role") and the ownership-transfer assumption; without it a co-Owner could never be demoted by anyone.
2. Assignable roles: `new_role_rank <= actor_rank`; assigning `owner` additionally requires the `owner.assign` permission (defense in depth — rank 5 and the permission coincide for members, but staff ranks are derived, see below).
3. Self-guard: `target.user_id != principal.user_id` for role change and disable.
4. Last-owner guard: an UPDATE that would demote or disable the only active `role = 'owner'` membership is refused (`409 conflict`).

Platform staff acting inside a tenant have no membership row; their rank derives from effective permissions: `owner.assign` → 5, else `members.manage` → 4. This makes non-production staff and production Super Admin owner-equivalent, production Support admin-equivalent — consistent with 008's staff matrix.

**Rationale**: A single integer comparison is auditable and testable as a matrix; deriving staff rank from permissions avoids inventing a parallel staff-rank table.

**Alternatives considered**: Pure permission-per-action checks without ranks — rejected: cannot express "Manager may not touch Admin" (clarification session answer A). DB-level rank enforcement — rejected: rules involve the acting principal, which the DB does not know; handler + transaction is the right layer.

## R3. Concurrency: transaction + row locks for member writes

**Decision**: `PATCH /tenant/members/{id}` runs in a transaction: `SELECT … FOR UPDATE` the target membership, run guards (including `SELECT count(*) … WHERE role='owner' AND status='active' AND deleted_at IS NULL FOR UPDATE` for the last-owner check), apply the update, insert the audit row, commit.

**Rationale**: Spec edge case demands one consistent final state under concurrent admin edits, and the last-owner invariant must not be racy (two concurrent demotions of two owners must not leave zero). Locking the owner rows serializes exactly the dangerous interleavings.

**Alternatives considered**: A DB trigger guarding the last owner — deferred: all writes flow through one handler; a trigger would duplicate the rule and complicate platform-side tenant administration (010) which may legitimately never touch memberships. Revisit if a second write path appears.

## R4. Membership disable = `status` column, immediacy for free

**Decision**: Migration `0018` adds `status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','disabled'))` to `tenant_memberships`. `authorize::fetch_membership_role` gains `AND status = 'active'`. Re-enable flips status back; the row keeps its `role`, satisfying "re-enablement restores the role held at disabling" with no extra storage.

**Rationale**: The tenant-context middleware already resolves the role from the database on **every** request — so a disabled membership fails authorization on the member's very next request (FR-010, SC-005) with zero session machinery. Soft-delete (`deleted_at`) is the wrong tool: it means "membership never existed" (unique index excludes it, cascade uses it), while disabled members must stay visible in the roster.

**Alternatives considered**: `disabled_at TIMESTAMPTZ` — equivalent; TEXT status chosen for symmetry with `tenants.status` and roster filtering ergonomics. Session revocation on disable — unnecessary (per-request evaluation) and wrong-scoped (the user may be active in other tenants).

## R5. Invitations: dedicated table, hashed single-use token

**Decision**: Migration `0019` creates `tenant_invitations`: `id`, `tenant_id` FK, `email CITEXT`, `role` (same CHECK vocabulary), `token_hash TEXT` (SHA-256 hex of a 256-bit random token, unique index), `status` CHECK pending/accepted/revoked, `invited_by` FK users, `expires_at` (created + 7 days), `accepted_at`/`revoked_at`/`accepted_user_id` NULL, timestamps. Partial unique index `(tenant_id, email) WHERE status = 'pending'` enforces FR-006's one-pending-per-address at the schema level. "Expired" is a derived state (`status = 'pending' AND expires_at < now()`), surfaced as `expired` in API responses — no cron needed.

The raw token appears exactly twice: in the `accept_url` returned to the inviter (copyable link) and in the outbound email. Only the hash is stored; lookups are `WHERE token_hash = digest($token)`.

**Rationale**: Storing only a hash means a database leak does not leak join capability (same posture as password hashing, Constitution III). Deriving expiry at read time avoids background jobs. The partial unique index turns a race between two admins inviting the same address into a clean 409.

**Alternatives considered**: Signed JWT invite tokens (stateless) — rejected: revocation and single-use require server state anyway. Reusing `tenant_memberships` with a `pending` status — rejected: a pending invite has no `user_id`, which the table requires, and mixing lifecycles muddies the membership guards.

## R6. Verification & email binding (clarification: email-bound + verified)

**Decision**: Acceptance enforces, in order: token valid (pending, unexpired, hash match) → accepting identity's email equals the invited email (CITEXT `=`, so case-insensitive by schema) → membership created. The invited address's verification is constituted by the invitation flow itself: when email delivery is configured, the token reaches the invitee **only** through their mailbox, so presenting it proves control of the address (the invitation email *is* the "confirmation sent to that address" of FR-005a). When email delivery is not configured, the copyable link hand-delivered by the admin is the spec's own guaranteed-path degradation (FR-015); the email-match rule still holds — a signed-in user with a different email is refused (403), and a new registrant's email field is fixed to the invited address (not editable).

Two acceptance modes on `POST /invitations/{token}/accept`:
- **Signed-in**: principal's email must match; adds the membership to the existing account.
- **Anonymous**: payload `{display_name, password}` creates the user with `email = invitation.email`, then the membership, then issues a session cookie (reusing identity's Argon2id + JWT session helpers) so the invitee lands signed-in.

A disabled membership in the same tenant blocks acceptance (409, per edge case — path back is re-enablement).

**Rationale**: Satisfies FR-005a without inventing a second OTP round-trip that would itself depend on the email infrastructure the spec says may be absent. No `email_verified_at` column is added: the platform has no account-wide verification concept yet, and this feature's guarantee is scoped to invitation acceptance; adding half a verification system here would be speculative.

**Alternatives considered**: Separate confirmation code emailed at acceptance time — rejected: circular dependency on optional email infra; doubles the invitee flow for no added proof when the token already traveled by email. Bearer link (accept with any account) — rejected by clarification answer C.

## R7. Email delivery: `EmailSender` port in `notifications`

**Decision**: The placeholder `notifications` crate gets its first interface: `EmailMessage { to, subject, body_text, body_html }` and trait `EmailSender { fn is_configured(&self) -> bool; async fn send(&self, msg) -> Result<(), EmailError> }`. Two implementations: `SmtpEmailSender` using `lettre` (tokio executor, rustls TLS) built when `AppConfig.smtp_url` + `smtp_from` are set; `LogEmailSender` otherwise (logs the message at info level, `is_configured() = false`). Invitation creation atomically stores the invitation and delivery payload in `outbox_events`, returning `queued` without waiting for SMTP. A supervised worker uses five-minute claim leases, performs SMTP outside database transactions, and makes at most three attempts before terminal failure/dead-letter. Claim-token finalization gives bounded at-least-once delivery: a crash after SMTP acceptance but before finalization may cause one later retry.

**Rationale**: Clarification answer C requires real sending when configured and graceful degradation when not. A port in `notifications` respects module boundaries (Constitution I) and gives password-reset/notification features a ready seam. `lettre` is the de-facto Rust SMTP crate with tokio + rustls support matching the stack.

**Alternatives considered**: Building email into `tenancy` — rejected: wrong module, unshareable. Provider SDK (SES/SendGrid) — rejected: SMTP is provider-neutral; an SDK can implement the same port later. Blocking send in the request path — rejected: SMTP latency/failures must not affect invitation creation (Constitution X, FR-015).

## R8. API shape: roster and invitations as sibling collections

**Decision**: Tenant-scoped endpoints (all under the existing tenant-context middleware):
- `GET /tenant/members` (`members.view`) — memberships ⋈ users, filters `q` (name/email ILIKE), `status` (active/disabled), cursor pagination per the 010 pattern.
- `PATCH /tenant/members/{id}` (`members.manage`) — partial update, exactly one of `{role}` or `{status}` per call; all R2/R3 guards.
- `GET /tenant/members/invitations` (`members.view`) — pending (and recently expired) invitations.
- `POST /tenant/members/invitations` (`members.manage`) — create; returns invitation + `acceptUrl` + truthful `emailSent`/`emailDeliveryStatus`.
- `GET /tenant/members/invitations/{id}/delivery` (`members.view`) — tenant-scoped targeted delivery status used for bounded UI polling.
- `DELETE /tenant/members/invitations/{id}` (`members.manage`) — revoke; guarded by rank (invitation's role must be assignable by the actor).

Public (token-capability, no session required): `GET /invitations/{token}` (preview: tenant name, invited email, role, expiry state, whether a matching account exists) and `POST /invitations/{token}/accept` (R6). Invalid/expired/revoked tokens answer 404/410 with no tenant detail beyond the preview contract.

The team page renders pending invitations as a section above the paginated member table (both carry status badges), satisfying "invitation appears in the roster as a pending member" without forcing a UNION cursor.

**Rationale**: Two homogeneous collections keep pagination single-statement (Constitution X) and contracts clean (Constitution V). A merged feed was rejected: cursor pagination across a UNION of heterogenous rows complicates ordering and buys nothing at 500-member scale — invitations are few and short-lived, and GitHub/Linear-style UIs present them as a distinct section anyway.

## R9. Frontend structure

**Decision**: `features/tenant/team/` with `team-api.service.ts` (Observable-returning, functional-interceptor HTTP per core rules), `team.store.ts` (NgRx SignalStore, `rxMethod` for debounced search/filter/cursor + invitations sub-state), `team-list.component.ts` (composes shared `data-table`, `status-badge`, `toolbar`, `search-input`, `empty-state`, `loading-state`), `invite-dialog.component.ts` (Taiga dialog via project wrapper; two steps: form → result with accept-link copy + email-sent indicator), `role-select.component.ts` (options filtered to ranks the current user may assign, driven by the server-provided permission set + own role from tenant context — presentation only; the server re-checks). Public `features/auth/invite/accept-invitation.component.ts` on `/invite/:token` (no auth guard; works signed-in and anonymous). Sidebar gains a "Team" item gated on `members.view`; `APP_PATHS`, `PAGE_TITLES`, `PAGE_PERMISSIONS` extended. Status badge mapping: active → success, invited → warning/notice, disabled → neutral, per the existing status-badge variants.

**Rationale**: Mirrors the 010 platform/tenants folder shape (service + SignalStore + pages) — feature-local state belongs in SignalStore per spec-002; all UI composes shared components (Constitution IX). Role-visibility on the client is presentation-only; enforcement stays server-side (008 FR-010: no frontend role→permission mapping — the client consumes the permission list and its own role rank from `/me`).

**Alternatives considered**: Putting the team page under `settings` — rejected: Manager holds member management but must not see settings (008 FR-002b); a settings-nested route would wrongly couple their gates.

## R10. Audit vocabulary

**Decision**: Extend `tenancy::audit` helpers with tenant-scoped actions: `member.invited`, `member.invitation_revoked`, `member.invitation_accepted`, `member.role_changed` (details: `{previous_role, new_role}`), `member.disabled`, `member.enabled`. Actor is the acting principal (for acceptance: the accepting user), `resource_type = "membership"` or `"invitation"`, `tenant_id` always set. Written app-side in the same transaction as the state change (consistent with 010's app-level `audit::record`; no DB trigger needed since all writes are handler-mediated).

**Rationale**: Constitution III requires who/what/when for role changes; spec SC-002 requires 100% coverage of all six lifecycle actions with before/after for role changes.

**Alternatives considered**: DB triggers per action — rejected: the slug-audit trigger exists because tenants had a pre-existing DB-level mutation concern; membership writes all flow through the new handlers, so app-side records (in-transaction) are simpler and carry the actor naturally.
