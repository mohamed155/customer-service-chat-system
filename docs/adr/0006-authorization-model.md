# ADR-006: Authorization — permission-based RBAC, policies, impersonation, API keys

- **Status:** Proposed. Becomes Accepted when the MOH-16 PR merges.
- **Date:** 2026-11-06
- **Deciders:** Platform engineering
- **Related:** [System architecture §5.2–5.3 and §6.2](../architecture/system-architecture.md#62-authorization),
  ADR-004 (multi-tenancy and RLS), ADR-005 (authentication), ADR-014 (envelope encryption)

## Context

Every request has to answer three questions:

1. **Who is calling?** A tenant user, a platform user, or a machine (API key). ADR-005 covers how
   human callers authenticate.
2. **In which tenant?** The dashboard is multi-tenant. One user can belong to several tenants,
   and platform users can reach any tenant.
3. **May they do this, to this resource?** Some rules depend only on the caller's role
   ("Viewers can't edit"). Others depend on the resource too ("Agents can only reply to
   conversations assigned to them").

What exists today (features 006, 007, 026 and later):

- The `authz` crate has a closed `Permission` enum with dot-named values (`conversations.view`,
  `members.manage`, `platform.audit.view`, …). It also defines fixed role→permission matrices for
  tenant roles (`TenantRole`: Owner, Admin, Manager, Agent, Viewer) and platform roles
  (`PlatformRole`: SuperAdmin, Developer, Support, Sales, Finance), and a `require_permission`
  route layer.
- `tenant_context_middleware` resolves `X-Tenant-ID` into a `TenantContext` that carries the
  effective `PermissionSet`.
- Platform users get a fixed set of tenant permissions **directly from their platform role** in
  any tenant they name in `X-Tenant-ID`. `POST /platform/tenants/{id}/switch` only writes an
  audit row. Nothing bounds the access by time or purpose, and nothing ties it to a reason.

The forces at play:

- Code must check **capabilities**, not role names, so roles can be renamed or split without
  touching handlers.
- Cross-tenant access by staff is the biggest insider and compromise risk. It must be explicit,
  time-boxed, justified and fully audited.
- Integrations need non-human credentials that are scoped, revocable and attributable.
- Unauthorized callers must not be able to tell a tenant that doesn't exist from one they can't
  access (enumeration).

## Decision

### 1. Permission-based RBAC

- **Permissions are the only thing code checks.** A role is a named, versioned bundle of
  permissions, and handlers never compare role names.
- **Naming convention: `resource.action`**, dot-separated and lower snake case, as already
  built: `conversations.manage`, `knowledge_base.view`, `billing.manage`. Platform permissions
  start with `platform.` (`platform.tenants.list`). The colon form (`conversations:reply`) in the
  architecture doc's examples is **superseded** by this convention.
- **The catalog is closed and compiled in.** `authz::Permission` is the single catalog, and
  adding a permission means changing the code. A test enforces that every permission is granted
  to at least one role (`every_catalog_permission_is_granted_to_at_least_one_role`).
- **Built-in roles only for v1.** The five tenant roles and five platform roles are fixed
  matrices in `authz::matrix`, and a membership row stores the role name. **Custom tenant roles**
  (tenant-defined bundles) come later. Their table will reference catalog permissions by name.
  Unknown stored role names **fail closed** with no permissions, as `tenant_context_middleware`
  already does.
- **Hierarchy is expressed in data, not in code.** Owner ⊇ Admin ⊇ Manager ⊇ Agent ⊇ Viewer
  holds because the matrices are written that way, and matrix tests assert it. There is no
  `role >= Admin` comparison anywhere.
- **Declaring permissions on routes.** Every protected route has a `require_permission(P)` route
  layer. A CI test walks the built router and fails on any tenant or platform route without one,
  unless the route is on an explicit allowlist with a comment explaining why (for example
  `GET /me`).
  Typed extractors (`RequirePermission<P>`) can be added later. The route-layer form is the
  contract for now.

### 2. Policy layer for resource-level rules

RBAC answers "may this caller do X at all?". The **policy layer** answers "may this caller do X
to *this* resource?".

- Each resource type that needs such rules has a policy module in its owning crate. That
  module exposes pure functions or a trait over `(actor, tenant context, resource)`:

  ```rust
  pub trait Policy<R> {
      fn check(&self, actor: &Actor, ctx: &TenantContext, resource: &R) -> Decision;
  }
  pub enum Decision { Allow, Deny(DenyReason) }
  ```

- Policies **can only narrow** what RBAC grants. They never grant a permission the role lacks.
  The existing tool-approval resolution (`tools::policy::resolve_builtin`, which only tightens)
  is the model to follow.
- Policies run **after** the handler loads the resource, using the tenant-scoped repository, so
  they always see a row from the caller's tenant.
- The first policies to write:
  - **Conversations:** an Agent may reply to or claim a conversation only when it's assigned to
    them or sits unclaimed in a queue they're routed to. Manager and above bypass the check.
  - **PII export:** a Viewer may not export customer contact fields.
  - **Owner protection:** only an Owner can change or remove another Owner, and the last Owner
    can't be removed.
- A policy denial returns **404** when the caller can't see the resource at all, and **403**
  when they can see it but can't act on it.
- If the rule count grows past a handful per resource, adopt an embedded policy engine
  (Cedar). The trait boundary keeps that switch local to each crate.

### 3. Platform users and impersonation sessions

Platform roles grant **platform-scope** permissions (`platform.*`) on `/platform/*` routes. They
grant **no tenant permissions on their own**. Acting inside a tenant requires an
**impersonation session**:

| Aspect | Rule |
| -- | -- |
| Start | `POST /platform/impersonation-sessions { tenant_id, reason, ticket_ref?, mode }` requires `platform.tenants.switch` and a fresh MFA step-up (ADR-005 §6). The reason is mandatory free text with at least 10 characters. |
| Modes | `read_only` (the default): the view permissions of the staff role's tenant mapping. `read_write`: the full mapping. It needs SuperAdmin or Support **and** a second approver in production. |
| Duration | Time-boxed to **60 minutes** by default and 4 hours at most. It can't be extended, only replaced by a new session with a new reason. |
| Token | The session ID goes into the access token as `imp`, which forces a token refresh at start. One active impersonation per user at a time. |
| Effective permissions | The **intersection** of the staff role's tenant mapping (`authz::staff_tenant_permissions`) and the session mode. Never more than a tenant Owner would have. `owner.assign`, `tenant.delete` and `billing.manage` are never available through impersonation. |
| Visibility | The dashboard shows a persistent banner (tenant, reason, time left, "End session"). The tenant's audit log shows the impersonation, and its Owners get a notification (027) when a session starts. |
| Audit | Every request made during the session is audited with `actor_user_id = staff user`, `on_behalf_of_tenant_id`, `impersonation_session_id`. Start and end events go to both the tenant and platform audit logs. |
| End | Ended explicitly, by expiry, by logout, or by a SuperAdmin. An ended session's `imp` is rejected on the next request. |

Non-production environments may keep today's direct staff access, gated by the existing
`is_production` flag, so test setup doesn't need impersonation sessions.

### 4. API keys (server-to-server)

API keys are a separate **principal kind** (`PrincipalKind::ApiKey`). They don't belong to any
user and never carry platform permissions.

- **Format:** `csk_live_<key_id>_<secret>` and `csk_test_…`. `key_id` is a public 12-character
  prefix. `secret` is 256 bits, base64url. The prefix makes keys recognisable to secret scanners
  (GitHub push protection) and to support staff.
- **Storage:** `api_keys (id, tenant_id, key_id, secret_hash, name, permissions[],
  created_by, created_at, last_used_at, last_used_ip, expires_at, revoked_at)`. `secret_hash` is
  `HMAC-SHA-256(server pepper, secret)`. Keys are high-entropy, so a slow hash isn't needed, and
  the pepper keeps a database leak alone from being enough to check guesses. The plaintext is
  shown **once**, at creation.
- **Scope:** a key belongs to **exactly one tenant** and carries an explicit permission subset,
  which must be a subset of its creator's permissions at creation time. `members.*`,
  `billing.manage`, `owner.assign` and `tenant.delete` can never go on a key.
- **Transport:** `Authorization: Bearer csk_…` only. Keys in cookies or query strings are
  rejected. The tenant comes from the key, so `X-Tenant-ID` is optional, and when present it must
  match the key's tenant or the request fails with `403`. CSRF checks don't apply, since no
  cookie is involved.
- **Lifecycle:** creating a key needs `integrations.manage` plus MFA step-up. Keys can be listed
  (prefix, name, scopes, last used), rotated (the old and new key overlap for up to 24 h) and
  revoked (effective immediately). `last_used_at` writes are debounced to at most once a minute.
- **Rate limits and audit:** per-key limits on top of the per-tenant ones, and audit entries
  record `actor = api_key:<key_id>`.
- **Separate from existing channel credentials.** Widget publishable keys and visitor session
  tokens (`widgets`), integration webhook intake tokens (`integrations`) and WhatsApp webhook
  signatures stay as they are. Each one authenticates a single public endpoint family and never
  yields a `TenantContext` for the dashboard API.

### 5. Tenant header validation flow

This is the order every request to a `/tenant/*` route goes through. Steps marked *(target)* are
part of this decision but not built yet. See the gap table below.

1. **Resolve the principal** (`identity::principal_middleware`, the outermost layer).
   1. Read the session cookie and validate the JWT: signature, `exp`, and *(target)* `alg = EdDSA`,
      `iss`, `aud`. Or, *(target)*, read `Authorization: Bearer csk_…` and resolve an API-key
      principal.
   2. Load the user (`deleted_at IS NULL`) and check that the session hasn't been revoked (today:
      `revoked_sessions.jti`; *(target)*: `auth_sessions.revoked_at` by `sid`, cached in Redis).
   3. In Development and Test only, fall back to the `X-Dev-User-Id` header. The header is
      ignored in Staging and Production.
   4. On success, insert `Principal` (and the claims) into the request extensions and record
      `principal.id` on the tracing span. On failure, continue without a principal.
2. **CSRF check** (`csrf_origin_middleware`). For unsafe methods with a principal, the `Origin`
   must be in `CORS_ALLOWED_ORIGINS`. *(Target, ADR-005 §5:)* reject a missing
   `Origin`/`Referer` and require the custom header. Skipped for API keys.
3. **Require authentication** (`authentication_middleware`). No principal: **401
   `unauthenticated`**.
4. **Parse the tenant header** (`tenancy::tenant_context_middleware`).
   1. `X-Tenant-ID` missing, non-ASCII, or not a valid UUID: **400 `validation_failed`**.
      *(Target)* For API-key principals the header is optional and must equal the key's tenant.
5. **Load the tenant.** Fetch it by ID with `deleted_at IS NULL`.
   - Not found: audit `access_denied` with reason `not_found`. Today that returns 403
     `unauthorized`. *(Target)* **404 `not_found`**.
6. **Resolve effective permissions by principal kind.**
   - **Tenant user:** load the active, non-deleted membership role.
     - No membership: audit `no_membership`. *(Target)* **404**, so it can't be told apart from
       step 5.
     - Unknown stored role: log an error, audit `unknown_role`, and **fail closed** (same
       response as no membership).
     - Otherwise the permissions are `tenant_role_permissions(role)`.
   - **Platform user:** today, `staff_tenant_permissions(platform_role, is_production)`.
     *(Target, production)* the request must carry an `imp` claim for an active impersonation
     session **for this tenant**. Otherwise **404**. Effective permissions are the mode-capped
     intersection from §3.
   - **Invalid platform role:** empty permission set, with a warning logged.
   - *(Target)* **API key:** the key's stored permission subset.
7. **Check tenant status.** If the tenant isn't `active` and the caller is a tenant user (or API
   key), audit `suspended` and return **403**. Today the error code is `unauthorized`, and the
   target is `tenant_suspended`. Platform users skip this check, so support can investigate
   suspended tenants (within an impersonation session, in the target state). The check runs
   only after membership is confirmed in step 6, so only members learn that a tenant is
   suspended.
