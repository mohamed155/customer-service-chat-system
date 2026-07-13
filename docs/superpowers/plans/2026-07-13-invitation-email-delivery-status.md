# Invitation Email Delivery Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist and expose truthful invitation email delivery outcomes without blocking invitation creation.

**Architecture:** Store `unconfigured`, `queued`, `sent`, or `failed` on each tenant invitation. Creation commits the invitation with its initial status and returns immediately; a detached sender updates that exact tenant/id row, while existing invitation-list refreshes expose the eventual result.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/PostgreSQL, Angular 22, NgRx SignalStore, Vitest.

## Global Constraints

- Use migration-only schema changes.
- Keep invitation creation non-blocking and preserve the copyable acceptance link.
- Keep `email_sent` truthful: true only for persisted `sent` state.
- Use deterministic sender tests and strict RED/GREEN cycles.
- Do not edit `specs/011-tenant-team-management/tasks.md` or commit.

---

### Task 1: Persisted Backend Delivery State

**Files:**
- Create: `backend/migrations/0020_invitation_email_delivery_status.sql`
- Modify: `backend/crates/shared/db/tests/schema.rs`
- Modify: `backend/crates/modules/tenancy/src/invitations.rs`
- Modify: `backend/crates/server/src/router.rs`
- Test: `backend/crates/server/tests/team_members.rs`

**Interfaces:**
- Produces: `EmailDeliveryStatus`, persisted `email_delivery_status`, and invitation create/list JSON projections.

- [ ] Write schema and deterministic endpoint tests that require the new column, immediate queued/unconfigured creation result, and eventual sent/failed list result.
- [ ] Run focused Cargo tests and record the expected missing-column/projection failures (RED).
- [ ] Add migration `0020`, project the column, and update the detached send task using `WHERE tenant_id = $1 AND id = $2`.
- [ ] Make `emailSent` derive only from `EmailDeliveryStatus::Sent`; creation therefore returns false for queued/unconfigured.
- [ ] Run the focused Cargo tests and record passes (GREEN).

### Task 2: Frontend Projection and Eventual Truth

**Files:**
- Modify: `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/invitation-table.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/invitation-table.component.spec.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/invite-dialog.component.spec.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts`

**Interfaces:**
- Consumes: list item `emailDeliveryStatus: 'unconfigured' | 'queued' | 'sent' | 'failed'`.
- Produces: status text visible after the existing list refresh/poll path.

- [ ] Write failing component/store assertions for queued, sent, failed, and unconfigured projections (RED).
- [ ] Run focused dashboard Vitest specs and record failures.
- [ ] Add the API model field and minimal accessible delivery-status rendering, preserving the manual-link fallback.
- [ ] Run focused dashboard Vitest specs and record passes (GREEN).

### Task 3: Contract and Verification

**Files:**
- Modify: `specs/011-tenant-team-management/contracts/rest-api.md`

- [ ] Update create/list response documentation and define `email_sent` as true only for `sent`.
- [ ] Run backend formatting/checks and focused/full tests that are feasible without starting a database.
- [ ] Use Podman only if live PostgreSQL verification is required.
- [ ] Run frontend format, lint, test, and build checks relevant to changed files.
