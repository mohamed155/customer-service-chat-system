# Implementation Plan: Tenant Team Management

**Branch**: `011-tenant-team-management` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/011-tenant-team-management/spec.md`

## Summary

Give tenant-side managing roles (Owner/Admin/Manager) self-service control of their team: a members roster (memberships joined with users, plus pending invitations), email-bound single-use invitations with dual delivery (copyable acceptance link always; automatic email when SMTP is configured), rank-checked role changes, and reversible tenant-scoped disablement — all audited and enforced server-side.

Technical approach: **no new permission codes** — the 008 catalog already defines `members.view`, `members.manage`, and `owner.assign`, and the matrix already grants them to the right roles. Backend adds two migrations (`status` column on `tenant_memberships`; new `tenant_invitations` table with hashed tokens), tenant-scoped member/invitation endpoints in the `tenancy` module registered through the fail-closed `.guarded()` builder, two public invitation endpoints (token = capability), and an `EmailSender` port in the `notifications` module (SMTP via lettre when configured, logging fallback otherwise). Disablement is immediate for free because `fetch_membership_role` already resolves the role from the database on every request — it merely gains an `AND status = 'active'` predicate. Hierarchy (strict below-rank management, at-or-below-rank assignment, Owner-on-Owner exception, last-owner and self guards) is enforced in the handler inside a transaction with `SELECT … FOR UPDATE`. Frontend adds `features/tenant/team/` (Observable data service + SignalStore, RxJS-first), an invite dialog with role selector, the shared status-badge for member status, and a public `/invite/:token` acceptance page.

## Technical Context

**Language/Version**: Backend Rust (edition 2024); Frontend TypeScript ~6.0 / Angular 22 (standalone, signals, zoneless, OnPush)

**Primary Dependencies**: Axum, SQLx (PostgreSQL), existing `authz`/`tenancy`/`identity` module crates; `notifications` module gains its first real content (EmailSender port; `lettre` with tokio + rustls for the SMTP implementation); `sha2` for token hashing (already in the dependency tree via workspace); Angular Router, Reactive Forms, NgRx SignalStore, existing `core/authz` + shared components; RxJS operators for all new async flows (constitution v1.2.0)

**Storage**: PostgreSQL — migration `0018` adds `status TEXT NOT NULL DEFAULT 'active'` (CHECK: active/disabled) to `tenant_memberships`; migration `0019` creates `tenant_invitations` (tenant-owned: `tenant_id` FK, CITEXT email, role CHECK reusing the five-role vocabulary, `token_hash` unique, status pending/accepted/revoked, `expires_at`, partial unique index one-pending-per-(tenant,email)). Existing guards remain load-bearing: `membership_guard_deleted_parent` (0011) and the soft-delete cascade (0005). `users.email` is already CITEXT → case-insensitive invite-email matching comes from the schema, not app code.

**Testing**: `cargo test` — extend `backend/crates/server/tests/rbac.rs` (new operations in the role×operation matrix) + new live-gated suite `backend/crates/server/tests/team_members.rs` (roster/search/pagination, invite lifecycle incl. duplicate/expiry/revocation/email-mismatch, hierarchy matrix, last-owner + self guards, disable immediacy, audit rows) + `shared/db/tests/schema.rs` additions for the two migrations; Vitest for service/store/page/dialog specs

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — existing Cargo workspace + Angular pnpm workspace

**Performance Goals**: Roster is a single query (memberships ⋈ users with search + status filter + cursor in one statement, following the 010 list pattern); invitation list a second small query; no N+1; SC-007's 500-member roster served by the existing `(tenant_id, user_id)` index prefix; write paths add only the audit insert + the FOR UPDATE transaction

**Constraints**: Deny-by-default routing (`.guarded()` — permission is a required argument); tenant-scoped routes mounted through `mount_tenant` so the existing tenant-context middleware enforces isolation before handlers run; public invitation endpoints are capability-guarded by a hashed single-use token and rate-limited by token unguessability (256-bit); 401/403/404/409/410/422 from the existing `kernel::ApiError` vocabulary; cross-tenant references answered with `not_found` (spec: never confirm existence); schema changes via migration only (Constitution VIII); RxJS-first frontend async; no frontend role→permission mapping (008 FR-010); email delivery is optional infrastructure — its absence must never fail invitation creation (spec FR-015)

**Scale/Scope**: 2 migrations; 0 new permission codes; 5 tenant-scoped endpoints + 2 public endpoints; 1 new backend port (EmailSender) + 2 implementations; ~2 frontend pages (team list, invite acceptance) + 1 dialog + 1 role-select + 1 data service + 1 SignalStore; 1 sidebar nav item; audit vocabulary +6 actions

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Member/invitation routes live in `tenancy` (already owns memberships + tenant audit); email sending is a port in `notifications` consumed via its public interface — no cross-module data access; `authz` untouched except tests | ✅ Pass |
| II. Multi-Tenant Isolation | `tenant_invitations` carries `tenant_id`; every roster/member/invitation query filters by the middleware-resolved tenant; disable is per-membership (other tenants unaffected); public acceptance endpoints resolve the tenant from the invitation row, never from client input | ✅ Pass |
| III. Zero-Trust Security & RBAC | Reuses `members.view`/`members.manage`/`owner.assign` with deny-by-default registration; in-handler rank checks on top of route gates; invitation tokens stored hashed (never logged); role changes, disables, and the full invitation lifecycle audited with actor/action/before-after/time | ✅ Pass |
| IV. AI Provider Independence | Not touched | ✅ N/A |
| V. API-First & Contract Consistency | Endpoints documented in `contracts/rest-api.md`; cursor pagination + error envelope reused; PATCH is partial-update; invite creation idempotency conflict → 409 | ✅ Pass |
| VI. Observability by Default | Request-id/tracing unchanged; persisted delivery state and a targeted tenant-scoped status endpoint expose queued/sent/failed outcomes; audit trail append-only; terminal failures are dead-lettered with the last error | ✅ Pass |
| VII. Test-First & Regression Discipline | rbac matrix extension + dedicated integration suite (hierarchy matrix, guards, immediacy) + schema tests + frontend specs per story | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration-only; CHECKs mirror existing vocabulary; partial unique index enforces one pending invite per (tenant, email); `token_hash` unique index is the production lookup path; last-owner guard enforced in a transaction (FOR UPDATE) — DB trigger deferred as unneeded duplication while all writes flow through the one handler | ✅ Pass |
| IX. Design System Discipline | Team page composes existing shared components (data-table, status-badge, toolbar, search-input, empty/loading states); role-select and invite-dialog built as reusable pieces; no raw Taiga styling in feature pages | ✅ Pass |
| X. Performance & Efficiency | Single-statement roster query; per-request role lookup already exists; invitation creation writes a transactional outbox event and never waits for SMTP; the worker claims briefly, releases database locks before I/O, and retries at most three times | ✅ Pass |

**Initial gate**: PASS — no violations, Complexity Tracking not required.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no deviations. Two nuanced calls, both grounded in clarifications: (1) verification of the invited address is constituted by the single-use token + email-match binding (the invitation email *is* the confirmation sent to that address); the admin-shared link path is the spec's own graceful-degradation clause. (2) The Owner-on-Owner exception to strict hierarchy implements FR-008's "only the Owner may change an Owner's role" and the ownership-transfer assumption.

## Project Structure

### Documentation (this feature)

```text
specs/011-tenant-team-management/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── rest-api.md      # Roster, member PATCH, invitation CRUD + public accept endpoints, errors, audit actions
│   └── permissions.md   # Reused permission codes, rank model, route→permission map, page permissions
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0018_membership_status.sql          # NEW — status column (active/disabled) + CHECK
│   └── 0019_tenant_invitations.sql         # NEW — invitations table, token_hash unique, one-pending partial index
└── crates/
    ├── modules/
    │   ├── notifications/
    │   │   ├── Cargo.toml                  # MODIFIED — lettre (tokio, rustls), async-trait
    │   │   └── src/
    │   │       ├── lib.rs                  # MODIFIED — EmailSender port, EmailMessage type
    │   │       ├── smtp.rs                 # NEW — SmtpEmailSender (lettre) when SMTP configured
    │   │       └── noop.rs                 # NEW — LogEmailSender fallback (is_configured() = false)
    │   ├── tenancy/src/
    │   │   ├── lib.rs                      # MODIFIED — export members/invitations modules
    │   │   ├── members.rs                  # NEW — roster list, PATCH member (role/status) handlers + rank rules
    │   │   ├── invitations.rs              # NEW — create/list/revoke + public preview/accept handlers, token hashing
    │   │   ├── authorize.rs                # MODIFIED — fetch_membership_role gains AND status = 'active'
    │   │   └── audit.rs                    # MODIFIED — member.* action constants/helpers
    │   └── identity/src/
    │       └── routes.rs                   # MODIFIED — expose account-creation + session-issue helper reused by invite acceptance
    ├── shared/
    │   ├── config/src/lib.rs               # MODIFIED — optional SMTP settings (url, from) in AppConfig
    │   └── db/tests/schema.rs              # MODIFIED — 0018/0019 schema assertions
    └── server/
        ├── src/router.rs                   # MODIFIED — tenant member/invitation routes via .guarded(); public /invitations/{token} routes
        └── tests/
            ├── rbac.rs                     # MODIFIED — new operations in the role×operation matrix
            └── team_members.rs             # NEW — roster/invite-lifecycle/hierarchy/guards/immediacy/audit suite

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts            # MODIFIED — TeamMember, MemberStatus, TenantInvitation, payloads
│   ├── authz/permissions.ts                # MODIFIED — PAGE_PERMISSIONS gains tenant team page (members.view)
│   └── router/
│       ├── app-paths.ts                    # MODIFIED — tenant.team + public invite path
│       └── page-title.ts                   # MODIFIED — team + invite acceptance titles
├── layout/sidebar/…                        # MODIFIED — "Team" nav item gated on members.view
├── app.routes.ts                           # MODIFIED — public /invite/:token route (no auth guard)
└── features/
    ├── auth/invite/
    │   └── accept-invitation.component.ts  # NEW — token preview, register-or-accept flow, error states
    └── tenant/
        ├── tenant.routes.ts                # MODIFIED — team child route (members.view)
        └── team/
            ├── team-api.service.ts         # NEW — Observable API access (roster, invitations, patch, revoke)
            ├── team.store.ts               # NEW — SignalStore: search/filter/cursor + invitations state via rxMethod
            ├── team-list.component.ts      # NEW — pending-invitations section + member table + actions
            ├── invite-dialog.component.ts  # NEW — email + role form, accept-link + email_sent result step
            └── role-select.component.ts    # NEW — role selector limited to assignable ranks
```

**Structure Decision**: Backend follows the established pattern — membership and invitation logic joins `tenancy` (which already owns membership queries, tenant audit, and the `/tenant` route family), authorization vocabulary stays in `authz` (unchanged), registration in `server/router.rs`, and email delivery becomes the `notifications` module's first real interface so later features (password reset, notifications) reuse the same port. Frontend adds the first tenant-area management feature folder (`features/tenant/team/`) with feature-scoped service + SignalStore per the spec-002 state rules; the public acceptance page lives in `features/auth/` beside the other unauthenticated screens.

## Complexity Tracking

No constitution violations — table intentionally empty.
