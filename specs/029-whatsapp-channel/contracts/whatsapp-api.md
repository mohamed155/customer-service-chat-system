# Contracts: WhatsApp Channel (029)

Conventions follow the platform's existing REST contract style: JSON bodies, camelCase response fields, `ApiError` envelope for errors, tenant routes require session auth + `X-Tenant-ID` + permission checks.

## 1. Public webhook (unauthenticated, signature-verified)

### `GET /integrations/whatsapp/webhook/{token}`

Meta subscription verification handshake.

Query params: `hub.mode=subscribe`, `hub.verify_token`, `hub.challenge`.

| Condition | Response |
|-----------|----------|
| Token resolves to an active whatsapp connection AND `hub.verify_token` matches the connection's stored `verify_token` secret AND mode is `subscribe` | `200` text/plain body = `hub.challenge` verbatim |
| Anything else (unknown token, inactive, wrong verify_token, wrong mode) | `404 {"error":"not found"}` — byte-identical to 028's uniform rejection |

### `POST /integrations/whatsapp/webhook/{token}`

Meta delivery intake. Headers: `X-Hub-Signature-256: sha256=<hex>` (HMAC-SHA256 of raw body keyed by connection's `app_secret`).

Accepted payload: Meta webhook envelope (`entry[].changes[].value`) containing any mix of `messages[]` (text / image / audio / video / document / location / contacts / sticker / unknown) and `statuses[]` (`sent` / `delivered` / `read` / `failed` with error details).

| Condition | Response | Side effects |
|-----------|----------|--------------|
| Unknown token / inactive connection | `404` uniform | throttled `delivery_rejected` event (028 policy) |
| Bad/missing signature | `404` uniform | `delivery_rejected` (`invalid_signature`) |
| Rate limit exceeded (60/min per connection, 028 policy) | `429` | `delivery_rejected` (`rate_limited`) |
| Payload >256KB or malformed JSON/envelope | `400` | `delivery_rejected` (`payload_too_large` / `malformed_payload`) |
| Verified | `200 {"received":true}` | delivery row + `delivery_accepted` event; per message: dedupe→identity→conversation→message + `conversation.customer_message` outbox emit; media → `message_attachments(pending)`; per status: monotonic `whatsapp_message_meta.delivery_status` update + `conversation.message_status` SSE |

Idempotency: redelivery of an already-seen `wamid` → `200`, no new message. Unknown-`wamid` status entries are ignored (logged at debug).

## 2. Tenant API additions

### `GET /tenant/conversations/{conversationId}/attachments/{attachmentId}`

Streams stored media. Auth: session + tenant context + existing conversation view permission. Response: `200` with stored `Content-Type`/`Content-Length` (and `Content-Disposition: attachment; filename=...` for documents); `404` if the attachment isn't `stored` or doesn't belong to the tenant/conversation (uniform); `409` never (no partial states leak).

### Timeline response extension (existing `GET /tenant/conversations/{id}` / timeline endpoint)

Each message object gains optional fields (absent for non-WhatsApp channels; batch-loaded, no N+1):

```jsonc
{
  "id": "...",
  "kind": "customer | reply | ai | system | note",
  "body": "...",
  // NEW, WhatsApp messages only:
  "attachments": [
    { "id": "...", "kind": "image", "status": "stored", "mimeType": "image/jpeg",
      "sizeBytes": 12345, "fileName": null,
      "url": "/tenant/conversations/{cid}/attachments/{aid}" }   // null unless stored
  ],
  "delivery": { "status": "pending|sent|delivered|read|failed", "failureReason": null }  // outbound only
}
```

### Reply endpoint behavior change (existing `POST .../messages`, kind `reply`)

For conversations with `channel = "whatsapp"`:

| Condition | Response |
|-----------|----------|
| Last customer message older than 24h | `422` validation error, code `whatsapp_window_expired`, message "The WhatsApp messaging window has expired for this conversation." |
| Channel connection disconnected/missing | `422`, code `whatsapp_channel_disconnected` |
| Body > 4096 chars | `422`, code `whatsapp_body_too_long` |
| OK | `201` message created (delivery proceeds async; message starts with `delivery.status = "pending"`) |

## 3. Integrations surface (no endpoint changes)

The whatsapp catalog entry flows through the existing 028 endpoints (`GET /tenant/integrations`, `GET/POST/PATCH/DELETE /tenant/integrations/{slug}/connection`, secret rotation, events). The connect response's webhook URL for slug `whatsapp` is `/integrations/whatsapp/webhook/{token}` (slug-parameterized base path in the existing URL-display logic).

## 4. Internal events

| Event | Producer → Consumer | Payload |
|-------|--------------------|---------| 
| `conversation.customer_message` (existing) | whatsapp intake → ai agent responder | `{conversation_id, message_id, channel:"whatsapp"}` |
| `whatsapp.outbound_message` (new outbox type) | conversations reply route & ai responder → whatsapp sender worker | `{tenantId, conversationId, messageId}` |
| `conversation.message_status` (new tenant SSE event) | whatsapp sender/status intake → dashboard realtime client | `{conversationId, messageId, status, failureReason?}` |

## 5. Outbound Graph API calls (behind `WhatsAppApi` trait)

| Call | Purpose |
|------|---------|
| `POST /{phone_number_id}/messages` `{messaging_product:"whatsapp", to, type:"text", text:{body}}` | send agent/AI reply; returns `messages[0].id` (wamid) |
| `GET /{media_id}` | resolve short-lived media download URL + mime |
| `GET <media url>` (Bearer) | download media bytes → S3 |

Error mapping: Graph error code `131047` → `failed` + window-expired reason; `401/403`-class → `failed` + credential reason (also surfaces connection error status via an `integration_events` failure entry); transient 5xx/network → bounded retry, then `failed`.
