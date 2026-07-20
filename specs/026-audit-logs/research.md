# Research: Audit Logs (026)

All findings below were verified against the codebase on 2026-07-19 (branch `026-audit-logs`). No NEEDS CLARIFICATION items remained after `/speckit-clarify`; this document records the technical decisions and the code-level evidence behind them.

## R1. Existing write-side coverage (verified inventory)

**Decision**: Reuse the existing append-only store and per-module writers unchanged. The only recording gap to close is **tool executions**. Billing has no auditable actions yet (placeholder crate) — the `billing.*` category is reserved, not instrumented.

**Evidence**:

| Spec category | Status | Where recorded |
|---|---|---|
| Auth events | ✅ recorded | `identity/src/audit.rs` — `auth.login_succeeded`, `auth.login_failed`, `auth.logged_out` |
| Tenant changes | ✅ recorded | `tenancy` — `platform.tenant_created/_updated/_status_changed/_switched`, `tenant.access_denied` |
| User role changes | ✅ recorded | `tenancy/src/audit.rs` — `member.invited/_role_changed/_disabled/_enabled/…`, `skill.*` |
| Prompt changes | ✅ recorded | `ai/src/agent_audit.rs` — `agent_prompt.version_created`, `agent_prompt.version_restored` (the `prompts` crate is a placeholder; prompt management lives in `ai`) |
| AI provider changes | ✅ recorded | `ai/src/audit.rs` — `ai_config.*`, `ai_credential.set/deleted`; `ai/src/agent_audit.rs` — `agent_config.*` |
| Tool executions | ❌ **gap** | `tools/src/audit.rs` only records config changes (`tool.created/updated/deleted` via `record_config_change`); `executor.rs::execute` writes nothing to `audit_logs` |
| Billing changes | ⚠️ nothing to audit | `billing` crate is `//! Placeholder module crate` — no billing actions exist in the product |

