# Data Model: Audit Logs (026)

## Storage (existing — no schema changes)

### `audit_logs` (created in `0006_audit_logs.sql`, amended `0010`/`0013`)

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | `gen_random_uuid()` |
| `actor_user_id` | UUID NULL → `users(id)` ON DELETE RESTRICT | NULL = system/unattributed actor |
| `action` | TEXT NOT NULL | namespaced `prefix.verb`, length 1..=100 (CHECK) |
| `resource_type` | TEXT NOT NULL | e.g. `user`, `tenant`, `invitation`, `tenant_tool`, `agent_configuration` |
| `resource_id` | TEXT NOT NULL | `audit_logs_resource_required` CHECK (0013) |
| `tenant_id` | UUID NULL → `tenants(id)` ON DELETE RESTRICT | NULL = platform-level event |
| `details` | JSONB NOT NULL DEFAULT `{}` | free-form context (changed fields, outcome, …) |
| `created_at` | TIMESTAMPTZ NOT NULL | event timestamp; list sort key |
| `updated_at` | TIMESTAMPTZ NOT NULL | vestigial (rows are immutable) |

**Immutability**: trigger `audit_logs_append_only` raises on UPDATE/DELETE (FR-003).

**Existing indexes**: `audit_logs_tenant_created_idx (tenant_id, created_at DESC)`, `audit_logs_created_idx (created_at DESC)`.

**New migration `0053_audit_read_indexes.sql`**:

```sql
CREATE INDEX audit_logs_actor_created_idx
    ON audit_logs (actor_user_id, created_at DESC);
```

### `users` (joined for actor display)

`LEFT JOIN users u ON u.id = a.actor_user_id` **without** filtering `deleted_at` (FR-011). Read: `display_name`, `email`, `platform_role` (NOT NULL ⇒ platform staff), `deleted_at` (NOT NULL ⇒ label "deleted user").

## API DTOs (crate `audit`, `model.rs` — snake_case fields, `ToSchema`)

### `AuditActorDto`

| Field | Type | Notes |
|---|---|---|
| `kind` | `"user" \| "system"` | `system` when `actor_user_id` IS NULL or join misses |
| `id` | `Option<Uuid>` | |
| `display_name` | `Option<String>` | |
| `email` | `Option<String>` | |
| `is_platform_staff` | `bool` | `u.platform_role IS NOT NULL` (clarification #3 / FR-013) |
| `deleted` | `bool` | `u.deleted_at IS NOT NULL` (FR-011) |

### `AuditEntryDto`

| Field | Type | Notes |
|---|---|---|
| `id` | Uuid | |
| `action` | String | raw action, e.g. `member.role_changed` |
| `category` | String | derived (see mapping below) |
| `actor` | AuditActorDto | |
| `resource_type` | String | |
| `resource_id` | String | |
| `tenant_id` | `Option<Uuid>` | present on platform endpoint; on tenant endpoint always == context tenant |
| `details` | `serde_json::Value` | full metadata — powers the drawer (no detail endpoint) |
| `created_at` | `DateTime<Utc>` | |

### Envelope

`{ "data": [AuditEntryDto…], "pagination": { "next_cursor": string|null, "has_more": bool } }` — same shape as conversations inbox (`Pagination` / `PaginatedResponse`).

## Query parameters

Both endpoints: `cursor` (opaque, from `next_cursor`), `limit` (1..=100, default 50), `from` / `to` (inclusive UTC dates `YYYY-MM-DD`, analytics-style validation), `category` (code below), `actor_id` (UUID). Platform endpoint adds `tenant_id` (UUID; also accepts `tenant_id=none` for platform-level rows only — decided at tasks time whether to expose the `none` sentinel or omit v1). All filters AND-combine (FR-006). Invalid values ⇒ 422 `ApiError::unprocessable_entity`.

## Category mapping (canonical, single source in `audit::model`)

| `category` code | action prefixes |
|---|---|
| `auth` | `auth.` |
| `tenant` | `platform.`, `tenant.` |
| `members` | `member.`, `skill.`, `availability.` |
| `prompts` | `agent_prompt.` |
| `ai` | `ai_config.`, `ai_credential.`, `agent_config.` |
| `tools` | `tool.` |
| `billing` | `billing.` (reserved) |
| `conversations` | `conversation.` |
| `customers` | `customer.` |
| `escalations` | `escalation.` |
| `knowledge` | `knowledge_category.`, `knowledge_document.`, `knowledge_item.` |
| `widgets` | `widget_instance.` |
| `other` | anything unmatched (derived only — not a filter value in v1) |

Filtering: `a.action LIKE ANY($1)` where `$1` = the selected category's prefix list + `%`. Derivation for display: longest-prefix match over the same table.

## New audit write (tool executions)

`tools::audit::record_execution` → row: action `tool.executed`, resource_type `tenant_tool`, resource_id = tool id, tenant_id = execution tenant, actor = triggering membership's user (or NULL when AI-autonomous), details `{ "tool_name", "conversation_id", "outcome", "approval_mode" }`. Called from `executor.rs::execute` on both success and failure outcomes; write failure logs via `tracing::error!` and never fails the execution (FR-012).

## Permissions (authz crate)

| Permission | Code | Granted to |
|---|---|---|
| `Permission::AuditView` | `audit.view` | `Permission::TENANT` (⇒ Owner) + `TENANT_ADMIN` |
| `Permission::PlatformAuditView` | `platform.audit.view` | all five platform arrays |

`Permission::ALL` 28 → 30; update `catalog_parity_with_contract` test list. Owner−Admin difference set unchanged.

## State transitions

None — audit entries are created once and never change (append-only).
