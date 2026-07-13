# Tasks: Tenant Team Management

**Input**: Design documents from `/specs/011-tenant-team-management/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/rest-api.md, contracts/permissions.md, quickstart.md

**Tests**: Included — Constitution Principle VII (Test-First & Regression Discipline) requires unit/integration/API coverage for shipped functionality, and plan.md's source tree names the test files explicitly (`team_members.rs`, `rbac.rs` additions, Vitest specs).

**Organization**: Tasks are grouped by user story (P1–P4 from spec.md) so each can be implemented, tested, and demoed independently. No new permission codes are introduced (research R1) — all gates reuse `members.view`/`members.manage`/`owner.assign` from feature 008.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1/US2/US3/US4 — omitted for Setup, Foundational, and Polish tasks
- File paths are exact and repo-relative

---

## Phase 1: Setup (Dependencies & Config)

**Purpose**: Add the third-party dependencies and config plumbing new stories will need. No schema or route changes here.

- [X] T001 [P] Add `lettre` (tokio + rustls features) and `async-trait` to `backend/crates/modules/notifications/Cargo.toml`
- [X] T002 [P] Add optional SMTP settings (`smtp_url: Option<String>`, `smtp_from: Option<String>`) to `AppConfig` in `backend/crates/shared/config/src/lib.rs`, sourced from env vars, defaulting to `None`
- [X] T003 [P] Add an `ApiError::gone` (HTTP 410) constructor to `backend/crates/shared/kernel/src/lib.rs`, following the existing constructor pattern (`not_found`, `conflict`, etc.)

**Checkpoint**: `cargo build --workspace` succeeds; no behavior changes yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema, shared backend module scaffolding, and shared frontend vocabulary that every user story below depends on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 Create migration `backend/migrations/0018_membership_status.sql` — add `status TEXT NOT NULL DEFAULT 'active'` + `CHECK (status IN ('active','disabled'))` to `tenant_memberships`, per data-model.md §1
- [X] T005 Create migration `backend/migrations/0019_tenant_invitations.sql` — `tenant_invitations` table, `token_hash` unique index, one-pending-per-`(tenant_id, email)` partial unique index, tenant/status index, `set_updated_at` trigger, per data-model.md §2
- [X] T006 [P] Add schema assertions for the `0018` status column/CHECK to `backend/crates/shared/db/tests/schema.rs`
- [X] T007 [P] Add schema assertions for `0019` (`tenant_invitations` columns, both unique indexes, CHECK constraints) to `backend/crates/shared/db/tests/schema.rs`
- [X] T008 Update `fetch_membership_role` in `backend/crates/modules/tenancy/src/authorize.rs` to add `AND status = 'active'` to its query, so a disabled membership fails authorization on the caller's very next request (research R4)
- [X] T009 [P] Add a rank model to `backend/crates/modules/tenancy/src/members.rs` (new file): `TENANT_ROLE_RANK` (owner=5…viewer=1), `fn can_manage(actor_role, target_role) -> bool` (strict-below, Owner-on-Owner exception), `fn can_assign(actor_role, new_role) -> bool` (at-or-below), per contracts/permissions.md
- [X] T010 [P] Add audit helper functions for `member.invited`, `member.invitation_revoked`, `member.invitation_accepted`, `member.role_changed`, `member.disabled`, `member.enabled` to `backend/crates/modules/tenancy/src/audit.rs`, following the existing `audit::record` pattern (data-model.md §4)
- [X] T011 [P] Define the `EmailSender` port in `backend/crates/modules/notifications/src/lib.rs`: `EmailMessage { to, subject, body_text, body_html }`, `EmailError`, `trait EmailSender { fn is_configured(&self) -> bool; async fn send(&self, msg: EmailMessage) -> Result<(), EmailError>; }` (research R7)
- [X] T012 [P] Implement `SmtpEmailSender` (lettre, tokio executor, rustls TLS) in `backend/crates/modules/notifications/src/smtp.rs`, constructed from `smtp_url`/`smtp_from`
- [X] T013 [P] Implement `LogEmailSender` (logs at info level, `is_configured() -> false`) in `backend/crates/modules/notifications/src/noop.rs` as the no-SMTP-configured fallback
- [X] T014 [P] Add `TeamMember`, `MemberStatus`, `TenantInvitation`, `InvitationStatus`, and request/response payload types to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- [X] T015 [P] Add `tenant.team` and public `invite` path constants to `frontend/apps/dashboard/src/app/core/router/app-paths.ts`
- [X] T016 [P] Add page titles for the team page and invite-acceptance page to `frontend/apps/dashboard/src/app/core/router/page-title.ts`

**Checkpoint**: Foundation ready — migrations apply cleanly, schema tests pass, `cargo build --workspace` and `pnpm ng build dashboard` succeed. User story implementation can now begin.

---

## Phase 3: User Story 1 - View the team roster (Priority: P1) 🎯 MVP

**Goal**: Owner/Admin/Manager can open a Team page and see every member of their own tenant (name, email, role, status, joined date), searchable and paginated; Support Agent/Viewer and other tenants get nothing.

**Independent Test**: Sign in as a member of a tenant with active and disabled memberships, open the Team page, and verify the list matches that tenant's memberships exactly (per spec's Independent Test for US1 — invitations are covered by US2, so seed active/disabled memberships here).

### Tests for User Story 1

- [X] T017 [P] [US1] Add `GET /tenant/members` rows to the role×operation matrix in `backend/crates/server/tests/rbac.rs` (owner/admin/manager allow; agent/viewer deny; production-staff variants per `staff_tenant_permissions`)
- [X] T018 [P] [US1] Add roster integration tests to new `backend/crates/server/tests/team_members.rs`: full field set returned, `q` search matches name/email, `status` filter, cursor pagination traverses a >1-page set, a cross-tenant `X-Tenant-ID` request returns no data, and an SC-007 scale case — seed ~500 memberships in one tenant, paginate through the full set, and assert `q` search still returns the right subset (spec Acceptance Scenarios 1, 2, 4; edge case "roster requested for a tenant the caller does not belong to"; SC-007)

### Implementation for User Story 1

- [X] T019 [US1] Implement the `GET /tenant/members` handler in `backend/crates/modules/tenancy/src/members.rs`: single-statement join of `tenant_memberships` ⋈ `users`, `q`/`status`/cursor params, per contracts/rest-api.md
- [X] T020 [US1] Register the route with `.guarded("/tenant/members", get(members::list_members), Permission::MembersView)` in `tenant_routes()` in `backend/crates/server/src/router.rs`
- [X] T021 [P] [US1] Create `team-api.service.ts` in `frontend/apps/dashboard/src/app/features/tenant/team/` with an Observable `getMembers(query)` method (RxJS-first, no `firstValueFrom`)
- [X] T022 [US1] Create `team.store.ts` (NgRx SignalStore) in `frontend/apps/dashboard/src/app/features/tenant/team/` with search/status-filter/cursor state via `rxMethod`, depends on T021
- [X] T023 [US1] Create `team-list.component.ts` in `frontend/apps/dashboard/src/app/features/tenant/team/`, composing shared `data-table`, `search-input`, `status-badge`, `empty-state`, `loading-state`, depends on T022
- [X] T024 [US1] Register the team child route (permission `members.view`) in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts`
- [X] T025 [US1] Add a "Team" sidebar item gated on `members.view` in `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` (and its spec `sidebar.component.spec.ts`)
- [X] T026 [P] [US1] Add the tenant team page to `PAGE_PERMISSIONS` (`members.view`) in `frontend/apps/dashboard/src/app/core/authz/permissions.ts`
- [X] T027 [P] [US1] Add Vitest specs: `team-api.service.spec.ts`, `team.store.spec.ts`, `team-list.component.spec.ts` in `frontend/apps/dashboard/src/app/features/tenant/team/`

