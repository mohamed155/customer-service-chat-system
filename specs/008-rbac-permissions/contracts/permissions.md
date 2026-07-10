# Contract: Permission Catalog & Role Matrix

**Feature**: 008-rbac-permissions — canonical, single source of truth (FR-001/FR-002). The backend `authz::matrix` implements exactly this document; a unit test asserts the implemented catalog matches this list.

## Permission catalog

### Tenant scope

| Code | Grants |
|------|--------|
| `overview.view` | View the tenant overview page |
| `conversations.view` | View conversations |
| `conversations.manage` | Reply, assign, close, escalate conversations |
| `customers.view` | View customers |
| `customers.manage` | Edit customer records |
| `ai_agent.view` | View AI agent configuration |
| `ai_agent.manage` | Change AI agent configuration |
| `knowledge_base.view` | View knowledge base |
| `knowledge_base.manage` | Create/edit/delete knowledge base content |
| `integrations.view` | View integrations |
| `integrations.manage` | Install/configure/remove integrations |
| `analytics.view` | View analytics |
| `members.view` | View tenant members |
| `members.manage` | Invite/remove members, change non-Owner roles |
| `settings.view` | View workspace settings |
| `settings.manage` | Change workspace settings |
| `billing.view` | View billing information |
| `billing.manage` | Change billing/payment configuration |
| `tenant.delete` | Delete the tenant |
| `owner.assign` | Assign or transfer the Owner role |

### Platform scope

| Code | Grants |
|------|--------|
| `platform.tenants.list` | List/search all tenants (`GET /platform/tenants`) |
| `platform.tenants.switch` | Switch into a tenant context (`POST /platform/tenants/{id}/switch`) |
| `platform.admin` | Platform administration (reserved; guards the platform dashboard area) |
| `platform.billing.view` | View platform-level billing data |
| `platform.diagnostics.view` | View platform diagnostics/technical data |

## Tenant role → permission matrix

✅ = granted. Per FR-002a/FR-002b invariants: Owner ⊇ Admin ⊇ Manager; Owner − Admin = billing/tenant.delete/owner.assign; Manager has no settings/billing.

| Permission | Owner | Admin | Manager | Support Agent | Viewer |
|------------|:-----:|:-----:|:-------:|:-------------:|:------:|
| overview.view | ✅ | ✅ | ✅ | ✅ | ✅ |
| conversations.view | ✅ | ✅ | ✅ | ✅ | ✅ |
| conversations.manage | ✅ | ✅ | ✅ | ✅ | — |
| customers.view | ✅ | ✅ | ✅ | ✅ | ✅ |
| customers.manage | ✅ | ✅ | ✅ | ✅ | — |
| ai_agent.view | ✅ | ✅ | ✅ | — | ✅ |
| ai_agent.manage | ✅ | ✅ | ✅ | — | — |
| knowledge_base.view | ✅ | ✅ | ✅ | ✅ | ✅ |
| knowledge_base.manage | ✅ | ✅ | ✅ | — | — |
| integrations.view | ✅ | ✅ | ✅ | — | ✅ |
| integrations.manage | ✅ | ✅ | ✅ | — | — |
| analytics.view | ✅ | ✅ | ✅ | — | ✅ |
| members.view | ✅ | ✅ | ✅ | — | — |
| members.manage | ✅ | ✅ | ✅ | — | — |
| settings.view | ✅ | ✅ | — | — | — |
| settings.manage | ✅ | ✅ | — | — | — |
| billing.view | ✅ | — | — | — | — |
| billing.manage | ✅ | — | — | — | — |
| tenant.delete | ✅ | — | — | — | — |
| owner.assign | ✅ | — | — | — | — |

## Platform role → platform permissions

| Permission | Super Admin | Developer | Support Engineer | Sales | Finance |
|------------|:-----------:|:---------:|:----------------:|:-----:|:-------:|
| platform.tenants.list | ✅ | ✅ | ✅ | ✅ | ✅ |
| platform.tenants.switch | ✅ | ✅ | ✅ | ✅ | ✅ |
| platform.admin | ✅ | — | — | — | — |
| platform.billing.view | ✅ | — | — | — | ✅ |
| platform.diagnostics.view | ✅ | ✅ | — | — | — |

## Platform staff inside a tenant (FR-005a)

**Non-production (dev / qa / stg — any `Environment` other than `Production`)**: every platform role receives the **full tenant permission set** (all 20 tenant-scope permissions).

**Production**:

| Permission | Super Admin | Developer | Support Engineer | Sales | Finance |
|------------|:-----------:|:---------:|:----------------:|:-----:|:-------:|
| overview.view | ✅ | ✅ | ✅ | ✅ | ✅ |
| conversations.view | ✅ | ✅ | ✅ | — | — |
| conversations.manage | ✅ | — | ✅ | — | — |
| customers.view | ✅ | ✅ | ✅ | — | — |
| customers.manage | ✅ | — | ✅ | — | — |
| ai_agent.view | ✅ | ✅ | — | — | — |
| ai_agent.manage | ✅ | — | — | — | — |
| knowledge_base.view | ✅ | ✅ | ✅ | — | — |
| knowledge_base.manage | ✅ | — | — | — | — |
| integrations.view | ✅ | ✅ | — | — | — |
| integrations.manage | ✅ | — | — | — | — |
| analytics.view | ✅ | ✅ | — | ✅ | ✅ |
| members.view | ✅ | ✅ | — | ✅ | ✅ |
| members.manage | ✅ | — | — | — | — |
| settings.view | ✅ | ✅ | — | ✅ | ✅ |
| settings.manage | ✅ | — | — | — | — |
| billing.view | ✅ | — | — | — | ✅ |
| billing.manage | ✅ | — | — | — | — |
| tenant.delete | ✅ | — | — | — | — |
| owner.assign | ✅ | — | — | — | — |

Notes:

- Support Engineer: works support areas (conversations, customers, knowledge base) — no settings changes (spec US3).
- Developer: read-only/diagnostic — every `.view`, no `.manage`.
- Sales & Finance: read-only account-level info (overview, analytics, members list, settings view; Finance additionally billing view).
- Tenant-user permissions are identical in every environment; only staff-in-tenant varies.

## Page → permission mapping (frontend route/nav gating)

| Dashboard page | Required permission |
|----------------|---------------------|
| Overview | `overview.view` |
| Conversations | `conversations.view` |
| Customers | `customers.view` |
| AI Agent | `ai_agent.view` |
| Knowledge Base | `knowledge_base.view` |
| Integrations | `integrations.view` |
| Analytics | `analytics.view` |
| Settings | `settings.view` |
| Platform area | `platform.admin` |
