# Tenant Team Management Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining spec gaps in tenant team management by tightening backend validation and ownership locking, and by surfacing invitation acceptance and invitation-load edge cases in the dashboard.

**Architecture:** Keep the current feature structure intact. On the backend, reuse the project email validation helper and keep invitation delivery reporting aligned with configuration state. Preserve the existing team store/component split on the frontend, but add the minimum UI branching needed to show invitation-load failures and signed-in invite-email mismatches.

**Tech Stack:** Rust 2024, Axum, SQLx, Angular 22, NgRx SignalStore, RxJS, Vitest.

## Global Constraints

- Reuse existing route contracts and response envelopes from `specs/011-tenant-team-management/contracts/rest-api.md`.
- Keep team management tenant-scoped and deny-by-default.
- Do not introduce new permission codes.
- Maintain the current Angular standalone / OnPush / RxJS-first patterns.
- Preserve the current `ApiError` vocabulary, including `gone`.

---

### Task 1: Harden backend invitation creation

**Files:**
- Modify: `backend/crates/modules/tenancy/src/invitations.rs`
- Modify: `backend/crates/server/tests/team_members.rs`

**Interfaces:**
- Consumes: `routes::is_valid_email`, `notifications::EmailSender::is_configured`, existing create-invitation response shape.
- Produces: 422 for malformed invite emails, `email_sent` that reflects configuration state, and stable contract coverage for the invitation response.

- [ ] **Step 1: Write a failing regression test for malformed invite email**

```rust
#[tokio::test]
async fn create_invitation_rejects_invalid_email() {
    // call POST /tenant/members/invitations with `not-an-email`
    // assert 422 validation_failed and field details for `email`
}
```

- [ ] **Step 2: Verify the test fails against current validation**

Run: `cargo test -p server --test team_members create_invitation_rejects_invalid_email -v`

- [ ] **Step 3: Implement the validation and delivery-state fix**

```rust
if !routes::is_valid_email(&email) {
    details.push(ErrorDetail {
        field: "email".into(),
        code: "invalid_format".into(),
        message: "Invalid email format".into(),
    });
}

let email_sent = sender.is_configured();
let email_delivery_status = if sender.is_configured() {
    EmailDeliveryStatus::Queued
} else {
    EmailDeliveryStatus::Unconfigured
};
```

- [ ] **Step 4: Verify the invitation tests pass**

Run: `cargo test -p server --test team_members create_invitation_success create_invitation_rejects_invalid_email -v`

### Task 2: Make last-owner locking deterministic

**Files:**
- Modify: `backend/crates/modules/tenancy/src/members.rs`
- Modify: `backend/crates/server/tests/team_members.rs`

**Interfaces:**
- Consumes: the existing `update_member` transaction and audit helpers.
- Produces: a deterministic owner-row locking order that preserves the last-owner guard under concurrent demotion/disable.

- [ ] **Step 1: Write a failing concurrency regression test for owner demotion/disable**

```rust
#[tokio::test]
async fn last_owner_changes_remain_conflicted_under_concurrency() {
    // two concurrent PATCH requests against the final two owners
    // assert exactly one success and one 409 conflict
}
```

- [ ] **Step 2: Verify the test exposes the current race risk**

Run: `cargo test -p server --test team_members last_owner_changes_remain_conflicted_under_concurrency -v`

- [ ] **Step 3: Lock owner rows in deterministic order before evaluating the guard**

```rust
let owner_rows = sqlx::query(
    "SELECT id FROM tenant_memberships \
     WHERE tenant_id = $1 AND role = 'owner' AND status = 'active' AND deleted_at IS NULL \
     ORDER BY id \
     FOR UPDATE",
)
```

- [ ] **Step 4: Verify the concurrency test passes**

Run: `cargo test -p server --test team_members last_owner_changes_remain_conflicted_under_concurrency -v`

### Task 3: Surface invite acceptance mismatch and invitation-load failures

**Files:**
- Modify: `frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

**Interfaces:**
- Consumes: invitation preview `email`, current signed-in user email, and `TeamStore.invitationsStatus()/invitationsError()`.
- Produces: explicit signed-in email mismatch guidance and a visible invitation-load error state.

- [ ] **Step 1: Add failing specs for the signed-in email mismatch and invitation-load error**

```ts
it('shows a mismatch error when signed in with a different email', async () => {});
it('renders invitation load errors instead of no invitations', async () => {});
```

- [ ] **Step 2: Verify the specs fail**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

- [ ] **Step 3: Implement the UI branching and error surface**

```ts
const previewEmail = this.preview()?.email?.toLowerCase();
const currentEmail = this.currentUserService.currentUser()?.email?.toLowerCase();
const emailMismatch = computed(() => this.isAuthenticated() && previewEmail && currentEmail !== previewEmail);
```

- [ ] **Step 4: Verify the specs pass**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

### Task 4: Final verification

**Files:**
- Modify: `specs/011-tenant-team-management/tasks.md` only if any task status needs correction after implementation.

**Interfaces:**
- Produces: a validated implementation with the remaining backend and frontend gaps closed.

- [ ] **Step 1: Run the focused backend and frontend tests**

Run: `cargo test -p server --test team_members && pnpm vitest frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

- [ ] **Step 2: Run the broader regression checks for touched areas**

Run: `cargo test -p server --test rbac && pnpm vitest frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts`