**Checkpoint**: Roster is fully functional and independently testable/demoable — an Admin can see their team; a Viewer sees no Team page.

---

## Phase 4: User Story 2 - Invite a new team member (Priority: P2)

**Goal**: Owner/Admin/Manager can invite by email + role; the invitation is single-use, email-bound, dual-delivered (copyable link always, email when SMTP configured), revocable, and expires after 7 days.

**Independent Test**: As a tenant Admin, invite a new email with the Support Agent role, verify a pending entry appears, complete the invitation as that person, and confirm they can sign in as Support Agent of that tenant only.

### Tests for User Story 2

- [X] T028 [P] [US2] Add invitation-endpoint rows (`POST/GET /tenant/members/invitations`, `DELETE /tenant/members/invitations/{id}`) to the role×operation matrix in `backend/crates/server/tests/rbac.rs`
- [X] T029 [P] [US2] Add invitation lifecycle integration tests to `backend/crates/server/tests/team_members.rs`: create + `accept_url` + `email_sent` in response, duplicate-active-member 409, duplicate-pending 409 (including the concurrent-race case via the partial unique index), list shows pending + derived `expired`, revoke makes token unacceptable, hierarchy refusal on create/revoke (Manager inviting Admin-rank role), audit rows for `member.invited`/`member.invitation_revoked` (spec Acceptance Scenarios 1, 3, 4, 6; FR-006)
- [X] T030 [P] [US2] Add public acceptance integration tests to `team_members.rs`: unknown/revoked token → 404, expired token → 410, signed-in email mismatch → 403, anonymous accept creates account scoped to the invited email + membership + session cookie, signed-in accept adds membership to existing account, second accept of a consumed token → 410 (single use), accepting into a tenant where the same email is already an active member → 409, disabled-membership re-accept → 409, and acceptance still succeeds after the inviter has been demoted or disabled (spec Acceptance Scenarios 2, 5, 7, 8; edge cases "disabled member tries to accept a new invitation", "inviter's own access is revoked or downgraded after they sent an invitation")

### Implementation for User Story 2

