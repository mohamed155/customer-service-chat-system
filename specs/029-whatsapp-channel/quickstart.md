# Quickstart Validation: WhatsApp Channel (029)

Runnable checks proving the feature end-to-end without a live Meta account (webhook deliveries are simulated with signed curl requests). See [contracts/whatsapp-api.md](contracts/whatsapp-api.md) for payload shapes and [data-model.md](data-model.md) for tables.

## Prerequisites

- PostgreSQL up with migrations applied through `0057_whatsapp_channel.sql` (`cd backend && sqlx migrate run` or the repo's usual migration command).
- Backend running (`cargo run -p server`) with `INTEGRATION_SECRETS_KEY` set (same requirement as 028) and S3-compatible storage configured (same env as the knowledge module; MinIO locally is fine).
- Dashboard running (`npx nx serve dashboard` from `frontend/`), signed in as a tenant Owner/Admin.

## 1. Connect the channel (US1)

1. Dashboard → Integrations → WhatsApp → Connect. Fill: `phone_number_id` (any test value, e.g. `123456`), `business_phone`, `access_token`, `app_secret` (remember it — it signs the simulated webhooks, e.g. `test-app-secret`), `verify_token` (e.g. `test-verify`).
2. Expect: status **connected**, webhook URL `/integrations/whatsapp/webhook/<token>` displayed, secrets shown masked only. Note `<token>`.
3. Handshake check:
   `curl "localhost:<port>/integrations/whatsapp/webhook/<token>?hub.mode=subscribe&hub.verify_token=test-verify&hub.challenge=42"` → body `42`.
   Wrong verify_token → `404 {"error":"not found"}`.

## 2. Inbound message → inbox (US2)

1. Save a minimal Meta payload as `inbound.json` (envelope with one text message, `wa_id` = `2015550123`, a fresh `wamid`).
2. Sign and send:
   ```bash
   SIG=$(openssl dgst -sha256 -hmac 'test-app-secret' -hex < inbound.json | awk '{print $2}')
   curl -X POST "localhost:<port>/integrations/whatsapp/webhook/<token>" \
        -H "Content-Type: application/json" -H "X-Hub-Signature-256: sha256=$SIG" \
        --data-binary @inbound.json
   ```
3. Expect: `200 {"received":true}`; inbox shows a new conversation with a WhatsApp badge; a new customer exists with WhatsApp identity `+2015550123`; integration event log shows `delivery_accepted`.
4. **Dedupe**: resend the identical payload → `200`, no second message (SC-003).
5. **Identity attach**: create a customer manually with phone identifier `+2015550124`, send an inbound from `wa_id 2015550124` → message lands on that customer, no duplicate profile (FR-007).
6. **Rejection**: send with a wrong signature → `404`, no side effects, `delivery_rejected` logged.
7. **Media**: send an inbound image payload (any `provider_media_id`) → message appears with a pending attachment; with a `WhatsAppApi` double or MinIO-backed fake it transitions to viewable; on fetch failure it shows the failed indicator (FR-010).

## 3. Agent reply (US3)

1. Open the conversation, send a reply → message appears with delivery status **pending**, then **sent** once the sender worker dispatches (with a stubbed/test-doubled Graph API in dev; against real Meta it progresses to delivered/read).
2. Simulate a status webhook (`statuses[]` entry for the outbound wamid, `status:"read"`) → the message shows **read**; a late `delivered` for the same wamid does not downgrade it.
3. **Window**: in a conversation whose last customer message is >24h old (backdate in SQL for the test), send a reply → `422 whatsapp_window_expired`, composer shows the explanation (FR-013).
4. **Disconnected**: disconnect the integration, attempt a reply → `422 whatsapp_channel_disconnected`; inbound deliveries now return `404`.

## 4. AI reply (US4)

1. Enable AI handling for the tenant with `whatsapp` in the agent configuration's enabled channels.
2. Send a new inbound webhook message → the agent responder generates an `ai` message (existing pipeline) and the sender worker dispatches it; conversation shows the AI reply with delivery status.
3. Trigger an escalation rule → standard handoff/routing/notifications fire, identical to web_chat (FR-015).

## 5. Automated suites

```bash
# Backend (DB required; run narrow suites — the workspace-wide gate skips/aborts):
cd backend
cargo test -p whatsapp
cargo test -p conversations
cargo test -p integrations

# Frontend:
cd frontend && npx nx test dashboard
```

Expected: all green; whatsapp suite covers signature/handshake, intake (dedupe/identity/mapping), window math, sender worker with API double, status monotonicity, attachment endpoint tenant isolation.
