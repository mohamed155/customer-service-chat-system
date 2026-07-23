# Quickstart: Integrations Foundation (028) — validation guide

Proves the feature end-to-end. Contracts: [contracts/integrations-api.md](contracts/integrations-api.md); data model: [data-model.md](data-model.md); design rationale: [research.md](research.md).

## Prerequisites

- PostgreSQL up with migrations applied through `0056_integrations_foundation.sql` (`cd backend && sqlx migrate run` or the project's usual migration flow).
- The `APP_INTEGRATION_SECRETS_KEY` env var set to a base64-encoded 32-byte key (same format as `APP_AI_KEY_ENCRYPTION_KEY`). Missing/short keys will fail config validation at server start.
- Backend: `cd backend && cargo run -p server` (API at `http://localhost:<port>/api/v1`). The integration retention sweeper starts with the server — confirm `integration retention sweep: started` in the logs.
- Frontend: `cd frontend && pnpm ng serve dashboard`.
- Seeded users in one tenant: an Owner/Admin, a Manager, a Viewer, and an Agent (for the RBAC test). A second tenant with its own admin for the isolation check. Dev identity flow per 006/007 (login sets `app_session` cookie; tenant calls need `X-Tenant-ID`).

## Automated checks (all must pass)

### Backend (unit + integration w/ PostgreSQL)

```bash
# Start services (Docker required):
docker compose -f infra/docker-compose.yml up -d postgres redis

# Run migrations:
cd backend && DATABASE_URL="postgres://customer_service:customer_service_dev@localhost:5432/customer_service" sqlx migrate run

# Run unit tests for the integrations crate:
cargo test -p integrations --lib

# Run the audit module's unit tests (CATEGORY_PREFIXES extension):
cargo test -p audit --lib

# Run the US1 integration tests (catalog, RBAC, isolation):
cargo test -p server --test integrations_catalog --test integrations_rbac --test integrations_isolation

# After US2 lands, run the lifecycle + secret-confidentiality suites:
cargo test -p server --test integrations_lifecycle --test integrations_secret_confidentiality

# After US3 lands, run the webhook + events suites:
cargo test -p server --test integrations_webhook --test integrations_events
```

### Frontend

```bash
# Typecheck and build:
cd frontend && pnpm ng build dashboard

# Unit tests (store specs + updated component spec):
pnpm ng test dashboard --watch=false --filter='Integrations'

# Full lint / format:
pnpm lint
pnpm format:check
```

## End-to-end manual flow (US1)

1. Sign in as the tenant Admin. Navigate to **Integrations**. The four seeded entries (`generic-webhook` available; `slack`, `microsoft-teams`, `crm` "coming soon") should appear with `Not connected` badges.
2. Click into `generic-webhook`. The detail page shows the config schema (source label + signing secret, both required). No connection panel yet — "Not connected" message.
3. Sign in as the Agent and try to open `/tenant/integrations`. The page should not load; the API returns 403.

## End-to-end manual flow (US2)

1. As Admin, on the `generic-webhook` detail page, fill in `source_label = "Billing system"` and `signing_secret = "whsec_test_1234"`, then click **Connect**.
2. The status badge flips to **Connected** and the response shows a `webhook_url` of the form `https://<api>/hooks/v1/<token>`. The `secrets` list shows `{ "field_key": "signing_secret", "hint": "1234" }`. The plaintext `signing_secret` is **not** anywhere in the response, the page, or the network tab.
3. Open `pgcli` (or psql) and `SELECT * FROM audit_logs WHERE action LIKE 'integration.%';` — there should be one `integration.connected` row, and the JSON `details` should be `{"connectionId": "...", "slug": "generic-webhook"}` (no secrets).
4. Click **Disconnect**. The badge flips to **Disconnected**, the secrets list empties, the webhook URL becomes `null`. `SELECT * FROM integration_events WHERE connection_id = ...` now has two rows: `connected` and `disconnected`. The event history is preserved.
5. Click **Connect** again. A new `webhook_url` is issued (different token), the `integration_connections` row for `(tenant, generic-webhook)` is still the same single row, and the event log now reads `connected, disconnected, connected`.

## End-to-end manual flow (US3)

1. From the connected state, sign a body and POST it to the webhook URL:

   ```bash
   BODY='{"event":"invoice.paid","id":"inv_42"}'
   SECRET="whsec_test_1234"   # the value you used in US2 step 1
   SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -hex | awk '{print $2}')
   curl -i -X POST "$WEBHOOK_URL" \
        -H "Content-Type: application/json" \
        -H "X-Webhook-Signature: sha256=$SIG" \
        --data "$BODY"
   ```

   Expect `202 {"status":"accepted"}` and a new row in `integration_webhook_deliveries` plus a `delivery_accepted` event.

2. POST with a wrong signature. Expect a `404` with a body byte-identical to the next case, plus a `delivery_rejected` event with `reason: "invalid_signature"`.

3. Disconnect the integration, then POST again (still signed correctly). Expect the same `404` body, plus a throttled `inactive_connection` event (one row per connection per minute even if you POST 100 times).

4. POST a body > 256 KB. Expect `413 payload too large`.

5. POST 61 correctly signed deliveries within one minute. The first 60 return 202; the 61st returns `429`. `integration_events` should contain at most one `rate_limited` row from that burst (throttled).

6. As Admin, return to the `generic-webhook` detail page. The event log should list each of the above in newest-first order with type, outcome, reason, and relative time. "Load more" is available if the page is full.

7. Sign in as the second tenant's Admin and try `GET /api/v1/tenant/integrations/generic-webhook/events`. The response is `404` (the slug has no connection for that tenant) and the event log for the first tenant is not visible.

## Common failures and what they mean

| Symptom | Cause | Fix |
|---|---|---|
| `APP_INTEGRATION_SECRETS_KEY must be a base64-encoded string of exactly 32 bytes` at server start | env var missing or wrong length | Generate one with `openssl rand -base64 32` and set it. |
| `cargo test -p integrations` fails to compile a referenced module (e.g. `webhook`) | Phase 2 placeholder files are still in place | Implement the referenced task or run the right subset of tests. |
| Detail page shows `webhook_url: null` after connecting | US2 (T035) is not yet implemented | Confirm task T035 has landed. |
| `cargo fmt` complains repo-wide | Pre-existing at HEAD, unrelated to this feature | Do not treat it as a regression. |
| `pnpm ng test dashboard` shows a `knowledge-base` failure | Pre-existing dirty state at the start of the session | Not part of this spec; ignore or fix separately. |