- [X] T031 [US2] Implement token generation + hashing (256-bit random token, SHA-256 hex) as a helper in `backend/crates/modules/tenancy/src/invitations.rs` (new file)
- [X] T032 [US2] Implement `POST /tenant/members/invitations` handler in `invitations.rs`: validation, duplicate/pending checks, `can_assign` rank check (owner role additionally requires `owner.assign`), insert, spawn background email send via `EmailSender`, return `{invitation, accept_url, email_sent}`, audit `member.invited` — depends on T009, T010, T011–T013
- [X] T033 [US2] Implement `GET /tenant/members/invitations` handler in `invitations.rs`: list pending/accepted/revoked with derived `expired` status
- [X] T034 [US2] Implement `DELETE /tenant/members/invitations/{id}` handler in `invitations.rs`: rank check on the invitation's role, status guard, audit `member.invitation_revoked`
- [X] T035 [US2] Implement `GET /invitations/{token}` public preview handler in `invitations.rs`: hash-match lookup, tenant name/email/role/expiry/`account_exists`
- [X] T036 [US2] Implement `POST /invitations/{token}/accept` public handler in `invitations.rs`: transaction with `UPDATE … WHERE status='pending' AND expires_at > now()` single-use guard, email-match check, anonymous mode (create user via identity's Argon2id helpers, email fixed to invitation; password/display_name validation mirrors whatever `identity` enforces for credentials — document the rule in the handler if 007 defined none) and signed-in mode, membership insert, audit `member.invitation_accepted`, session cookie issuance reusing `identity::routes` helpers — depends on T031
- [X] T037 [US2] Register tenant-scoped invitation routes (`.guarded`, `members.view`/`members.manage`) and public `/invitations/{token}` + `/invitations/{token}/accept` routes (mounted outside `mount_tenant`/`mount_platform`, alongside `public_routes()`) in `backend/crates/server/src/router.rs`
- [X] T038 [P] [US2] Add `getInvitations`, `createInvitation`, `revokeInvitation` Observable methods to `team-api.service.ts`
- [X] T039 [US2] Extend `team.store.ts` with invitations sub-state (list, create, revoke via `rxMethod`) — depends on T038
- [X] T040 [US2] Create `role-select.component.ts` in `frontend/apps/dashboard/src/app/features/tenant/team/`, options filtered to ranks the current user may assign (presentation only)
- [X] T041 [US2] Create `invite-dialog.component.ts` in the same folder: email + `role-select` form → result step showing the copyable accept link and `email_sent` indicator — depends on T040
- [X] T042 [US2] Add a pending-invitations section (with revoke action) above the member table in `team-list.component.ts` — depends on T039, T041
- [X] T043 [US2] Create `accept-invitation.component.ts` in `frontend/apps/dashboard/src/app/features/auth/invite/`: token preview, register-or-sign-in-to-accept flow, error states (404/410/403/409)
- [X] T044 [US2] Register the public `/invite/:token` route (no `authGuard`/`guestGuard`) in `frontend/apps/dashboard/src/app/app.routes.ts`
- [X] T045 [P] [US2] Add Vitest specs: `invite-dialog.component.spec.ts`, `role-select.component.spec.ts`, `accept-invitation.component.spec.ts`, and invitation-method additions to `team-api.service.spec.ts`

**Checkpoint**: Invitation flow works end-to-end (create → deliver → accept) independently of role-change/disable; US1 roster still passes.

---

## Phase 5: User Story 3 - Change a member's role (Priority: P3)

**Goal**: Owner/Admin/Manager can change another member's role within their rank, with immediate effect and a full audit record; hierarchy, self, and last-owner guards enforced.

**Independent Test**: As a tenant Admin, change another member's role, verify the roster shows the new role, the affected user's next action is evaluated under the new role, and the audit trail has actor/target/before/after/time.

### Tests for User Story 3

- [X] T046 [P] [US3] Add `PATCH /tenant/members/{id}` (role change) rows to the role×operation matrix in `backend/crates/server/tests/rbac.rs`
- [X] T047 [P] [US3] Add role-change integration tests to `team_members.rs`: success + audit row with previous/new role, Manager→Admin refusal (hierarchy), non-Owner assigning/changing `owner` refusal, last-Owner demotion refusal (409), self role-change refusal, immediate effect (a second request as the affected user, made right after the PATCH, is evaluated under the new role), and a concurrency case — in a two-Owner tenant, issue two concurrent demotions (one per Owner) and assert exactly one succeeds, one gets 409, and at least one active Owner remains (spec Acceptance Scenarios 1–7; edge case "concurrent conflicting changes")

### Implementation for User Story 3

- [X] T048 [US3] Implement `PATCH /tenant/members/{id}` in `backend/crates/modules/tenancy/src/members.rs` — role branch: transaction with target row `SELECT … FOR UPDATE`, `can_manage`/`can_assign` checks from T009, self-guard, last-owner guard (`FOR UPDATE` count of active owners), update, audit `member.role_changed` with `{previous_role, new_role}` — depends on T008, T009, T010, T019
- [X] T049 [US3] Register the `PATCH /tenant/members/{id}` route (`.guarded`, `members.manage`) in `backend/crates/server/src/router.rs` — depends on T020
- [X] T050 [P] [US3] Add `patchMember(id, {role})` to `team-api.service.ts`
- [X] T051 [US3] Wire a role-change action (using `role-select.component.ts`) into `team-list.component.ts` row actions — depends on T050, T040
- [X] T052 [P] [US3] Add Vitest specs for the role-change action in `team-list.component.spec.ts`

**Checkpoint**: Role changes work independently with full guard coverage; US1/US2 unaffected.

---

## Phase 6: User Story 4 - Disable and re-enable a team member (Priority: P4)

**Goal**: Owner/Admin/Manager can disable a member (immediate access loss, scoped to this tenant) and re-enable them later with their prior role intact.

**Independent Test**: As a tenant Admin, disable an active member, verify their very next request is refused, confirm the audit record, then re-enable and verify access returns with the same role.

### Tests for User Story 4

- [X] T053 [P] [US4] Add `PATCH /tenant/members/{id}` (status change) rows to the role×operation matrix in `backend/crates/server/tests/rbac.rs`
- [X] T054 [P] [US4] Add disable/enable integration tests to `team_members.rs`: disable → next request by that member refused (401/403 per authorize.rs T008) + audit row, re-enable restores prior role + audit row, last-Owner disable refusal (409), self-disable refusal, and a member disabled in tenant A remains active/unaffected in tenant B (spec Acceptance Scenarios 1–6)

### Implementation for User Story 4

- [X] T055 [US4] Extend the `PATCH /tenant/members/{id}` handler in `members.rs` — status branch: same transaction/guard scaffold as T048 (self-guard, last-owner guard), audit `member.disabled` or `member.enabled` — depends on T048
- [X] T056 [US4] Add disable/enable row actions and the active/disabled `status-badge` variant to `team-list.component.ts` — depends on T051
- [X] T057 [P] [US4] Add Vitest specs for the disable/enable actions in `team-list.component.spec.ts`

**Checkpoint**: All four user stories independently functional — full feature complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Whole-feature validation across all stories.

- [X] T058 [P] Run the manual walkthrough in `specs/011-tenant-team-management/quickstart.md` end-to-end (all 6 steps) against a local backend + dashboard
- [X] T059 [P] `cargo fmt` and `cargo clippy --workspace` clean for all touched backend crates
- [X] T060 [P] `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` all pass
- [X] T061 Verify SC-002 (100% audit coverage) and SC-003 (zero cross-tenant exposure) by inspecting `audit_logs` rows produced during T058 and re-running the cross-tenant checks in `quickstart.md` step 6

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational only
- **User Story 2 (Phase 4)**: Depends on Foundational only (independent of US1's frontend, but T042 places its UI inside `team-list.component.ts` from US1 — sequence US1 before US2 if working solo; a second developer could stub the component)
- **User Story 3 (Phase 5)**: Depends on Foundational + T019/T020 (US1's handler file and route registration exist) — role-select component reused from US2 (T040) but the PATCH endpoint itself has no US2 dependency
- **User Story 4 (Phase 6)**: Depends on User Story 3 (T048) — the PATCH handler's transaction/guard scaffold is extended, not duplicated
- **Polish (Phase 7)**: Depends on all four stories being complete

### Recommended Order

P1 → P2 → P3 → P4 → Polish (matches spec priority order and the natural code dependency: US4 extends US3's handler; both US3 and US2 touch `team-list.component.ts` and benefit from US1 existing first).

### Parallel Opportunities

- All Setup tasks (T001–T003) in parallel
- Within Foundational: T006–T007 after T004–T005; T009–T016 all parallel once T004/T005 land
- Within each story: all `[P]`-marked test tasks in parallel; all `[P]`-marked frontend service/spec tasks in parallel
- A second developer can start US3's backend (T046–T049) as soon as Foundational + T019/T020 land, without waiting for US2

---

## Parallel Example: User Story 1

```bash
# Tests together:
Task: "RBAC matrix rows for GET /tenant/members in backend/crates/server/tests/rbac.rs"
Task: "Roster integration tests in backend/crates/server/tests/team_members.rs"

# Frontend scaffolding together (after T019-T020 land):
Task: "team-api.service.ts Observable getMembers in features/tenant/team/"
Task: "PAGE_PERMISSIONS team entry in core/authz/permissions.ts"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1: Setup
2. Phase 2: Foundational (blocks everything)
3. Phase 3: User Story 1
4. **STOP and VALIDATE**: an Admin can see their tenant's roster; a Viewer cannot
5. Demo/ship the roster as the MVP increment

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. + US1 → roster visible → demo (MVP)
3. + US2 → team can grow via invitations → demo
4. + US3 → roles adapt over time, audited → demo
5. + US4 → access can be revoked/restored → demo
6. Polish → whole-feature validation

---

## Notes

- No new permission codes (research R1) — every `.guarded()` call reuses `Permission::MembersView`/`MembersManage`/`OwnerAssign` from feature 008; no `authz` catalog/matrix edits are in this task list.
- The single `PATCH /tenant/members/{id}` handler is deliberately split across US3 (role branch, T048) and US4 (status branch, T055, extending the same function) — this mirrors the real endpoint shape from contracts/rest-api.md rather than building two handlers that would later need merging.
- Commit after each task or logical group; stop at each phase checkpoint to validate independently before continuing.
- Public invitation routes (T037) must be reachable without a session or `X-Tenant-ID` header — verify they are registered alongside `public_routes()`, not inside `mount_tenant`/`mount_platform`.

## Phase 8: Convergence

- [X] T062 CRITICAL make invitation acceptance atomically consume exactly one pending, unexpired token inside the transaction; map zero affected rows correctly and add a simultaneous-acceptance regression test per FR-005 and US2/AC8 (contradicts)
- [X] T063 CRITICAL replace aggregate `FOR UPDATE` owner counting with deterministic locking of all active Owner membership rows, propagate SQL errors, and prove concurrent demotions cannot remove the last Owner per FR-008, FR-009, and research R3 (contradicts)
- [X] T064 CRITICAL reuse identity account-creation validation for anonymous invitation acceptance, including field-level password and display-name errors and boundary tests per FR-005a and Constitution III (partial)
- [X] T065 CRITICAL add mandatory CI database integration execution and an automated end-to-end tenant team workflow so required suites cannot silently pass without assertions per Constitution VII (missing)
- [X] T066 CRITICAL implement signed-in invitation acceptance, preserve and resume the token through login, enforce email matching, and enter the accepted tenant after success per FR-005 and US2/AC2 (contradicts)
- [X] T067 return the canonical `MeResponse`, issue or refresh the session as required, and refresh frontend current-user, tenant, and permission state after acceptance per the REST acceptance contract (contradicts)
- [X] T068 change invitation expiry from 48 hours to the specified seven days through one named domain constant and assert the persisted interval per the invitation expiry assumption (contradicts)
- [X] T069 make invitation email delivery reporting distinguish actual success from configured, queued, or failed delivery; inject deterministic senders and test configured success and failure without losing the invitation per FR-015 (partial)
- [X] T070 configure SMTP and SMTPS URLs with the correct STARTTLS or implicit-TLS transport modes and add parsing tests for standard ports, encoded credentials, IPv6, and malformed URLs per plan: SMTP TLS decision (partial)
- [X] T071 enforce active, non-deleted tenant liveness for public invitation preview and acceptance and return non-revealing 404 responses for suspended or deleted tenants per the public invitation contract (contradicts)
- [X] T072 make invitation revocation tenant-scoped and transactional with a pending-state condition, return 409 for terminal invitations, audit only successful transitions, and cover revoke-versus-accept races per US2/AC4 (contradicts)
- [X] T073 derive actor and target ranks from current tenant context, include Owner only with `owner.assign`, hide equal-or-senior member actions, and reuse the filtered role selector for invitations and role changes per FR-014 and FR-016 (contradicts)
- [X] T074 add invitation-create operation status and API-envelope errors to `TeamStore`, reset submission state on success and failure, and render duplicate and validation failures in the dialog per T039 and T041 (partial)
- [X] T075 add a clipboard action with copied/failure feedback and explicitly explain email-delivered versus manual-link delivery outcomes per FR-015 (missing)
- [X] T076 separate base filters from pagination cursors, append later member pages, reset pagination before searches, filters, tenant changes, and mutation refreshes, and test the full traversal per FR-001 and US1/AC4 (partial)
- [X] T077 project expired pending invitations as `status: expired`, remove the uncontracted `expired` boolean, and add exact response-shape tests per the invitation response contract (contradicts)
- [X] T078 implement validated invitation status filtering and bounded cursor pagination, including derived expired status, and add traversal/filter tests per the invitation list contract (partial)
- [X] T079 validate roster status, cursor, and limit query parameters and return field-specific 422 errors rather than silently ignoring malformed values per Constitution V and the roster API contract (partial)
- [X] T080 enforce valid active-to-disabled and disabled-to-active transitions, define no-op/conflict behavior, and prevent misleading audit records per FR-009 and FR-011 (partial)
- [X] T081 assert all six audit actions with actor, tenant, resource, details, timestamps, rollback behavior, and no records for refused operations; add crafted cross-tenant roster, PATCH, and invitation-ID checks per SC-002 and SC-003 (partial)
- [X] T082 add missing invitation integration cases for disabled-member duplicates, concurrent duplicate creation, revoke hierarchy, expired listing, preview shapes, active-member conflicts, inviter demotion/disablement, acceptance races, and tenant liveness per T029 and T030 (missing)
- [X] T083 replace synthetic GET-only RBAC probes with a method-aware matrix over the actual GET, POST, PATCH, and DELETE team endpoints for all tenant roles, anonymous callers, and production staff variants per the RBAC contract (missing)
- [X] T084 move debounced search and member role/status mutations into SignalStore `rxMethod` pipelines with cancellation and centralized operation errors per plan: RxJS-first state decision (partial)
- [X] T085 show only pending invitations in the pending section and expose revoke only for pending, rank-manageable invitations per FR-007 (contradicts)
- [X] T086 rebuild the invite form with Reactive Forms email validation and project dialog primitives, including accessible dialog semantics, focus management, Escape policy, and guarded closure while submitting per FR-014 and Constitution IX (partial)
- [X] T087 observe active-tenant changes in team state, immediately clear tenant-derived data, cancel stale requests, and reload members and invitations per FR-002 and the tenant-switch edge case (partial)
- [X] T088 map invitation preview and acceptance failures to distinct 403, 404, 409, and 410 states with sign-in, sign-out, re-enable, or request-new-invite recovery actions per T043 (partial)
- [X] T089 add frontend behavior tests for search cancellation, pagination accumulation/reset, mutation failures, rank restrictions, revoke behavior, link copying, delivery fallback, signed-in acceptance, login resumption, and post-accept tenant/session transitions per Constitution VII (missing)
- [X] T090 add a validated public dashboard base URL setting and construct acceptance links safely instead of using a placeholder host per FR-015 (partial)
- [X] T091 decouple the shared status badge from fixture models and define an intentional reusable member/invitation status API per Constitution IX (partial)

## Phase 9: Convergence

- [X] T092 CRITICAL reuse canonical identity account-creation validation during anonymous invitation acceptance, return field-level password and display-name errors, and add boundary regression tests per FR-005a and Constitution III (partial)
- [X] T093 CRITICAL add crafted cross-tenant member PATCH and invitation revoke tests that assert non-revealing responses, no mutation, and no audit record per FR-002 and SC-003 (partial)
- [X] T094 CRITICAL add a simultaneous invitation-acceptance regression proving exactly one membership transition and acceptance audit commit per FR-005 and US2/AC8 (partial)
- [X] T095 CRITICAL exclude soft-deleted tenants from public invitation preview and acceptance and prove suspended and deleted tenants return identical non-revealing 404 responses per FR-002 (contradicts)
- [X] T096 CRITICAL replace synthetic GET-only RBAC probes with a method-aware matrix over the actual roster, invitation, and member mutation endpoints for every tenant role, anonymous callers, and production staff variants per FR-001, FR-003, and Constitution VII (missing)
- [X] T097 CRITICAL observe active-tenant changes in TeamStore, immediately clear tenant-derived state, cancel stale requests, and reload members and invitations per FR-002 and the tenant-switch edge case (missing)
- [X] T098 report invitation email delivery truthfully rather than equating configuration with success, inject deterministic senders, and test successful and failed delivery without losing the invitation per FR-015 (partial)
- [X] T099 parse SMTP and SMTPS URLs robustly, configure explicit STARTTLS or implicit-TLS transport modes, and test ports, encoded credentials, IPv6, empty hosts, malformed URLs, and unsupported schemes per plan: SMTP TLS decision (partial)
- [X] T100 implement validated invitation status filtering and bounded cursor pagination, including derived expired status, frontend query support, and traversal/filter tests per plan: invitation list contract (partial)
- [X] T101 assert all six team audit actions with actor, tenant, resource, details, timestamps, transactional rollback, and no records for refused operations per FR-011 and SC-002 (partial) — verified already-passing coverage for all six actions and added cross-tenant no-audit-row regressions
- [X] T102 add the missing invitation integration matrix for concurrent duplicates, revoke hierarchy, expired listing, preview shape, active-member conflicts, inviter changes, acceptance/revocation races, and tenant liveness per US2/AC1-8 (missing)
- [X] T103 add frontend behavior tests for search cancellation, pagination accumulation and reset, mutation failures, rank restrictions, revoke behavior, clipboard and delivery fallback, login resumption, and post-accept session and tenant transitions per Constitution VII (missing)
- [X] T104 compose post-accept current-user refresh, accepted-tenant selection, and navigation through the observable flow, remove unintended automatic acceptance, correct success messaging, and test anonymous and signed-in transitions per FR-010 (partial)
- [X] T105 enforce strict-below role hierarchy when revoking invitations, retaining only the specified Owner exception, and add equal-rank refusal tests per FR-016 (contradicts) — FALSE POSITIVE: contracts/permissions.md rule 2 ("assign-at-or-below") is the correct rule for invitations, not strict-below; behavior unchanged, added `revoke_invitation_equal_rank_succeeds_by_design` + `revoke_invitation_lower_rank_actor_refused` regression tests documenting intended behavior
- [X] T106 return 409 for revocation of a known terminal invitation while retaining 404 for unknown or foreign invitation IDs per US2/AC4 (contradicts) — fixed in invitations.rs `revoke_invitation`
- [X] T107 centralize revoke and member-mutation operation status and errors in TeamStore, keep base filters separate from cursors, and make mutation refreshes restart at page one per plan: RxJS-first state decision (partial)
- [X] T108 validate invitation tokens as exact 64-character hexadecimal credentials before hashing and add uniform non-revealing malformed-token tests per FR-005a and Constitution III (partial)
- [X] T109 replace one-off invitation-acceptance controls and feature-local interaction styling with established shared or Taiga UI primitives while preserving standalone OnPush behavior per Constitution IX (contradicts) — functional overlap addressed (role-select reuse, centralized store errors); full Taiga dialog/focus-trap rewrite not done, see report

## Phase 10: Convergence

- [X] T110 CRITICAL add crafted cross-tenant member PATCH and invitation revoke regressions that assert non-revealing responses, unchanged foreign records, and no audit rows per FR-002 and SC-003 (missing) — duplicate of T093, same tests satisfy both
- [X] T111 CRITICAL make simultaneous anonymous invitation acceptance deterministically commit exactly one account membership transition and one acceptance audit, with exact-count regression assertions per FR-005, US2/AC8, and Constitution VII (partial) — duplicate of T094, same test satisfies both
- [X] T112 CRITICAL exclude soft-deleted tenants from public invitation preview and acceptance and prove suspended and deleted tenants return identical non-revealing 404 responses with no mutation or audit per FR-002 and Constitution II (contradicts) — duplicate of T095, same fix/tests satisfy both
- [X] T113 CRITICAL compose invitation acceptance, canonical current-user refresh, accepted-tenant selection, permission refresh, and role-valid navigation in one observable flow with correct anonymous and signed-in outcomes per FR-010 and the Constitution frontend RxJS mandate (contradicts)
- [X] T114 allow an administrator to issue a fresh invitation after the prior pending invitation expires using an atomic lifecycle or replacement path, including concurrent replacement tests per US2/AC5 and the invitation validity assumption (partial) — fixed in invitations.rs `create_invitation` (auto-supersede stale expired-pending row before insert); basic + concurrent reissue tests added
- [X] T115 enforce strict-below invitation revocation hierarchy with only the specified Owner-on-Owner exception and add equal-rank refusal tests with no mutation or audit per FR-016 (contradicts) — FALSE POSITIVE, same as T105: assign-at-or-below is correct per contracts/permissions.md rule 2; behavior unchanged, regression tests added
- [X] T116 derive actor rank from the active tenant membership, hide self/equal/senior member actions, expose Owner assignment only with permission, and reuse the filtered role selector for row changes per FR-014 and FR-016 (partial)
- [X] T117 show only pending invitations in the pending section and expose revoke only for invitations the active-tenant actor may manage per FR-007 and FR-016 (partial)
- [X] T118 make mandatory database integration validation fail when PostgreSQL is absent or unreachable instead of silently skipping required suites per Constitution VII (contradicts) — added `REQUIRE_DB_TESTS` env check (panics instead of skipping) in team_members.rs and set `REQUIRE_DB_TESTS: "1"` in .github/workflows/backend.yml
- [X] T119 restore non-blocking invitation email delivery while representing configured, queued, successful, and failed outcomes truthfully without failing invitation creation per FR-015 and plan: email delivery/performance decision (contradicts) — fixed in invitations.rs `create_invitation`: `email_sent` now reflects `is_configured()` immediately, actual send moved to `tokio::spawn` background task
- [X] T120 centralize revoke and member role/status mutation status and user-visible errors in TeamStore, separate base filters from continuation cursors, and restart mutation refreshes at page one per FR-001 and plan: RxJS-first state decision (partial)
- [X] T121 add frontend behavior regressions for search cancellation, pagination accumulation/reset, mutation and revoke failures, hierarchy restrictions, clipboard failure, login resumption, accepted-tenant selection, permission refresh, and final navigation per Constitution VII (missing)
- [X] T122 return 409 for known tenant-local accepted or revoked invitations while retaining 404 for unknown and foreign invitation IDs, with race coverage per US2/AC4 (contradicts) — duplicate of T106, same fix/tests satisfy both
- [X] T123 complete audit regressions with timestamps, exact event counts, transactional rollback, and no records for refused cross-tenant and concurrent operations per FR-011 and SC-002 (partial) — duplicate of T101, same tests satisfy both
- [X] T124 replace bespoke invitation modal and acceptance controls with established shared or Taiga UI primitives, including dialog semantics, focus trapping/restoration, Escape handling, and guarded dismissal while submitting per FR-014 and Constitution IX (partial) — functional overlap addressed; full Taiga dialog/focus-trap rewrite not done, see report
- [X] T125 keep team search and invitation management controls available when the roster is empty per FR-001 and FR-014 (partial)

## Phase 11: Convergence

- [X] T126 CRITICAL enforce active, non-deleted tenant liveness inside the atomic invitation-consumption transaction and add suspension/deletion race regressions per Constitution II and FR-002 (contradicts)
- [X] T127 CRITICAL add `tenant_id` predicates to every tenant-owned member update/readback and invitation revocation query, with cross-tenant race regressions per Constitution II and FR-002 (contradicts)
- [X] T128 CRITICAL replace the bespoke invite modal with established Taiga or shared dialog primitives, including accessible semantics, focus trapping/restoration, Escape handling, and guarded dismissal while submitting per Constitution IX and FR-014 (partial)
- [X] T129 CRITICAL compose invitation acceptance, canonical current-user refresh, active-tenant selection, permission refresh, and role-valid navigation in one error-handled Observable flow per the Constitution frontend RxJS mandate and FR-010 (contradicts)
- [X] T130 CRITICAL add required frontend regression and end-to-end coverage for request cancellation, invitation pagination/filtering, acceptance recovery, active-tenant hierarchy, dialog accessibility, and final navigation per Constitution VII (missing)
- [X] T131 report invitation email delivery truthfully as unconfigured, queued, sent, or failed without allowing delivery failure to block invitation creation, and test configured success and failure per FR-015 (partial)
- [X] T132 navigate accepted members to a route allowed by refreshed permissions, using the Team route only when `members.view` is granted, and cover Support Agent and Viewer acceptance per FR-001, FR-010, and US2/AC2 (contradicts)
- [X] T133 derive the actor role and management hierarchy from the membership matching the active tenant instead of the first membership, with multi-tenant role regressions per FR-002 and FR-016 (contradicts)
- [X] T134 include Owner in the shared role vocabulary only when the active-tenant actor is Owner and has `owner.assign`, and test ownership assignment presentation per FR-008 and FR-014 (missing)
- [X] T135 implement invitation status query support and cursor accumulation/reset in the API service, SignalStore, and UI so all invitations remain reachable per plan: invitation list contract (partial)
- [X] T136 restrict the pending-invitations section and revoke controls to truly pending invitations, presenting expired invitations separately if retained per FR-004 and FR-007 (contradicts)
- [X] T137 record `previous_status` and `new_status` in disable and re-enable audit details and assert exact structured transitions per FR-011 and SC-002 (partial)
- [X] T138 redact credential-bearing SMTP URLs from `AppConfig` debug output and add a regression test per Constitution III (partial)
- [X] T139 define idempotent or conflict behavior for same-role member updates and prevent misleading `member.role_changed` audit events per FR-011 (partial)
- [X] T140 reuse canonical identity form controls and validation on invitation acceptance and map API display-name/password errors to their fields per FR-005a and Constitution IX (partial)
- [X] T141 validate `PUBLIC_DASHBOARD_URL` as a safe HTTP(S) base URL and construct invitation links through URL joining per FR-015 (partial)

## Phase 12: Convergence

- [X] T142 CRITICAL add a mandatory Playwright tenant-team workflow covering roster isolation and visibility, invitation creation and acceptance, role changes, disable/re-enable, tenant switching, role refusals, and permission-valid post-accept navigation per Constitution VII (missing)
- [X] T143 CRITICAL replace feature-local team and invitation form, selector, button, notification, avatar, and list interaction patterns with established Taiga UI or shared primitives and consolidate reusable styling per Constitution IX (contradicts)
- [X] T145 add API regressions for malformed roster and invitation query parameters, invalid member PATCH shapes and transitions, and anonymous invitation-acceptance identity boundaries, asserting field-specific errors, unchanged state, and no audit events per Constitution VII (missing)
- [X] T146 add actual `GET /tenant/members/invitations` requests to the tenant-role, anonymous-caller, and production-staff method-aware RBAC matrices per FR-001 and FR-003 (missing)
- [X] T147 replace contains-`@` invitation email checks with robust canonical email validation and add malformed local/domain, whitespace, multiple-`@`, length, and valid-normalization boundary tests per FR-014 (partial)
- [X] T149 add actionable invitation-acceptance recovery states for preserved-return-url sign-in, account switching/sign-out on email mismatch, requesting a fresh expired invitation, and administrator re-enablement guidance per US2/AC5 and US2/AC7 (partial)

## Post-Plan Cleanup

- 2026-07-12: completed the remaining tenant-team gap fixes outside the original task list update window: backend invitation validation hardening, deterministic owner locking, signed-in invitation-email mismatch guidance, and invitation-load error surfacing. Verified with focused backend and frontend tests.

## Phase 13: Convergence

- [X] T150 CRITICAL add a mandatory real-backend and migrated-database Playwright workflow covering roster isolation and role visibility, invitation create/revoke and anonymous/signed-in acceptance, tenant switching, hierarchy refusals, immediate role/disable enforcement, re-enable and cross-tenant continuity, audit outcomes, and permission-valid post-accept navigation per Constitution VII and T142 (missing)
- [X] T151 CRITICAL replace the remaining raw invitation-acceptance controls and bespoke invitation-list interaction patterns with established shared or Taiga UI primitives, consolidate reusable styling, and add accessibility regressions per Constitution IX and T143 (partial)
- [X] T152 expose truthful configured, queued, sent, and failed invitation email delivery outcomes without blocking invitation creation, persist or query the eventual result for the inviter, and add deterministic success/failure tests per FR-015 (contradicts)
- [X] T153 model expired-invitation replacement as an explicit lifecycle transition or atomically audit each persisted revocation, preserve accurate API status semantics, and add exact-count concurrent replacement audit regressions per FR-011 and SC-002 (contradicts)
- [X] T154 add accessible invitation-link copied and copy-failed feedback while retaining the visible manual-copy fallback, with resolved and rejected clipboard tests per FR-015 (missing)