8. **Build the tenant context.** Insert `TenantContext { tenant_id, tenant_status,
   principal_kind, tenant_role, permissions, request_id }` and the `PermissionSet` into the
   request extensions, and record `tenant.id` on the span. `TenantContext` is built only here.
   *(Target)* Give it a private constructor so no other code can mint one.
9. **Bind the database tenant.** *(Target, ADR-004)* Every transaction a handler opens through
   the tenant-scoped repository runs `SET LOCAL app.tenant_id = $tenant_id`. Row-level security
   policies then enforce isolation even if a query forgets its `WHERE tenant_id`.
10. **Route permission guard** (`authz::require_permission(P)`). If `P` is missing from the set,
    the result is **403 `unauthorized`** and `authz.denied_permission` is recorded on the span.
11. **Policy check** (inside the handler, §2). The handler loads the resource through the
    tenant-scoped repository and applies the resource policy, returning **404** or **403** as
    described in §2.
12. **Post-response audit.** If the response status is 403, the middleware writes an
    `access_denied` audit entry with the reason `permission_denied`.

*(Target)* Steps 5–6 read from a **membership cache** keyed `t:{tenant_id}:m:{user_id}` with a
60 s TTL. It's invalidated by `MemberRoleChanged`, `MemberRemoved` and `TenantStatusChanged`
events, so a role change takes effect within one request after the event, or within 60 s at worst.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Role checks in handlers** (`if role == Admin`) | Couples every handler to the role list, so splitting or adding a role means a codebase-wide edit. Permission checks keep roles as pure data. |
| **Roles and permissions inside the JWT** | They go stale for the token's whole lifetime and make the token grow with tenant count. Tenant choice per request (`X-Tenant-ID`) plus a cached lookup keeps them fresh. |
| **Tenant in the URL path** (`/tenants/{id}/…`) instead of a header | Equally valid. The header was already shipped in 006 and keeps route definitions tenant-agnostic. Path-based tenancy may be revisited for the public API (ADR-012). |
| **Full ABAC or policy engine from day one** (Cedar, OPA) | Too heavy for the few resource rules v1 needs. An external engine (OPA) adds a network hop to every request. The `Policy` trait keeps Cedar as a drop-in later. |
| **Implicit staff access to every tenant** (current) | Time and purpose are unbounded, and the access is invisible to the tenant. Unacceptable for production customer data. |
| **"Log in as user" impersonation** (assume the target user's identity) | Mixes up the audit trail (actions look like the customer's own) and lets staff inherit the customer's personal permissions. Impersonation acts as staff, *on behalf of the tenant*. |
| **API keys tied to a user account** | The key dies when the user leaves, and it inherits drift in the user's permissions. Tenant-owned keys with fixed scopes are more predictable. |
| **Argon2 for API-key hashes** | Keys carry 256 bits of entropy, so brute force is already infeasible, and a slow hash would add tens of milliseconds to every machine request. A peppered HMAC is enough. |