Other recorded categories (shown in views per clarification #1): `conversation.*`, `customer.*`, `escalation.*`, `availability.changed`, `knowledge_*.*`, `widget_instance.*`.

**Alternatives considered**: Centralizing all writers into the `audit` crate — rejected: touches 7+ modules for zero behavioral gain; existing writers already follow one convention (same INSERT, `tracing::error!` on failure, transactional `record_in_tx` variants where atomicity matters, satisfying FR-012).

## R2. Immutability (FR-003)

**Decision**: Nothing to build. `0006_audit_logs.sql` installs trigger `audit_logs_append_only` raising an exception on any UPDATE/DELETE; `0013` makes `resource_id` NOT NULL. The new API surface is GET-only, so no role — tenant or platform — has any mutation path.

## R3. Read service location & shape

**Decision**: Activate the placeholder `audit` module crate with `model.rs` / `queries.rs` / `routes.rs`, mirroring the `analytics` crate layout (spec 025, the most recent pattern). Two endpoints:

- `GET /tenant/audit-logs` — `TenantContext`-scoped, guarded by new `Permission::AuditView`.
- `GET /platform/audit-logs` — mounted in `platform_routes()` (platform-permission middleware, no tenant context), guarded by new `Permission::PlatformAuditView`, optional `tenant_id` filter, includes tenant-less rows.

No separate detail endpoint: list rows carry the full `details` JSONB, so the drawer needs no extra fetch (page size ≤ 50 keeps payloads bounded).

**Alternatives considered**: separate `GET /tenant/audit-logs/{id}` — rejected as needless surface; revisit only if metadata payloads grow.

## R4. Permissions & RBAC wiring

**Decision**: Two new catalog permissions (`Permission::ALL` grows 28 → 30; the `catalog_parity_with_contract` test list in `permission.rs` must be updated in the same change):

- `audit.view` (`Permission::AuditView`): added to `Permission::TENANT` (Owner inherits all) **and** `TENANT_ADMIN`. NOT added to `TENANT_MANAGER`/`TENANT_AGENT`/`TENANT_VIEWER` (clarified: Owner/Admin only). Because it lands in both TENANT and TENANT_ADMIN, the existing matrix test asserting the Owner−Admin difference is exactly `{BillingView, BillingManage, TenantDelete, OwnerAssign}` still passes.
- `platform.audit.view` (`Permission::PlatformAuditView`): added to all five platform arrays (`PLATFORM_ALL`, `PLATFORM_DEVELOPER`, `PLATFORM_TENANT_ACCESS`, `PLATFORM_SUPPORT`, `PLATFORM_FINANCE`) per clarification #2.
- Production staff switched into a tenant (`staff_tenant_permissions`) do **not** get `audit.view` (except SuperAdmin, who inherits full TENANT): staff investigate via the platform view instead.
- `rbac.rs`: add `/test/tenant/audit/view` → `audit.view` row and `/test/platform/audit/view` to the platform test-route lists; add matching closure test routes in `router.rs`.

**Alternatives considered**: reusing `settings.view` or `platform.admin` — rejected: wrong role sets (Manager has neither audit access nor settings; platform view must reach Sales/Finance who lack `platform.admin`).

## R5. Category model & filtering

**Decision**: Category is **derived from the action prefix** (segment before the first `.`, with named groupings) — no schema change, works for all historical rows. Canonical mapping (also the filter contract):

| Category code | Action prefixes |
|---|---|
| `auth` | `auth.` |
| `tenant` | `platform.`, `tenant.` |
| `members` | `member.`, `skill.`, `availability.` |
| `prompts` | `agent_prompt.` |
| `ai` | `ai_config.`, `ai_credential.`, `agent_config.` |
| `tools` | `tool.` |
| `billing` | `billing.` (reserved — no writers yet) |
| `conversations` | `conversation.` |
| `customers` | `customer.` |
| `escalations` | `escalation.` |
| `knowledge` | `knowledge_category.`, `knowledge_document.`, `knowledge_item.` |
| `widgets` | `widget_instance.` |

Unknown prefixes fall back to category `other`. SQL: `action LIKE ANY($prefix_array)` per selected category; response DTO includes both raw `action` and derived `category`. Filters (date range from/to, category, actor id, tenant id on platform) combine with AND (FR-006).

**Alternatives considered**: adding a `category` column — rejected: migration + backfill + writer changes across modules for something derivable; violates "keep write side untouched".

## R6. Pagination

**Decision**: Cursor pagination copied from the conversations inbox pattern (`queries::encode_cursor(created_at, id)`-style opaque base64 cursor, `ORDER BY created_at DESC, id DESC`, `limit` clamped 1..=100, default 50, `pagination.next_cursor` echoed back verbatim). Consistent with the workspace HTTP contract (`PaginatedResponse<T>`/`ApiListQuery`, frontend/CLAUDE.md).

## R7. Actor & target presentation

**Decision**: Single query with `LEFT JOIN users u ON u.id = audit_logs.actor_user_id` (no `deleted_at` filter — deleted users must still resolve). DTO actor object: `{ kind: "user" | "system", id?, display_name?, email?, is_platform_staff (u.platform_role IS NOT NULL), deleted (u.deleted_at IS NOT NULL) }`. Rows with `actor_user_id NULL` present as `kind: "system"` (covers automation and unauthenticated events like `auth.login_failed`, whose attempted email lives in `details`/`resource_id`). `is_platform_staff` satisfies clarification #3 (staff actions visible in tenant view, identified and marked). FR-011 (deleted actor labeling) handled by `deleted` flag; missing-user join miss also maps to a labeled fallback.

## R8. Indexing (SC-005)

**Decision**: One additive migration `0053_audit_read_indexes.sql`: `CREATE INDEX audit_logs_actor_created_idx ON audit_logs (actor_user_id, created_at DESC);` for the actor filter. Existing `audit_logs_tenant_created_idx (tenant_id, created_at DESC)` and `audit_logs_created_idx (created_at DESC)` already cover the tenant and platform listing paths. Category filtering rides the tenant/time index (prefix LIKE on the residual rows is fine at the tens-of-thousands scale in SC-005).

## R9. Tool-execution audit writer (the one new writer)

**Decision**: Add `tools::audit::record_execution` writing action `tool.executed` (resource_type `tenant_tool`, resource_id = tool id, details: tool name, conversation id, outcome/status, approval mode), called from `tools/src/executor.rs::execute` at completion (success and failure outcomes). Non-transactional `record` variant with `tracing::error!` on failure, matching `record_config_change` and FR-012.

## R10. OpenAPI & coverage gates

**Decision**: Handlers use `#[utoipa::path]` + `routes!()` co-registration (required — plain `.route()` registrations fail `openapi_coverage.rs`). Add `("GET", "/api/v1/tenant/audit-logs")` and `("GET", "/api/v1/platform/audit-logs")` to the `EXPECTED` inventory in `openapi_coverage.rs`; register DTO schemas per `server/src/openapi.rs` conventions (follow whatever analytics needed there).

## R11. Frontend structure

**Decision**:

- Presentational, reusable pieces in `shared/components/`: `audit-log-table` (wraps existing `data-table`) and `audit-detail-drawer` (follows `version-history-drawer` / `dialog-shell` pattern). This lets tenant and platform pages share UI without cross-lazy-area imports (Principle IX).
- Routed pages per area: `features/tenant/audit-logs/` (component + `audit-logs-api.service.ts` + SignalStore + specs) and `features/platform/audit-logs/` (component + store + specs). Registered via `APP_PATHS`/`PAGE_PERMISSIONS` with `requiredPermission: 'audit.view'` (tenant) and `'platform.audit.view'` (platform); sidebar/nav entries added per area.
- Filters built from existing `select-filter` (category, and tenant on platform page) + date-range inputs following the analytics page's pattern; actor filter as search-input backed by actor id from selected row or free entry (exact UX decided in tasks against the analytics filter bar).
- Typed fixtures in `shared/fixtures/audit.fixtures.ts` for specs; cursor "Load more" like the conversations inbox.
- State: NgRx SignalStore per feature page (frontend/CLAUDE.md); RxJS-first data flows (constitution).

## R12. Retention & deferred items

**Decision**: Indefinite retention (spec assumption); no purge/export/alerting/real-time. Permission-denied auditing beyond failed sign-ins stays out (note: `authz.denied_permission` string exists in guard telemetry but is not an audit row — unchanged).
