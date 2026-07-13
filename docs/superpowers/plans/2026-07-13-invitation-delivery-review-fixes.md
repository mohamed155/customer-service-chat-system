# Invitation Delivery Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make queued invitation email durable and observable through terminal delivery state while preserving non-blocking invitation creation.

**Architecture:** Invitation creation writes an invitation and an `invitation.email_delivery` outbox event atomically. A supervised worker claims events with `FOR UPDATE SKIP LOCKED`, sends through `EmailSender`, and atomically transitions the tenant/id-scoped invitation plus outbox event; the Angular store polls list state for the created invitation until terminal.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/PostgreSQL, Angular 22, RxJS, NgRx SignalStore, Vitest.

## Global Constraints

- Strict test-first RED/GREEN development.
- Migration-only schema changes and Podman-only database infrastructure.
- Invitation creation must not await email transport.
- Do not edit `tasks.md` or commit.

---

### Task 1: Durable Backend Delivery

**Files:**
- Create: `backend/migrations/0021_invitation_delivery_outbox.sql`
- Modify: `backend/crates/modules/tenancy/src/invitations.rs`
- Modify: `backend/crates/server/src/main.rs`
- Modify: `backend/crates/server/src/router.rs`
- Test: `backend/crates/server/tests/team_members.rs`
- Test: `backend/crates/shared/db/tests/schema.rs`

**Interfaces:**
- Produces: `process_invitation_deliveries_once(pool, sender)` and `run_invitation_delivery_worker(pool, sender)`.

- [ ] Add failing tests for blocked transport response latency, seeded queued-event recovery, exact camelCase create/list shape with inviter, and rejected invalid delivery status.
- [ ] Run focused tests against Podman PostgreSQL and record RED.
- [ ] Insert the outbox event in the invitation transaction and remove request-owned detached sending.
- [ ] Implement one-pass claim/send/terminal transition with tenant/id predicates and `FOR UPDATE SKIP LOCKED`.
- [ ] Start and supervise the repeating worker from the server composition root using the same injected `EmailSender` port as HTTP composition.
- [ ] Run focused backend and schema tests and record GREEN.

### Task 2: Observable Frontend Delivery

**Files:**
- Modify: `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team.store.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.ts`
- Modify: `frontend/apps/dashboard/src/app/features/tenant/team/invitation-table.component.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/team.store.spec.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/invitation-table.component.spec.ts`
- Test: `frontend/apps/dashboard/src/app/features/tenant/team/team-list.component.spec.ts`

**Interfaces:**
- Consumes: full camelCase `TenantInvitation` in create and list responses.
- Produces: queued-to-terminal polling that patches list and dialog result.

- [ ] Add failing realistic queued-to-sent polling and semantic header/cell alignment tests.
- [ ] Run focused Vitest specs and record RED.
- [ ] Add RxJS polling that stops on terminal status or replacement/reset and patches both projections.
- [ ] Correct invitation table cell order and align create response types/fixtures.
- [ ] Run focused Vitest specs and record GREEN.

### Task 3: Contract and Focused Verification

**Files:**
- Modify: `specs/011-tenant-team-management/contracts/rest-api.md`

- [ ] Document exact camelCase create/list fields, full inviter shape, outbox recovery, and `emailSent` semantics.
- [ ] Run focused backend live tests, schema tests, frontend tests, checks, lint, and formatting.