## Consequences

**Positive**

- One mechanism (`PermissionSet` + `require_permission`) covers tenant users, staff and API keys.
- Staff access to customer data becomes explicit, justified, time-boxed, visible to the tenant
  and auditable. This is what enterprise customers and SOC 2 controls expect.
- Returning 404 for "not found" and "not a member" alike removes tenant enumeration.
- Integrations get credentials that can be revoked without touching any user.

**Negative and costs**

- Support workflows get slower. Staff have to open a session with a reason before looking at a
  tenant, and production writes need a second approver.
- New tables and endpoints: `impersonation_sessions` and `api_keys`, and later `tenant_roles`.
- The membership cache adds invalidation paths that need tests, since stale permissions are a
  security bug.
- The 403 → 404 switch changes the API contract. The dashboard's tenant-switch error handling
  has to treat 404 as "no access".

## Current state and migration

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| Z1 | Production staff tenant access needs no impersonation session. `/platform/tenants/{id}/switch` only writes the `platform.tenant_switched` audit row. | `backend/crates/modules/tenancy/src/lib.rs` (`PrincipalKind::Platform` branch), `tenancy/src/routes.rs` (`switch_tenant`), `authz/src/matrix.rs` (`staff_tenant_permissions`) | **Highest priority.** Add the `impersonation_sessions` migration, the start/end endpoints, the `imp` claim, the middleware check, the dashboard banner and the tenant-visible audit. |
| Z2 | An unknown tenant and a missing membership both return **403** (`ApiError::unauthorized` maps to `FORBIDDEN`). The architecture doc says 404. | `tenancy/src/lib.rs` (`forbidden_response_with_request_id`) | Switch both cases to `ApiError::not_found` and update the frontend tenant-switch handling and contract tests. |
| Z3 | API keys don't exist. There is no `PrincipalKind::ApiKey`. | `identity/src/lib.rs` | Add the `api_keys` table, the bearer resolution in `principal_middleware`, and the management endpoints under `integrations.manage`. |
| Z4 | There's no generic policy layer. Resource rules are ad hoc in handlers. | Handlers in `conversations`, `escalations`, `tenancy::members` | Add a `Policy` trait per crate, starting with conversation assignment, PII export and owner protection. |
| Z5 | No CI check ensures every route has a `require_permission`. | `backend/crates/server/src/router.rs` | Add a router-walking test with an explicit allowlist. |
| Z6 | `TenantContext` has public fields and can be built anywhere. | `tenancy/src/lib.rs` | Make the constructor private to the tenancy middleware. |
| Z7 | There's no membership cache and no RLS (`SET LOCAL app.tenant_id`). | `tenancy/src/authorize.rs`, migrations | Membership cache: this ADR. RLS: ADR-004. |
| Z8 | The `rbac` crate is an empty placeholder. `authz` is the real implementation. | `backend/crates/modules/rbac` | Delete `rbac`, or fold custom tenant roles into it when they're built. |
| Z9 | The architecture doc's permission examples use colons. | `docs/architecture/system-architecture.md` §5.3, §6.2 | Updated in this change to the dot convention. |
