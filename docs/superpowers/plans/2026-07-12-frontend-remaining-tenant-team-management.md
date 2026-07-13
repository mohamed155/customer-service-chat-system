# Frontend Remaining Tenant Team Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the remaining tenant team management frontend gaps: reusable invite dialog shell, invitation status filtering/pagination, and invitation acceptance page shell cleanup.

**Architecture:** Keep the team feature state in `TeamStore`, move modal chrome into a shared dialog shell, and keep the accept-invitation page aligned with the existing auth card pattern. Preserve the current RxJS-first data flow and only add the minimal state needed for status filters and cursor resets.

**Tech Stack:** Angular 22 standalone components, signals, NgRx SignalStore, RxJS, Taiga UI, Vitest.

## Global Constraints

- Preserve Angular standalone / OnPush / RxJS-first patterns.
- Keep route constants and permission gates unchanged except where the task explicitly requires a change.
- Do not modify backend files.
- Write the implementation report to `.superpowers/sdd/frontend-remaining-report.md`.

---

### Task 1: Shared dialog shell for invitation modal

**Files:**
- Create: `frontend/apps/dashboard/src/app/shared/components/dialog-shell/dialog-shell.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.spec.ts`

**Interfaces:**
- Consumes: projected title/content/actions, `open`, `ariaLabelledby`, `ariaDescribedby`, optional submit-guard flag, Escape/backdrop dismissal, focus restoration.
- Produces: a reusable dialog shell that restores focus to the trigger on close and blocks backdrop/Escape dismissal while submitting.

- [ ] **Step 1: Add a failing dialog-shell spec**

```ts
it('restores focus and blocks dismissal while submitting', async () => {
  // render shell with a trigger, open it, click backdrop, press Escape, assert no close while submitting
});
```

- [ ] **Step 2: Verify the spec fails**

Run: `pnpm vitest frontend/apps/dashboard/src/app/shared/components/dialog-shell/dialog-shell.component.spec.ts`

- [ ] **Step 3: Implement the shell and migrate invite-dialog to use it**

```ts
<app-dialog-shell [open]="true" [submitGuarded]="submitting()">...</app-dialog-shell>
```

- [ ] **Step 4: Verify the spec passes**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.spec.ts`

### Task 2: Invitation status filter and cursor handling

**Files:**
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team.store.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

**Interfaces:**
- Consumes: `InvitationQuery.status`, cursor-based pagination, invitation load/reload methods.
- Produces: `setInvitationStatusFilter(status)`, reset-on-filter-change behavior, appended pages when loading more, pending/expired sections that remain visible by default.

- [ ] **Step 1: Add failing store specs for filter reset and accumulation**

```ts
it('resets invitation cursor when the status filter changes', () => {});
it('appends later invitation pages when loading more', () => {});
```

- [ ] **Step 2: Verify the store specs fail**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts`

- [ ] **Step 3: Add the toolbar filter and default sections to the list UI**

```ts
<select (change)="onInvitationStatusChange($event)">
  <option value="all">All invitations</option>
  <option value="pending">Pending</option>
  <option value="expired">Expired</option>
  <option value="accepted">Accepted</option>
  <option value="revoked">Revoked</option>
</select>
```

- [ ] **Step 4: Verify the component specs pass**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

### Task 3: Invitation acceptance shell cleanup

**Files:**
- Modify: `frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts`

**Interfaces:**
- Consumes: `AuthCardComponent`, existing validation/error mapping, `CurrentUserService.load`, permission-based landing resolution.
- Produces: auth-card-based invitation accept page with the same acceptance flow and error handling.

- [ ] **Step 1: Add failing spec coverage for auth-card reuse**

```ts
it('renders inside the auth card shell', async () => {
  // assert app-auth-card is present and the form still behaves as before
});
```

- [ ] **Step 2: Implement the auth-card shell wrapping the invitation flows**

```ts
<app-auth-card title="Accept invitation" subtitle="Join ...">...</app-auth-card>
```

- [ ] **Step 3: Verify the invitation acceptance specs pass**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts`

### Task 4: Verification and report

**Files:**
- Add: `.superpowers/sdd/frontend-remaining-report.md`

**Interfaces:**
- Produces: concise implementation report with status, commits, tests, and concerns.

- [ ] **Step 1: Run the relevant frontend test files**

Run: `pnpm vitest frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts frontend/apps/dashboard/src/app/features/auth/invite/accept-invitation.component.spec.ts`

- [ ] **Step 2: Write the report file**

```md
status: DONE
commits: none
tests: ...
concerns: ...
```
