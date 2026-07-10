# Contract: REST API Changes

**Feature**: 008-rbac-permissions. Extends the platform REST contract (`specs/001-ai-customer-service-platform/contracts/rest-api.md`); error envelope, `X-Request-Id`, and pagination conventions unchanged.

## Authorization semantics (all `/api/v1` routes)

| Situation | Status | Error `code` | Message |
|-----------|--------|--------------|---------|
| No/invalid session | 401 | `unauthenticated` | `Authentication required` |
| Signed in, permission missing | 403 | `unauthorized` | `Access denied` |
| Route with no declared permission | 403 | `unauthorized` | `Access denied` (deny by default, FR-003) |

- 403 bodies never reveal whether the target resource exists or what permission was required (FR-006). The denied permission is recorded server-side only (tracing span `authz.denied_permission`; tenant-scope denials also audited with reason `permission_denied`).
- Existing behavior preserved: tenant-context failures keep their current codes (`validation_failed` for header problems, 403 `Access denied` for no membership/not found, 403 `Tenant is suspended`).

## Endpoint permission declarations (current surface)

| Endpoint | Scope | Required permission |
|----------|-------|---------------------|
| `POST /api/v1/auth/login` | public | — (guest) |
| `POST /api/v1/auth/logout` | public | — (any authenticated) |
| `GET /api/v1/me` | authenticated | — (any authenticated; returns the caller's own identity) |
| `GET /api/v1/tenant` | tenant | `overview.view` |
| `GET /api/v1/platform/tenants` | platform | `platform.tenants.list` |
| `POST /api/v1/platform/tenants/{id}/switch` | platform | `platform.tenants.switch` |

Every future endpoint MUST appear in one of the three route groups (`public`, `platform`, `tenant`) with a declared permission; the registration API makes the declaration a required argument.

## `GET /api/v1/me` — extended response

New fields marked ★. All permission arrays are **server-computed** from the canonical matrix (`contracts/permissions.md`); the client never derives permissions from role names (FR-010).

```jsonc
{
  "id": "6f0a…",
  "email": "owner@acme.test",
  "displayName": "Ada Owner",
  "platformRole": null,                     // unchanged; e.g. "support" for staff
  "platformPermissions": [],                // ★ platform-scope set; [] for tenant users
  "staffTenantPermissions": null,           // ★ platform users only: environment-resolved
                                            //   tenant-scope set they hold inside any tenant;
                                            //   null for tenant users
  "memberships": [
    {
      "tenantId": "0d9c…",
      "tenantName": "Acme",
      "tenantSlug": "acme",
      "role": "owner",                      // unchanged
      "permissions": [                      // ★ effective tenant-scope set for this role
        "overview.view", "conversations.view", "conversations.manage", "…"
      ]
    }
  ]
}
```

Example — platform Support Engineer in production:

```jsonc
{
  "platformRole": "support",
  "platformPermissions": ["platform.tenants.list", "platform.tenants.switch"],
  "staffTenantPermissions": [
    "overview.view",
    "conversations.view", "conversations.manage",
    "customers.view", "customers.manage",
    "knowledge_base.view"
  ],
  "memberships": []
}
```

- Freshness: values reflect the database state at request time (FR-011). Clients MUST re-fetch `/me` after receiving any 403 `unauthorized` and after a tenant switch.

## Frontend route contract

| Route (dashboard) | Guard data |
|-------------------|-----------|
| `/tenant/<page>` | `requiredPermission: '<page>.view'` per the page→permission table in `contracts/permissions.md` |
| `/platform/**` | `requiredPermission: 'platform.admin'` |
| `/tenant/select` | authenticated only (no permission) |

Guard behavior (FR-008, research R8): `CanMatch` — a disallowed route never loads its lazy bundle; redirect to the user's first permitted tenant page in sidebar order, else `/tenant/select`.
