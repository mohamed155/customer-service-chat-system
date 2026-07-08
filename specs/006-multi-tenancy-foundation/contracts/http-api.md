# Contract: Multi-Tenancy HTTP API

**Feature**: 006-multi-tenancy-foundation
**Consumers**: dashboard frontend, all future tenant-scoped backend features, integration tests.
**Alignment**: subset of `specs/001-ai-customer-service-platform/contracts/rest-api.md`; errors use the kernel envelope (`{ error: { code, message, details, request_id } }`) from spec 004. DTO shapes live in [data-model.md](../data-model.md).

## A. Headers

| Header | Direction | Contract |
|--------|-----------|----------|
| `X-Tenant-ID` | request | UUID of the working tenant. REQUIRED on every tenant-scoped endpoint; ignored by platform-scoped endpoints (FR-004). The only way tenant context enters the system — there is no server-side active-tenant state. |
| `X-Dev-User-Id` | request | Dev/test-only principal source (FR-019): UUID of an existing user. Honored only when the server environment is `development` or `test`; ignored (treated as absent) in `staging`/`production`. Replaced wholesale by the real auth feature. |
| `X-Request-Id` | both | unchanged from spec 004; all error envelopes carry it. |

CORS: `X-Tenant-ID` is always in `allow_headers`; `X-Dev-User-Id` only in dev/test environments.

## B. Error semantics (normative for all tenant-scoped endpoints)

| Condition | Status | code | Notes |
|-----------|--------|------|-------|
| No principal (missing/invalid identity) | 401 | `unauthenticated` | |
| Missing `X-Tenant-ID` on tenant-scoped route | 400 | `validation_failed` | message names the missing header |
| Malformed (non-UUID) `X-Tenant-ID` | 400 | `validation_failed` | |
| Tenant does not exist / is deleted | 403 | `unauthorized` | **byte-identical body** to the no-membership case — tenant existence is not probeable (FR-002) |
| Tenant user without active membership | 403 | `unauthorized` | audited as `tenant.access_denied` (FR-013) |
| Tenant user, tenant suspended | 403 | `unauthorized` | suspension-appropriate message (only members see it); audited (FR-007/FR-013) |
| Non-platform user calling `/platform/*` | 403 | `unauthorized` | |

Denial responses never include tenant data, names, or status beyond the cases above.

## C. Endpoints

### `GET /api/v1/me` — current principal

- Identity required; tenant-context-free (ignores `X-Tenant-ID`).
- 200 → `MeResponse` (profile, `platformRole`, active `memberships`).
- Purpose: frontend bootstrap — switcher visibility (platform vs tenant user), tenant-user default tenant resolution (FR-015).

### `GET /api/v1/platform/tenants` — tenant directory (switcher)

- Platform principals only (403 otherwise). Tenant-context-free.
- Query: cursor pagination per kernel `Page` conventions; optional `q` (matches name/slug).
- 200 → `Page<TenantSummary>`; includes suspended tenants (platform staff must reach them), never deleted ones.
- Note: 001 assigns this a per-platform-role grant (`P:sales+`); until the RBAC feature, the gate is "any platform role" — recorded divergence, tightened later without contract change.

### `POST /api/v1/platform/tenants/{id}/switch` — explicit tenant switch (FR-012)

- Platform principals only. Tenant-context-free (the *target* is the path id).
- Behavior: validates target exists and is not deleted (suspended allowed) → writes `platform.tenant_switched` audit row synchronously → 200 `TenantSummary`.
- Stateless: the server records the audit fact and returns; the **client** carries the new `X-Tenant-ID` thereafter (spec clarification). There is no server-side "exit switcher" — dropping the selection is purely client-side (001's `DELETE /platform/switch` is superseded by the stateless model for this feature).
- Errors: nonexistent/deleted target → 403 `unauthorized` (anti-enumeration holds here too).

### `GET /api/v1/tenant` — own tenant profile (first tenant-scoped endpoint)

- Runs under the tenant-context middleware: identity + `X-Tenant-ID` + authorization required.
- 200 → `TenantSummary` of the active tenant, read via `TenantContext.tenant_id` only.
- Doubles as the isolation matrix's probe target (FR-018) and the template every future tenant-scoped endpoint follows.

## D. Middleware contract (for future backend features)

- Mount tenant-scoped routes inside the tenant-context middleware; platform-scoped routes outside it.
- Handlers MUST take the tenant id from the `TenantContext` request extension — never from headers, path, or body (FR-003/FR-008).
- Data access for tenant-owned tables MUST filter by `TenantContext.tenant_id`.
- Authorization state is never cached across requests (FR-009).
- New sensitive tenant operations MUST write audit rows via the tenancy module's audit helper.

## E. Frontend contract

- `X-Tenant-ID` is attached exclusively by the tenant-context interceptor from the `tenantContext` store slice (FR-014); features never set tenant headers.
- Platform-scoped paths (`/me`, `/platform/*`) are excluded from tenant-header attachment.
- The switcher is rendered only when the current principal has a platform role (FR-015); tenant users' active tenant auto-resolves from `/me` memberships.
- A 403 `unauthorized` on a tenant-scoped call surfaces the "no access to this tenant" state and, if caused by the persisted selection, clears it (FR-016/FR-017).
