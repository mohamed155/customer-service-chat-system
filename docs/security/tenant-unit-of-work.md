# Tenant Unit of Work

How backend code reaches tenant data, and which queries deliberately do not. This is the application
half of the two-layer tenant isolation in [ADR-004](../adr/0004-multi-tenancy-rls-cells.md) (gap T1).
It was introduced by MOH-44; the Row-Level Security policies that depend on it come in steps of MOH-31.
Two groups are live. MOH-47: `customers`, `customer_channel_identifiers`, `conversations`, `messages`,
`message_attachments`, `message_citations`, `conversation_feedback`, `escalations` and
`whatsapp_message_meta`. MOH-48: the knowledge tables (`knowledge_categories`, `knowledge_items`,
`knowledge_documents`, `knowledge_item_tags`, `knowledge_chunks` with its pgvector embeddings,
`knowledge_index_state`), the AI tables (`ai_configurations`, `ai_credentials`, `ai_usage_records`,
`ai_generations`, `agent_configurations`, `agent_prompts`, `agent_prompt_versions`,
`agent_avatar_uploads`) and the tools tables (`tenant_tools`, `tenant_tool_policies`,
`tool_requests`). A query on one of them without a tenant returns no rows and writes nothing.

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

These run on the pool, not in a Unit of Work, because no single tenant applies. Every one that reads
or writes a tenant-owned table now goes through an audited function in the `xtenant` schema (see [the
audited cross-tenant path](#the-audited-cross-tenant-path)), which is what keeps it working once a
table's Row-Level Security policy is on. The tables below list each entry point with the function
that serves it, or the reason it needs none.

### Global tables (no `tenant_id`, never covered by RLS)

| Where | Tables |
| --- | --- |
| `tenancy`: `fetch_tenant`, `list_tenants`, `get_tenant_detail`, `switch_tenant`, `create_tenant`, `update_tenant` | `tenants` |
| `billing`: `list_plans`, `plan_by_id`, `create_plan`, `update_plan` | `billing_plans` |
| `identity`: `resolve_cookie_principal`, `resolve_dev_header_principal`, `logout`, `fetch_login_user` | `users`, `revoked_sessions` |
| `integrations`: `find_catalog_by_slug` | `integration_catalog` |

### Pre-tenant lookups (the tenant is found from a token or an identifier)

Served by the audited functions in the `xtenant` schema (see [the audited cross-tenant
path](#the-audited-cross-tenant-path)).

| Where | What it resolves | Function |
| --- | --- | --- |
| `integrations`: `find_connection_by_token_hash`, and the token lookup at the start of `webhook::receive_webhook` (its delivery writes run in the tenant's Unit of Work) | webhook token to connection to tenant | `xtenant.webhook_connection_by_token` |
| `integrations`: `connection_by_token_and_slug` (the WhatsApp webhooks) | webhook token and channel to connection to tenant | `xtenant.webhook_connection_by_token_and_slug` |
| `widgets`: `find_instance_by_public_id` | public widget id to instance to tenant | `xtenant.widget_instance_by_public_id` |
| `widgets`: `find_session_by_token_hash` | session token to session to tenant | `xtenant.widget_session_by_token` |
| `tenancy`: `invitations::preview_invitation`, `invitations::accept_invitation` | invitation token to invitation to tenant; the accept then runs in that tenant's Unit of Work | `xtenant.invitation_by_token` |
| `tenancy`: `routes::build_me_response` | the memberships of one user across tenants | `xtenant.user_memberships` |
| `identity`: `audit::record` | authentication events, written with no tenant | `xtenant.record_auth_event` |
| `tenancy`: `signup::signup` | none: it creates the tenant, then writes that tenant's own rows. The transaction carries the new tenant from the moment the tenant row exists (`db::set_tenant_in_tx`) | — |

### Cross-tenant jobs and platform routes

| Where | Why |
| --- | --- |
| Outbox claim, acknowledge and release (`outbox_events`): `ai::agent_responder::process_agent_responder_once`, `ai::engine::run_generation`, `escalations::events::process_escalation_outbox_once`, `knowledge::indexer::process_one_event`, `notifications::worker::process_notification_outbox_once`, `tenancy::invitations::process_invitation_deliveries_once`, `whatsapp::sender::claim_outbound_event` | A worker claims the oldest event across all tenants, then processes it. The processing itself (reading and writing tenant data) already runs in the event's Unit of Work. **Served by:** `xtenant.outbox_claim` (AI responder, escalations, notifications), `xtenant.outbox_claim_retryable` and `xtenant.outbox_reschedule` (knowledge indexer), `xtenant.outbox_ack` and `xtenant.outbox_release` (all of them), `xtenant.outbox_coalesce_customer_messages` (AI responder), `xtenant.outbox_take_whatsapp_outbound` (WhatsApp sender), `xtenant.invitation_delivery_expire_exhausted`, `_claim`, `_retry` and `_finalize` (invitation delivery) |
| `whatsapp::queries::claim_pending_attachment` | The next pending attachment across tenants. **Served by:** `xtenant.pending_attachment_next` |
| Sweeps: `integrations::retention::sweep_expired`, `tools::approval::sweep_expired`, `widgets::queries::delete_expired_sessions`, `escalations::presence::startup_sweep`, and the notification retention sweep in `server/src/main.rs` | Delete or revert rows older than a threshold in every tenant. **Served by:** `xtenant.integration_retention_sweep`, `xtenant.tool_request_expiry_sweep` (expires the requests and queues their decision events), `xtenant.widget_session_sweep`, `xtenant.available_agents`, `xtenant.notification_retention_sweep`. Each sweep writes one audit row per run with the count, and none when it changed nothing |
| `platform::queries`: `tenant_counts` | Reads `tenants` only (no `tenant_id`, never under RLS). **No function needed** |
| `platform::queries`: `active_conversation_count`, `ai_usage_totals`, `queue_health`, `provider_metrics`, `recent_provider_failures` | Platform dashboards aggregate over all tenants. **Served by:** `xtenant.platform_active_conversation_count`, `platform_ai_usage_totals`, `platform_queue_health`, `platform_provider_metrics`, `platform_recent_provider_failures` |
| `ai::routes`: `put_platform_config`, `put_platform_credential`, `delete_platform_credential`; the platform branch of `ai::resolution` | Platform-wide rows (`tenant_id IS NULL`) in `ai_configurations` and `ai_credentials`. **Served by:** `xtenant.platform_ai_config_set`, `platform_ai_credential_set`, `platform_ai_credential_delete` (each also writes the `ai_config.updated`, `ai_credential.set` and `ai_credential.deleted` audit rows), and `platform_ai_config`, `platform_ai_credential_secret`, `platform_ai_credential_hint` for the reads |
| `billing`: `list_subscriptions`, `routes::put_subscription` | A platform principal manages any tenant's subscription. **Served by:** `xtenant.platform_subscriptions_list`, `xtenant.platform_subscription_set` (applies the retired-plan rule and writes the `billing_subscription.changed` audit row for the tenant) |
| `audit::queries::list_entries` without a tenant scope | The platform-wide audit listing. **Served by:** `xtenant.platform_audit_entries` (records who asked) |

### Writes for a known tenant that still use a bare pool transaction

These are not cross-tenant: the tenant is known (from the event, the URL or the claimed row). They run
in a plain `pool.begin()` transaction, so `app.tenant_id` is not set, and a tenant policy blocks them.
MOH-47 moved the ones that write its tables onto `db::UnitOfWork::begin_for_tenant` (the AI responder,
the AI engine, the escalation consumer and the `update_contact` tool); MOH-48 needed no source change
for its tables beyond the AI resolution below. What is left is for **MOH-49** (the audit tables):

| Where | Main tenant tables touched |
| --- | --- |
| `tenancy::routes::create_tenant`, `update_tenant` | `tenants` (global) and the audit row for that tenant (`audit_logs`) |
| `billing::routes::create_plan`, `update_plan` | `billing_plans` (global) and an audit row with no tenant (`audit_logs`) |

Audit rows with no tenant are written straight into `audit_logs` by the platform plan routes and
similar platform routes (`tenancy::audit::record_in_tx` with `tenant_id = None`). The `audit_logs`
policy (MOH-49) must either accept inserts with `tenant_id IS NULL` or route them through a function.
Authentication events already use `xtenant.record_auth_event`.

## The audited cross-tenant path

Row-Level Security has no way to say "this query may see every tenant". ADR-004 section 2 forbids
the workaround of switching it off for the application, so work without a tenant, or across
tenants, goes through one narrow `SECURITY DEFINER` function per job, in the `xtenant` schema:

- **Owner.** The functions are owned by `app_platform` (no login, `BYPASSRLS`), not by `app_owner`.
  `FORCE ROW LEVEL SECURITY` applies policies to the table owner too and `app_owner` is
  `NOBYPASSRLS`, so a function it owned would return nothing as soon as a table's policy is on.
  `app_owner` can switch to `app_platform` (`SET ROLE`, no inheritance) to create the functions in a
  migration; `app_runtime` cannot.
- **Calling them.** `app_runtime` is the only role that may execute them
  (`SELECT … FROM xtenant.widget_session_by_token($1)`); `PUBLIC` may not. It still cannot read the
  underlying tables without a tenant.
- **Shape.** Each function sets `search_path = pg_catalog, public, pg_temp`, takes only the arguments
  it needs and returns only the columns its caller uses.
- **Audit.** A call that returns or changes something writes an `audit_logs` row through the
  internal helper `xtenant._audit` (action `xtenant.<function>`, resource type
  `cross_tenant_access`, the tenant and the resource id of what was resolved; no tokens or
  hashes). A call that finds nothing writes nothing. The lookups sit behind endpoints that a client
  can repeat (a widget's public id is printed on every page that embeds it), and `audit_logs` is
  append-only, so a lookup writes at most one row per function and resource per hour. The number of
  rows is bounded by the number of real resources, not by the number of requests. The outbox queue
  functions (`outbox_*`, `invitation_delivery_*`) write at most one row per function and event type
  per minute, because the outbox already holds one row per event. The helper is not
  executable by `app_runtime`, so the application cannot forge these rows.
- **Adding one.** Follow `backend/migrations/0068_cross_tenant_lookups.sql`. The catalog test in
  `backend/crates/shared/db/tests/xtenant.rs` checks owner, `search_path` and privileges for every
  function in the schema, and each function needs a behaviour test there. The test helpers in
  `db::testing` (`owner_tx`, `enforce_rls`, `become_runtime`) put forced Row-Level Security on the
  tables in a transaction that is rolled back, drop to `app_runtime`, and check that the function
  still works while a direct read returns nothing.

## The tenant-isolation harness

ADR-004 section 7 requires every tenant table to be proven against the wrong tenant. The harness is
the shared way to do it; **a table is not under Row-Level Security until it is in the harness**.

- **Database level, per table** (`backend/crates/shared/db/tests/tenant_isolation.rs`, 26 tables, built on
  `db::testing::isolation::assert_isolated`). For each table it creates a row in each of two tenants
  and, as `app_runtime`, checks: the owning tenant sees its row; the wrong tenant cannot read it by
  a guessed id, list it, update it or delete it, and cannot move its own row to the other tenant
  (the `WITH CHECK`); with no tenant at all nothing is visible, nothing is updated or deleted and an
  insert is refused with the row-level-policy error (SQLSTATE 42501, so a typo in a fixture cannot
  pass for a denial); and the rows are untouched afterwards. Tables keyed on the knowledge item
  (`knowledge_documents`, `knowledge_item_tags`, `knowledge_index_state`) are looked up by `item_id`.
- **Self-enforcing.** The last test in that file compares its table list with the tables that have
  forced Row-Level Security (`relforcerowsecurity`) and fails if they differ in either direction. A
  migration that forces RLS on a new table fails the build until the table has an entry (a seed for
  one row, an insert statement and the ids it needs).
- **Repository level** (`backend/crates/server/tests/tenant_isolation_repositories.rs`). Repository
  functions are called with tenant B and tenant A's real ids and must find nothing; the same call for
  tenant A finds it (every denial has an owning-tenant control, or it could pass on an empty fixture).
  It covers conversations and messages, customers, feedback, escalations, RAG retrieval
  (`knowledge::retrieval::search`, with identical content in both tenants), the indexing state and AI
  configuration resolution. Add the repository functions of a new module here.
- **Endpoint level.** Each feature suite keeps its wrong-tenant endpoint tests (404 for another
  tenant's id, no rows in lists): `customers.rs`, `conversations.rs`, `escalations.rs`,
  `feedback_api.rs`, `rag_citations.rs` and `engine_isolation.rs` in `backend/crates/server/tests/`
  for the conversations and customers group, and `rag_isolation.rs` (identical content in two tenants),
  `engine_tool_isolation.rs` and `tenant_tool_credential_confidentiality.rs` for knowledge, AI and tools.
  A new endpoint on a tenant table needs one.
- **Fixtures.** `db::testing::seed_pool()` is a pool on the owner connection that runs `SET ROLE
  app_platform` (BYPASSRLS), for creating and reading back rows for any tenant in test setup and
  assertions. **Never give it to the application** (state, router, worker or the repository under
  test): code that runs on it bypasses the policies and hides the bugs they exist to catch. Use
  `db::testing::owner_pool()` for `TRUNCATE`, and `db::testing::migrate()` for migrations.

Adding a table group:

1. Write the migration (copy `0072_rls_conversations_customers.sql`).
2. Add an `Entry` per table to `entries()` in `tenant_isolation.rs` (and seed its parents in `seed_graph`).
3. Run `REQUIRE_DB_TESTS=1 cargo nextest run --workspace` as `app_runtime`: the failures are fixtures that
   seed through the runtime pool (move them to `seed_pool()`) and source queries that run without a tenant
   (move them to a Unit of Work, or to a function in the `xtenant` schema if they span tenants).

## Notes for the Row-Level Security steps

- `ai_configurations` and `ai_credentials` hold platform default rows with `tenant_id IS NULL`. The
  policy is the same strict one as everywhere else, so **tenant queries cannot see them, on purpose**:
  a bug in tenant-scoped code must not be able to read the platform's API keys. `ai::resolution`
  reads the tenant's own row in its Unit of Work and falls back to the platform default through
  `xtenant.platform_ai_config`, `platform_ai_credential_secret` and `platform_ai_credential_hint`; the
  platform routes write them through the `platform_ai_*_set` and `_delete` functions. A test
  (`platform_ai_rows_are_invisible_to_tenant_queries`) keeps it that way.
- `audit_logs` has platform rows (`tenant_id IS NULL`) as well. MOH-49 decides: accept inserts with
  `tenant_id IS NULL` or route them through a function; the platform-wide listing already goes through
  `xtenant.platform_audit_entries`.
- `outbox_events` is claimed across tenants by the functions in `xtenant`; completing or deleting an
  event for a known tenant can use a Unit of Work.
- `tenant_memberships` and `tenant_invitations` are read before a tenant is known (sign-in,
  invitation accept); those reads belong to the pre-tenant table above.
