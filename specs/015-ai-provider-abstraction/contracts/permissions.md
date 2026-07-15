# Permissions Contract: AI Provider Abstraction

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

**Zero new permission codes.** The feature reuses the existing vocabulary from `backend/crates/modules/authz/src/permission.rs`; the role matrices are untouched.

## Reused codes

| Code | Used for |
|---|---|
| `ai_agent.view` | Reading the tenant's effective AI configuration and usage records/summaries |
| `ai_agent.manage` | Writing tenant AI configuration, BYOK credentials, connectivity test, and reading captured content (FR-018 access restriction) |
| `platform.admin` | All platform-default configuration and platform key management |

Rationale: the spec (Assumptions) reserves AI configuration for tenant Owner/Admin and equivalent platform roles — exactly the populations the existing matrix grants `ai_agent.manage` and `platform.admin`. Usage reads sit under `ai_agent.view` (not `analytics.view`) because usage is AI-subsystem data and the AI settings surface will consume it.

## Route → permission map (additions to `server/tests/rbac.rs`)

| Method | Route | Permission | Mount |
|---|---|---|---|
| GET | `/tenant/ai/config` | `ai_agent.view` | `mount_tenant` |
| PUT | `/tenant/ai/config` | `ai_agent.manage` | `mount_tenant` |
| DELETE | `/tenant/ai/config` | `ai_agent.manage` | `mount_tenant` |
| PUT | `/tenant/ai/credentials/{provider}` | `ai_agent.manage` | `mount_tenant` |
| DELETE | `/tenant/ai/credentials/{provider}` | `ai_agent.manage` | `mount_tenant` |
| POST | `/tenant/ai/config/test` | `ai_agent.manage` | `mount_tenant` |
| GET | `/tenant/ai/usage` | `ai_agent.view` | `mount_tenant` |
| GET | `/tenant/ai/usage/summary` | `ai_agent.view` | `mount_tenant` |
| GET | `/tenant/ai/usage/{id}` | `ai_agent.manage` | `mount_tenant` |
| GET | `/platform/ai/config` | `platform.admin` | `mount_platform` |
| PUT | `/platform/ai/config` | `platform.admin` | `mount_platform` |
| PUT | `/platform/ai/credentials/{provider}` | `platform.admin` | `mount_platform` |
| DELETE | `/platform/ai/credentials/{provider}` | `platform.admin` | `mount_platform` |
| POST | `/platform/ai/config/test` | `platform.admin` | `mount_platform` |

All registered through the deny-by-default `.guarded()` / `.guarded_with_methods()` builders — an unmapped route cannot ship unguarded.

## Isolation rules (tested in the `server/tests/ai.rs` matrix)

- Tenant routes operate strictly on the middleware-resolved tenant; referencing another tenant's usage-record id → `not_found` (never 403).
- Tenant routes can never read or write platform-scope rows; platform-default values surface to tenants only through the resolved `GET /tenant/ai/config` view (scope-labeled, key masked).
- The in-process `AiService` takes `tenant_id` from the caller's already-authorized context; it performs no authorization itself (callers are guarded routes/modules) but scopes every query by that tenant.
- Content capture: captured `request_content`/`response_content` is retrievable only via `GET /tenant/ai/usage/{id}` under `ai_agent.manage` of the owning tenant.
