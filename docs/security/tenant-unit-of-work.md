# Tenant Unit of Work

How backend code reaches tenant data, and which queries deliberately do not. This is the application
half of the two-layer tenant isolation in [ADR-004](../adr/0004-multi-tenancy-rls-cells.md) (gap T1).
It was introduced by MOH-44; the Row-Level Security policies that depend on it come in later steps of
MOH-31.

## What it is

`db::UnitOfWork` (`backend/crates/shared/db/src/tenant.rs`) is a database transaction that has
already run `SELECT set_config('app.tenant_id', $1, true)` on its own connection. The third argument
`true` makes the setting local to the transaction, so it disappears at commit or rollback and can
never reach the next user of a pooled connection (it is also safe behind PgBouncer in transaction
mode). The `app.tenant_id` setting is what the RLS policy compares `tenant_id` with.

```rust
// In a handler: the tenant comes from the authorised TenantContext.
let mut uow = match ctx.unit_of_work(&pool).await {
    Ok(uow) => uow,
    Err(response) => return response,
};
let rows = sqlx::query("SELECT ...").bind(ctx.tenant_id).fetch_all(uow.conn()).await?;
if let Err(response) = ctx.commit(uow).await { return response; }

// In a repository or job that only has a tenant id (a job payload, an authenticated session row):
let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
```

## Rules

- **Tenant data is read and written through a Unit of Work.** Never run a query on a tenant-owned
  table straight on the pool, and never set `app.tenant_id` yourself or with a session-level `SET`.
- **A Unit of Work that is dropped is rolled back.** Code that writes must call `commit()` (or
  `TenantContext::commit`) on the success path. Read-only code may simply drop it.
- **Pass the tenant, do not look it up.** Repository functions take a `tenant_id` and use it for both
  the Unit of Work and the `WHERE tenant_id = $1` filter; the two layers stay independent.
- **Keep it short.** A Unit of Work holds a pooled connection. Do not keep one open across calls to
  other services or while waiting on a user.

## Queries that do not use it

These run directly on the pool because no single tenant applies. They are the input for MOH-46 (the
audited cross-tenant path, one narrow `SECURITY DEFINER` function per job) and for the table groups
in MOH-47, MOH-48 and MOH-49, which must give each of them a deliberate rule before RLS is switched
on for the tables they touch.

### Global tables (no `tenant_id`, never covered by RLS)

| Where | Tables |
| --- | --- |
| `tenancy`: `fetch_tenant`, `list_tenants`, `get_tenant_detail`, `switch_tenant`, `create_tenant`, `update_tenant` | `tenants` |
| `billing`: `list_plans`, `plan_by_id`, `create_plan`, `update_plan` | `billing_plans` |
| `identity`: `resolve_cookie_principal`, `resolve_dev_header_principal`, `logout`, `fetch_login_user` | `users`, `revoked_sessions` |
| `integrations`: `find_catalog_by_slug` | `integration_catalog` |

### Pre-tenant lookups (the tenant is found from a token or an identifier)

| Where | What it resolves |
| --- | --- |
| `integrations`: `find_connection_by_token_hash`, `connection_by_token_and_slug`, and the token lookup at the start of `webhook::receive_webhook` (its delivery writes run in the tenant's Unit of Work) | webhook token to connection to tenant |
| `widgets`: `find_instance_by_public_id`, `find_session_by_token_hash` | public widget id or session token to tenant |
| `tenancy`: `invitations::preview_invitation`, `invitations::accept_invitation` | invitation token to tenant |
| `tenancy`: `signup::signup`, `routes::build_me_response` | creates a user, tenant and membership; lists one user's memberships across tenants |
| `identity`: `audit::record` | authentication events, written with no tenant |

### Cross-tenant jobs and platform routes

| Where | Why |
| --- | --- |
| Outbox claim and acknowledge (`outbox_events`): `ai::agent_responder::process_agent_responder_once`, `ai::engine::run_generation`, `escalations::events::process_escalation_outbox_once`, `knowledge::indexer::process_one_event`, `notifications::worker::process_notification_outbox_once`, `tenancy::invitations::process_invitation_deliveries_once`, `whatsapp::sender::claim_outbound_event` | A worker claims the oldest event across all tenants, then processes it. The processing itself (reading and writing tenant data) already runs in the event's Unit of Work |
| `whatsapp::queries::claim_pending_attachment` | Claims the next pending attachment across tenants |
| Sweeps: `integrations::retention::sweep_expired`, `tools::approval::sweep_expired`, `widgets::queries::delete_expired_sessions`, `escalations::presence::startup_sweep`, and the notification retention sweep in `server/src/main.rs` | Delete or revert rows older than a threshold in every tenant |
| `platform::queries` (six functions) | Platform dashboards aggregate over all tenants |
| `ai::routes`: `put_platform_config`, `put_platform_credential`, `delete_platform_credential`; the platform branch of `ai::resolution` | Platform-wide rows (`tenant_id IS NULL`) in `ai_configurations` and `ai_credentials` |
| `billing`: `list_subscriptions`, `routes::put_subscription` | A platform principal manages any tenant's subscription |
| `audit::queries::list_entries` without a tenant scope | The platform-wide audit listing |

## Notes for the Row-Level Security steps

- `ai_configurations` and `ai_credentials` hold platform default rows with `tenant_id IS NULL`, and the
  tenant branch of `ai::resolution` deliberately reads `tenant_id = $1 OR tenant_id IS NULL`. A plain
  `tenant_id = current_setting(...)` policy would hide the defaults, so these two tables need a policy
  that lets every tenant read the platform rows while only the owning tenant (or the audited platform
  path) can write them.
- `audit_logs` has platform rows (`tenant_id IS NULL`) as well. Treat it the same way.
- `outbox_events` is claimed across tenants. The claim has to go through the audited path; completing
  or deleting an event for a known tenant can use a Unit of Work.
- `tenant_memberships` and `tenant_invitations` are read before a tenant is known (sign-in,
  invitation accept); those reads belong to the pre-tenant table above.
