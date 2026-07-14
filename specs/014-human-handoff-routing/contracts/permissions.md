# Permissions Contract: Human Handoff & Routing

**Feature**: 014-human-handoff-routing | **Date**: 2026-07-14

**No new permission codes. No matrix changes.** (Research R8 — the 008 catalog already expresses the spec's "conversation-management" and "team-management" permission language.)

## Route → permission map

| Route | Method | Permission |
|-------|--------|------------|
| `/tenant/conversations/{id}/escalate` | POST | `conversations.manage` |
| `/tenant/escalations/queue` | GET | `conversations.view` |
| `/tenant/escalations/{id}/claim` | POST | `conversations.manage` |
| `/tenant/availability/me` | GET | `conversations.manage` |
| `/tenant/availability/me` | PUT | `conversations.manage` |
| `/tenant/skills` | GET | `members.view` |
| `/tenant/skills` | POST | `members.manage` |
| `/tenant/skills/{id}` | PATCH | `members.manage` |
| `/tenant/skills/{id}` | DELETE | `members.manage` |
| `/tenant/members/{membershipId}/skills` | PUT | `members.manage` |
| `/tenant/events` (SSE) | GET | `conversations.view` |

All registered through the deny-by-default `.guarded()`/`.guarded_with_methods()` builder under `mount_tenant`; `server/tests/rbac.rs` route→permission map extended accordingly.

## Semantic rules beyond route guards

- **Self-only availability**: the `/me` path shape makes the caller the resource; handlers never accept a target membership (FR-016).
- **Agent-capable routing targets** (FR-009): routing, claiming, drain, and manual reassignment only ever target active memberships whose role grants `conversations.manage` — Owner, Admin, Manager, Agent. Viewer is excluded from targets, availability rows, and skill assignment (`422` on `PUT …/skills` for a Viewer).
- **Queue visibility**: `conversations.view` (all roles incl. Viewer can see the queue, consistent with 013's "viewing available to all roles"); claiming requires `conversations.manage`, and the claim button is hidden without it (UI hint only — server enforces).
- **Cross-tenant**: any reference to another tenant's escalation, skill, or membership → `404 not_found`.

## Page permissions (frontend)

| Surface | Requirement |
|---------|-------------|
| Escalation queue page (`APP_PATHS.tenant.escalations`) | route data `conversations.view`; claim action gated by `conversations.manage` |
| Availability toggle (topbar) | rendered only when caller holds `conversations.manage` (Viewer never routes) |
| Skills manager (team page) | `members.manage`; read-only skill chips visible with `members.view` |
| Escalation banner / routing reason (conversation detail) | shown with conversation view access — no extra gate (FR-021) |
