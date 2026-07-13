# Permissions Contract: Conversation Core

**Feature**: 013-conversation-core | **Date**: 2026-07-13

## Permission codes — none added

The 008 catalog already contains both codes this feature needs; the role→permission matrix (`backend/crates/modules/authz/src/matrix.rs`) already grants them exactly as the spec's clarified model requires. **No catalog, matrix, or frontend permission-type changes.**

| Code | Owner / Admin / Manager / Agent | Viewer |
|------|--------------------------------|--------|
| `conversations.view` | ✅ | ✅ |
| `conversations.manage` | ✅ | ❌ |

This matches spec FR-015 (all roles view; Agent-and-above compose/status/assign/create; Viewer read-only) and Q5 (Agents may assign anyone active — `conversations.manage` is the only gate; there is no separate assignment permission).

Platform staff in a tenant's context follow the existing `staff_tenant_permissions` rules (e.g., production Support holds both codes; production Developer holds `conversations.view` only) — no changes.

## Route → permission map (new registrations in `server/src/router.rs`, all under `mount_tenant` via `.guarded()`)

| Route | Method | Permission |
|-------|--------|------------|
| `/tenant/conversations` | GET | `conversations.view` |
| `/tenant/conversations` | POST | `conversations.manage` |
| `/tenant/conversations/{id}` | GET | `conversations.view` |
| `/tenant/conversations/{id}` | PATCH | `conversations.manage` |
| `/tenant/conversations/{id}/messages` | GET | `conversations.view` |
| `/tenant/conversations/{id}/messages` | POST | `conversations.manage` |
| `/tenant/customers/{id}/conversations` | GET | `customers.view` (existing, unchanged) |

`rbac.rs` route→permission matrix tests extend to cover all six conversation routes.

## Frontend page permissions (`core/authz/permissions.ts`)

| Page / control | Gate |
|----------------|------|
| `APP_PATHS.tenant.conversations` (inbox) | `conversations.view` (existing entry, unchanged) |
| `APP_PATHS.tenant.conversationDetail` (new `conversations/:id`) | `conversations.view` |
| Composer (all modes), status control, assignee control, "New conversation" action | rendered only with `conversations.manage` (UI convenience — server remains the enforcement point, 008 FR-010: no frontend role→permission mapping) |
