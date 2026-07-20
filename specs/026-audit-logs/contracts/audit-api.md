# API Contract: Audit Logs (026)

Conventions: JSON snake_case; error envelope + `X-Request-Id` per workspace contract; utoipa-documented via `routes!()` co-registration (required by `openapi_coverage.rs`); GET-only surface (immutability is DB-enforced — there is deliberately no write/edit/delete endpoint).

## GET `/api/v1/tenant/audit-logs`

Tenant-scoped audit list. **Auth**: session + `X-Tenant-ID` (TenantContext) + permission `audit.view` (tenant Owner/Admin; SuperAdmin-switched staff inherit full tenant set).

**operation_id**: `list_tenant_audit_logs` — tag `audit`

### Query parameters

| Param | Type | Default | Notes |
|---|---|---|---|
| `cursor` | string | — | opaque; pass back `pagination.next_cursor` verbatim |
| `limit` | int | 50 | clamped 1..=100 |
| `from` | date `YYYY-MM-DD` | — | inclusive UTC start |
| `to` | date `YYYY-MM-DD` | — | inclusive UTC end |
| `category` | string enum | — | `auth\|tenant\|members\|prompts\|ai\|tools\|billing\|conversations\|customers\|escalations\|knowledge\|widgets` |
| `actor_id` | UUID | — | filter by actor user id |

All filters AND-combine. Invalid date/category/actor_id/cursor → `422`.

### Response `200`

```json
{
  "data": [
    {
      "id": "0c9f…",
      "action": "member.role_changed",
      "category": "members",
      "actor": {
        "kind": "user",
        "id": "7be1…",
        "display_name": "Dana Ops",
        "email": "dana@acme.test",
        "is_platform_staff": false,
        "deleted": false
      },
      "resource_type": "membership",
      "resource_id": "91d2…",
      "tenant_id": "5f3a…",
      "details": { "from": "agent", "to": "manager" },
      "created_at": "2026-07-18T14:03:22Z"
    }
  ],
  "pagination": { "next_cursor": "eyJ…", "has_more": true }
}
```

Ordering: `created_at DESC, id DESC` (keyset). Rows are always `tenant_id = <context tenant>` — platform-level (NULL-tenant) rows never appear here. `actor.kind = "system"` when the row has no actor user (automation, failed sign-ins). Entries include platform-staff actors with `is_platform_staff: true` (FR-013).

### Errors

| Status | When |
|---|---|
| 401 | no/invalid session |
| 403 | authenticated but lacking `audit.view` (Manager/Agent/Viewer) |
| 422 | invalid query parameter or cursor |

## GET `/api/v1/platform/audit-logs`

Cross-tenant audit list including platform-level (NULL-tenant) rows. **Auth**: session + platform role (platform middleware) + permission `platform.audit.view` (all platform roles). No `X-Tenant-ID` involved.

**operation_id**: `list_platform_audit_logs` — tag `audit`

### Query parameters

Same as tenant endpoint, plus:

| Param | Type | Notes |
|---|---|---|
| `tenant_id` | UUID | restrict to one tenant's rows |

### Response `200`

Same envelope/DTO; `tenant_id` is populated per row (or `null` for platform-level events).

### Errors

| Status | When |
|---|---|
| 401 | no/invalid session |
| 403 | tenant-only user (no platform role) or missing `platform.audit.view` |
| 422 | invalid query parameter or cursor |

## Coverage-gate additions (same change as route registration)

- `backend/crates/server/tests/openapi_coverage.rs` `EXPECTED`: `("GET", "/api/v1/tenant/audit-logs")`, `("GET", "/api/v1/platform/audit-logs")`.
- `backend/crates/server/tests/rbac.rs`: add the **real** routes (no test-closure routes needed — analytics does the same): `("/api/v1/tenant/audit-logs", "audit.view")` in `TENANT_OPERATIONS`, and `"/api/v1/platform/audit-logs"` in `PLATFORM_OPERATIONS`.
- `server/src/openapi.rs` `components(schemas(...))`: `audit::model::AuditActorDto`, `audit::model::AuditEntryDto` (+ pagination wrapper if not reusing an already-registered generic).

## Frontend consumers

- Tenant page `features/tenant/audit-logs` → `GET /tenant/audit-logs` via `audit-logs-api.service.ts`; route guard `requiredPermission: 'audit.view'`.
- Platform page `features/platform/audit-logs` → `GET /platform/audit-logs`; route guard `requiredPermission: 'platform.audit.view'`.
- Both render `shared/components/audit-log-table` + `audit-detail-drawer`; drawer consumes the row's `details` (no extra fetch).
