# Invitation Delivery Leases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove pagination-dependent polling and make outbox processing lock-free during SMTP with bounded retries and poison handling.

**Architecture:** A short transaction leases one eligible outbox event for five minutes using a claim token and increments its attempt count. SMTP runs after commit; token-guarded finalization sends, reschedules, or terminally fails the event, while a tenant-scoped status endpoint supports cancellable frontend polling.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/PostgreSQL, Angular 22, RxJS, NgRx SignalStore, Vitest.

## Global Constraints

- Maximum three delivery attempts; third failure or poison payload is terminal.
- Poll every second; allow three consecutive transient failures one second apart.
- Migration-only schema changes; Podman only; no `tasks.md` edits or commits.

---

### Task 1: Lease-Based Worker and Schema

**Files:** `backend/migrations/0022_outbox_delivery_claims.sql`, `backend/crates/modules/tenancy/src/invitations.rs`, `backend/crates/server/tests/team_members.rs`, `backend/crates/shared/db/tests/schema.rs`

- [ ] Add failing tests proving SMTP executes without an open claim transaction, stale-claim recovery, bounded retry/terminal failure, poison-row advancement, and the production index predicate.
- [ ] Run focused live tests for RED.
- [ ] Add claim columns/index and implement short claim plus token-guarded finalization transactions.
- [ ] Run focused live tests for GREEN.

### Task 2: Targeted Status Polling

**Files:** `backend/crates/modules/tenancy/src/invitations.rs`, `backend/crates/server/src/router.rs`, `backend/crates/server/tests/team_members.rs`, `frontend/apps/dashboard/src/app/features/tenant/team/team-api.service.ts`, `frontend/apps/dashboard/src/app/features/tenant/team/team.store.ts`, related specs.

- [ ] Add failing tenant-isolation/permission endpoint tests and frontend tests for direct lookup, success-reset failure counting, retry exhaustion, cancellation, and surfaced operation error.
- [ ] Run focused backend/frontend tests for RED.
- [ ] Implement `GET /tenant/members/invitations/{id}/delivery` guarded by `members.view` and direct RxJS polling/retry state.
- [ ] Run focused tests for GREEN.

### Task 3: Documentation and Validation

**Files:** `specs/011-tenant-team-management/{plan.md,research.md,contracts/rest-api.md}`

- [ ] Document five-minute leases, three attempts, poison handling, targeted polling, and bounded at-least-once semantics.
- [ ] Run focused backend live tests, frontend tests, compilation, lint, and formatting.
