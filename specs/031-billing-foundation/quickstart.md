# Quickstart Validation: Billing Foundation

Proves the feature end-to-end. Contracts: [contracts/billing-api.md](contracts/billing-api.md); schema: [data-model.md](data-model.md).

## Prerequisites

- PostgreSQL up (repo's usual dev database) and `DATABASE_URL` exported; migrations applied through `0059_billing_foundation.sql`.
- Backend server running (`cargo run -p server` from `backend/`), frontend dev server for UI checks (`pnpm start` from `frontend/`).
- Note (repo-known): `cargo test --workspace` skips DB tests and can abort early — always run the narrow suites below with the DB up.

## 1. Migration & seed

```bash
# after running migrations
psql "$DATABASE_URL" -c "SELECT name, monthly_price_cents, token_allowance, conversation_allowance, is_default FROM billing_plans"
```

Expected: one `Free` row, price 0, 100000 / 100, `is_default = t`.

## 2. Backend suites (DB-backed, narrow)

```bash
cd backend
cargo test -p billing                              # usage math, queries, enforcement decision matrix
cargo test -p authz                                # matrix: Admin now holds billing.view; platform manage rows
cargo test -p server --test billing_overview       # tenant endpoint + role gating
cargo test -p server --test billing_platform       # plan/subscription CRUD, 403s, audit rows
cargo test -p server --test billing_responder_gate # responder gate incl. billing-paused path
cargo test -p server --test rbac                   # probe-route sweeps
cargo test -p server --test auth                   # exact /auth/login + /me permission payload
cargo test -p server --test openapi_coverage       # documented-route inventory (fails on extras too)
```

Expected: all green. The billing suite must cover: default-plan fallback, unlimited (NULL) allowances, ≥80% warning / ≥100% exceeded boundaries, past_due/canceled pausing, month-window edges, 403s for missing permissions, audit rows on every mutation.

The last three are pre-existing suites this feature *modifies* rather than adds to — `auth`, `rbac`, and `openapi_coverage` all assert exact, hand-maintained inventories and go red on any unregistered addition. Responder-gate tests live in `crates/server/tests/`, not in the `ai` crate: that crate has no DB/migration harness.

## 3. API smoke (as a signed-in Owner + platform Super Admin)

Every route is mounted under `/api/v1` (`router.rs` nests the whole API there) — the prefix is omitted only inside `#[utoipa::path]` annotations, never in a real request.

```bash
# Tenant overview (Owner/Admin session + X-Tenant-ID)
curl -s -b "$COOKIES" -H "X-Tenant-ID: $TENANT" \
  http://localhost:8080/api/v1/tenant/billing/overview | jq

# Platform: create a tight plan, assign it, verify enforcement
PLAN=$(curl -s -b "$PCOOKIES" -X POST http://localhost:8080/api/v1/platform/billing/plans \
  -H 'content-type: application/json' \
  -d '{"name":"Tiny","monthly_price_cents":900,"token_allowance":1,"conversation_allowance":1}' | jq -r .id)
curl -s -b "$PCOOKIES" -X PUT \
  http://localhost:8080/api/v1/platform/billing/tenants/$TENANT/subscription \
  -H 'content-type: application/json' -d "{\"plan_id\":\"$PLAN\",\"status\":\"active\"}"
```

Expected: overview now shows the Tiny plan; with any recorded usage both metrics read `exceeded`, `ai_paused: true`.

## 4. Enforcement end-to-end

1. With the Tiny plan assigned, send a customer message on any connected channel (or the widget).
2. Expected: message and conversation are stored and visible in the inbox; **no AI reply** is generated; overview shows `ai_paused: true` with a reason.
3. Send a message that matches one of the tenant's escalation rules while still over limit. Expected: the conversation **still escalates** and lands in the human queue — the billing gate runs after rule evaluation, so pausing AI never strands the customer (SC-006).
4. Reply as a human agent from the dashboard on the paused conversation. Expected: the reply sends normally.
5. Reassign the tenant to `Free` (or a big plan): next customer message gets an AI reply again — no restart, no job.
6. Set subscription status to `past_due`, then `canceled`: AI replies pause in both; human agent replies from the dashboard still send.

## 5. Audit trail

```bash
curl -s -b "$PCOOKIES" "http://localhost:8080/api/v1/platform/audit-logs?limit=10" | jq '.entries[].action'
```

Expected: `billing_plan.created` and `billing_subscription.changed` entries with before/after details (SC-004).

## 6. Frontend gates

```bash
cd frontend
pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check
```

UI checks:
- Owner/Admin: sidebar shows **Billing**; page shows plan card (name, price, status), period line, two usage cards with normal/warning/exceeded styling, AI-paused banner when applicable. (SC-001, FR-007)
- Manager/Agent/Viewer: no Billing nav entry; direct route navigation is blocked by the permission guard.
- Platform Super Admin/Finance: platform Billing page lists plans (create/retire) and tenant subscriptions (assign plan/status); Developer/Sales/Support platform roles see neither.

## 7. RBAC probes (test builds)

`/test/tenant/billing/view` → 200 for Owner/Admin, 403 for Manager/Agent/Viewer. `/test/platform/billing/manage` → 200 for Super Admin/Finance, 403 for Developer/Sales/Support.
