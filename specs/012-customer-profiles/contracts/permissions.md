# Permissions Contract: Customer Profiles

**No new permission codes. No matrix changes.** The 008 catalog and matrix already provide exactly what clarification Q4 requires.

## Reused permission codes

| Code | Meaning here |
|------|--------------|
| `customers.view` | See the customer list, search, open profiles, read conversation history |
| `customers.manage` | Create customers, update contact info / identifiers / metadata |

## Role grants (existing matrix, unchanged)

| Tenant role | `customers.view` | `customers.manage` |
|-------------|------------------|--------------------|
| Owner | ✅ | ✅ |
| Admin | ✅ | ✅ |
| Manager | ✅ | ✅ |
| Agent | ✅ | ✅ |
| Viewer | ✅ | ❌ (read-only) |

Platform users operating in a tenant context receive tenant-scoped access via the existing platform-role → tenant-context grants; there is no cross-tenant customer surface anywhere (spec assumption).

## Route → permission map (all via `.guarded()`, deny-by-default)

| Route | Permission |
|-------|------------|
| `GET /tenant/customers` | `customers.view` |
| `POST /tenant/customers` | `customers.manage` |
| `GET /tenant/customers/{id}` | `customers.view` |
| `PATCH /tenant/customers/{id}` | `customers.manage` |
| `GET /tenant/customers/{id}/conversations` | `customers.view` |

## Frontend gating (display only — never the enforcement layer)

| Surface | Gate |
|---------|------|
| Customers page + profile page (`PAGE_PERMISSIONS`) | `customers.view` (list entry already present; profile route reuses it) |
| "New customer" button, edit actions, dialog | `customers.manage` |
| Sidebar "Customers" item | `customers.view` (already wired) |

Per 008 FR-010, the frontend consumes permission codes from the session payload; it never maps roles to permissions itself. UI hiding is convenience — the server-side `.guarded()` checks are the enforcement (spec FR-012).

## Enforcement layering (request path)

1. Session auth (401 if absent).
2. Tenant-context middleware resolves tenant from `X-Tenant-ID` + membership (`mount_tenant`).
3. `.guarded()` permission check (403 if the role lacks the code).
4. Handler queries always filter by the middleware-resolved `tenant_id` — cross-tenant ids fall out as `404 not_found` (never 403, spec FR-011).
