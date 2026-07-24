# API Contracts: Billing Foundation

All endpoints follow existing conventions: session-cookie auth, tenant context via `X-Tenant-ID` where tenant-scoped, `require_permission` layers in `server/src/router.rs`, utoipa annotations feeding the OpenAPI doc, standard `ApiError` envelope, snake_case wire fields.

## Tenant

### GET /tenant/billing/overview

Permission: `billing.view` (Owner, Admin; platform Finance/Super Admin in staff production access).

Response 200 `BillingOverviewDto`:

```json
{
  "plan": {
    "id": "uuid",
    "name": "Free",
    "monthly_price_cents": 0,
    "token_allowance": 100000,
    "conversation_allowance": 100,
    "is_default": true,
    "status": "active"
  },
  "subscription_status": "active",
  "period": { "start": "2026-07-01T00:00:00Z", "end": "2026-08-01T00:00:00Z" },
  "usage": {
    "tokens":        { "used": 84200, "allowance": 100000, "percent": 84, "state": "warning" },
    "conversations": { "used": 31,    "allowance": 100,    "percent": 31, "state": "normal" }
  },
  "ai_paused": false,
  "ai_paused_reason": null
}
```

Semantics:
- `allowance`/`percent` are `null` for unlimited metrics; `state` is then always `"normal"`.
- `state` ∈ `normal | warning | exceeded` (warning at ≥ 80%, exceeded at ≥ 100%).
- `ai_paused_reason` ∈ `token_limit | conversation_limit | subscription_status | null`.
- Tenants without a subscription row report the default plan and `subscription_status: "active"`.
- Errors: 401 unauthenticated, 403 missing permission (both via existing middleware).

## Platform

Reads and writes are separately authorized (FR-011): the two GETs below require only `platform.billing.view`, the three mutations require `platform.billing.manage`. A caller holding view alone receives full read access and 403 on every mutation — the UI's hiding of write controls is a convenience, never the enforcement boundary.

### GET /platform/billing/plans — `platform.billing.view`

200: `{ "plans": [PlanDto, …] }` (all plans, active and retired, ordered by created_at).

### POST /platform/billing/plans — `platform.billing.manage`

Request: `{ "name": "Starter", "monthly_price_cents": 4900, "token_allowance": 500000, "conversation_allowance": 500 }`
(`token_allowance` / `conversation_allowance` omitted or null ⇒ unlimited.)

201: created `PlanDto`. Errors: 409 duplicate name, 422 validation (empty name, negative values). Audit: `billing_plan.created`.

### PATCH /platform/billing/plans/{plan_id} — `platform.billing.manage`

Request: `{ "status": "retired" }` (or `"active"`). Only `status` is mutable.

200: updated `PlanDto`. Errors: 404 unknown plan, 422 invalid status, 409 attempting to retire the default plan. Audit: `billing_plan.status_changed`.

### GET /platform/billing/subscriptions — `platform.billing.view`

200: `{ "subscriptions": [ { "tenant_id": "uuid", "tenant_name": "Acme", "plan_id": "uuid", "plan_name": "Free", "plan_is_default": true, "status": "active", "explicit": false }, … ] }`

One row per tenant (LEFT JOIN — tenants without a subscription row show the default plan with `"explicit": false`).

### PUT /platform/billing/tenants/{tenant_id}/subscription — `platform.billing.manage`

Idempotent upsert. Request: `{ "plan_id": "uuid", "status": "active" }` (status ∈ active | past_due | canceled).

200: `{ "tenant_id": "uuid", "plan_id": "uuid", "status": "active" }`. Errors: 404 unknown tenant or plan, 422 invalid status, 409 assigning a retired plan (unless the tenant is already on it and only status changes). Audit: `billing_subscription.changed` with before/after.

## RBAC probe routes (test builds only, already exist unless noted)

- `/test/tenant/billing/view`, `/test/tenant/billing/manage` — existing.
- `/test/platform/billing/view` — existing.
- `/test/platform/billing/manage` — **new**, guarded by `platform.billing.manage`.

## Internal contract (crate boundary, not HTTP)

`billing::enforcement::check(pool, tenant_id) -> Result<EnforcementDecision, sqlx::Error>` where `EnforcementDecision { allowed: bool, reason: Option<PauseReason> }`. Called by `ai::agent_responder` as the last check before generation — after the channel, conversation-state, open-escalation gates *and* escalation-rule evaluation, so auto-escalation still runs for a paused tenant. A paused decision consumes the outbox event without generating (mirrors the existing escalation gate behavior).
