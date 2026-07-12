# Contract: REST API — Platform Tenant Management

**Feature**: 010-platform-tenant-management. Extends the platform REST contract; error envelope, `X-Request-Id`, cursor pagination, and the 008 authorization semantics (401 `unauthenticated` / 403 `unauthorized`, deny-by-default) unchanged.

## Endpoint summary

| Endpoint | Scope | Required permission | Change |
|----------|-------|--------------------|--------|
| `GET /api/v1/platform/tenants` | platform | `platform.tenants.list` | extended — `status` filter; summary gains `plan` |
| `POST /api/v1/platform/tenants` | platform | `platform.tenants.manage` | **new** |
| `GET /api/v1/platform/tenants/{id}` | platform | `platform.tenants.list` | **new** |
| `PATCH /api/v1/platform/tenants/{id}` | platform | `platform.tenants.manage` | **new** |
| `POST /api/v1/platform/tenants/{id}/switch` | platform | `platform.tenants.switch` | unchanged |

All registered through the fail-closed `.guarded()` builder. Tenant users (no platform role) receive 403 on every one of them.

## `GET /platform/tenants` (extended)

Query params: `q` (existing — matches name or slug, ILIKE), `status` (**new** — `active` | `suspended`; any other value → 422 `validation_failed`), `cursor`/`limit` (existing). All combinable; one SQL statement.

```jsonc
// 200 — Page envelope (existing shape)
{
  "items": [
    { "id": "…", "name": "Acme", "slug": "acme", "status": "active", "plan": "professional" } // ★ plan added
  ],
  "nextCursor": "…", "hasMore": true
}
```

## `POST /platform/tenants` (new)

```jsonc
// Request
{
  "name": "Acme Support",            // required, 1–200 chars
  "slug": "acme-support",            // required, ^[a-z0-9](-?[a-z0-9])*$, ≤63, unique among live tenants
  "plan": "starter",                 // optional — trial|starter|professional|enterprise; default "trial"
  "contactName": "Jane Doe",         // optional, 1–200 chars
  "contactEmail": "jane@acme.test"   // optional, must be a valid email when present
}
// 201 — TenantDetail (below). New tenant status is always "active".
```

Audit: `platform.tenant_created` (actor, tenant id, name/slug/plan).

## `GET /platform/tenants/{id}` (new)

```jsonc
// 200 — TenantDetail
{
  "id": "…",
  "name": "Acme Support",
  "slug": "acme-support",
  "status": "active",
  "plan": "starter",
  "contactName": "Jane Doe",     // null when unset
  "contactEmail": "jane@acme.test", // null when unset
  "createdAt": "2026-07-11T10:00:00Z",
  "updatedAt": "2026-07-11T10:00:00Z"
}
```

404 `not_found` for unknown or soft-deleted ids.

## `PATCH /platform/tenants/{id}` (new)

Partial update — only provided fields are validated and applied; omitted fields untouched. Accepts any subset of: `name`, `slug`, `plan`, `contactName` (nullable to clear), `contactEmail` (nullable to clear), `status` (`active` | `suspended`).

- 200 → updated TenantDetail.
- Concurrency: last write wins; every write audited (spec edge case).
- **Transaction contract**: the handler runs UPDATE + audit insert in one DB transaction that first calls `set_audit_actor(actor_id)` — required by the live `tenants_slug_change_audit` trigger whenever `slug` changes (the trigger emits the `tenant.slug_changed` audit row itself).
- Audit: field changes → `platform.tenant_updated` (old/new per changed field, slug excluded — trigger owns it); a `status` change → `platform.tenant_status_changed` (old/new). A PATCH mixing both emits both actions.
- Status effect: suspension/reactivation takes effect on the affected members' next request via the existing tenant-context middleware — no new mechanism.

## Error vocabulary (existing kernel codes)

| Situation | Status | Code | Notes |
|-----------|--------|------|-------|
| Missing/invalid field (name, slug format, plan value, email format, bad `status` filter) | 422 | `validation_failed` | `details: [{ field, message }]` per offending field |
| Slug already used by a live tenant (create or patch) | 409 | `conflict` | message names the problem, not the owning tenant |
| Unknown/soft-deleted tenant id | 404 | `not_found` | |
| Anonymous | 401 | `unauthenticated` | |
| Authenticated without required permission (incl. all tenant users) | 403 | `unauthorized` | body: "Access denied" (008 contract) |

## Frontend route contract

| Route | Guard data (`requiredPermission`) | Page |
|-------|------------------------------------|------|
| `/platform/tenants` | `platform.tenants.list` | tenant list (search, status filter, load-more, create entry for managers) |
| `/platform/tenants/new` | `platform.tenants.list`* | create form (*submit enforced server-side by `manage`; the form's entry points are hidden without it) |
| `/platform/tenants/:id` | `platform.tenants.list` | detail (+ edit/status actions gated by `*appHasPermission="'platform.tenants.manage'"`) |
| `/platform` (area base) | `platform.tenants.list` (was `platform.admin`) | area gate rebalanced — see contracts/permissions.md |

Breadcrumbs follow the 009 mechanism: `Platform / Tenants`, `Platform / Tenants / Tenant details`, `Platform / Tenants / New tenant` (static labels from `PAGE_TITLES`).
