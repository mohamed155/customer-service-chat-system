# Data Model: Billing Foundation

**Feature**: 031-billing-foundation | **Migration**: `backend/migrations/0059_billing_foundation.sql`

## New Tables

### billing_plans (platform-global catalog — deliberately no `tenant_id`)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | UUID | PK, `DEFAULT gen_random_uuid()` | |
| name | TEXT | NOT NULL, UNIQUE | Display name ("Free", "Starter", …) |
| monthly_price_cents | INTEGER | NOT NULL, CHECK `>= 0` | Display-only; nothing is charged (USD assumed) |
| token_allowance | BIGINT | NULL, CHECK `>= 0` | Monthly AI tokens (input+output). **NULL = unlimited** |
| conversation_allowance | INTEGER | NULL, CHECK `>= 0` | Monthly conversations started. **NULL = unlimited** |
| is_default | BOOLEAN | NOT NULL DEFAULT false | Fallback plan for tenants without a subscription row |
| status | TEXT | NOT NULL DEFAULT 'active', CHECK IN ('active','retired') | Retired plans keep working for current subscribers but cannot be newly assigned |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL DEFAULT now() | Repo conventions (005) |

Indexes: `UNIQUE (name)`; partial unique `billing_plans_one_default_idx ON billing_plans (is_default) WHERE is_default` (at most one default).

Seed (in the same migration): the default Free plan — name `Free`, price 0, `token_allowance = 100000`, `conversation_allowance = 100`, `is_default = true`, status active.

### tenant_subscriptions (tenant-owned)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| id | UUID | PK, `DEFAULT gen_random_uuid()` | |
| tenant_id | UUID | NOT NULL, **UNIQUE**, FK → tenants(id) ON DELETE RESTRICT | One subscription per tenant (FR-002) |
| plan_id | UUID | NOT NULL, FK → billing_plans(id) ON DELETE RESTRICT | Assigning a retired plan is rejected app-side; existing FKs to retired plans stay valid |
| status | TEXT | NOT NULL DEFAULT 'active', CHECK IN ('active','past_due','canceled') | `past_due` and `canceled` both pause AI (clarified) |
| assigned_by | UUID | NULL, FK → users(id) | Platform actor of the latest change |
| created_at / updated_at | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

The unique index on `tenant_id` doubles as the lookup index.

## Derived Data (no new tables)

| Derived value | Source | Computation |
|---------------|--------|-------------|
| Effective plan for a tenant | `tenant_subscriptions` ⟕ `billing_plans` | Tenant's subscription row if present, else the `is_default` plan; effective status defaults to `active` when no row exists |
| Billing period | clock | `[date_trunc('month', now() AT TIME ZONE 'utc'), + 1 month)` — computed per query, never stored |
| Token usage (period) | `ai_usage_records` | `SUM(COALESCE(input_tokens,0) + COALESCE(output_tokens,0))` for `tenant_id` within period (all statuses) |
| Conversation usage (period) | `conversations` | `COUNT(*)` for `tenant_id` with `created_at` within period (excluding soft-deleted rows if the column exists on that table) |
| Metric state | usage vs allowance | `normal` < 80% ≤ `warning` < 100% ≤ `exceeded`; allowance NULL ⇒ always `normal`, no percent; allowance `0` ⇒ always `exceeded` (a deliberate "AI disabled for this metric" plan — `0` is accepted on create, only negatives are rejected) |
| AI paused + reason | all of the above | Paused iff status ∈ {past_due, canceled} (reason `subscription_status`) OR token usage ≥ allowance (`token_limit`) OR conversation usage ≥ allowance (`conversation_limit`); first matching reason in that order |

## State Transitions

- **Plan**: `active ⇄ retired` (PATCH; both directions allowed, audited). No delete.
- **Subscription**: `active ⇄ past_due ⇄ canceled` — any status may be set directly by a platform manage action (upsert). Row creation happens on first assignment; before that the tenant is implicitly on the default plan with status `active`.
- **Enforcement**: purely derived — changes to plan, status, usage, or the month boundary take effect on the next AI-reply gate evaluation and the next overview read (FR-006 needs no job/worker). The gate is the *last* check before generation in `ai::agent_responder`, i.e. after escalation-rule evaluation, so a paused tenant still auto-escalates to a human queue.

## Audit Records (existing `audit_logs` table, append-only)

| Action | resource_type / resource_id | details JSON |
|--------|------------------------------|--------------|
| `billing_plan.created` | `billing_plan` / plan id | full created attributes |
| `billing_plan.status_changed` | `billing_plan` / plan id | `{from, to}` |
| `billing_subscription.changed` | `billing_subscription` / subscription id, `tenant_id` set | `{plan_id: {from, to}, status: {from, to}}` (nulls for first assignment) |

Written via `tenancy::audit::record_in_tx` inside the mutation transaction (pattern verified in `ai::audit`).

## Module Ownership & Dependencies

- The `billing` crate owns both new tables and all queries on them; it reads `ai_usage_records`, `conversations`, `tenants` via SQL only (no crate dependency on `ai`/`conversations`).
- `ai` gains a dependency on `billing` for the enforcement gate (`billing::enforcement::check(pool, tenant_id) -> EnforcementDecision`). No cycle: `billing` never depends on `ai`.
