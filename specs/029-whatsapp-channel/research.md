# Research: WhatsApp Channel (029)

All Technical Context unknowns resolved. Decisions below are grounded in the current codebase (verified against source, not prior plans) and Meta WhatsApp Business Cloud API behavior as of mid-2026.

## R1. Provider integration surface (Meta Cloud API)

**Decision**: Integrate directly with Meta's WhatsApp Business Cloud API (Graph API). Per-tenant configuration fields: `phone_number_id` (text, required), `business_phone` (display text, required), `access_token` (secret, required), `app_secret` (secret, required — HMAC key for webhook signatures), `verify_token` (secret, required — echoed during Meta's GET verification handshake). Sends go to `POST https://graph.facebook.com/<version>/{phone_number_id}/messages` with `Authorization: Bearer <access_token>`; pin one Graph version in code (config-overridable), current stable at implementation time.

**Rationale**: Clarification session chose Meta Cloud API directly (bring-your-own Meta business account). These five fields are the minimal set covering the verification handshake, signature verification, and message sending. The 028 `config_schema` jsonb (`text` + `secret` field kinds) expresses this schema without schema-system changes.

**Alternatives considered**: Twilio WhatsApp (rejected in clarification — middleman fees, second-hand API); provider-agnostic multi-provider adapter in v1 (rejected — YAGNI; the `WhatsAppApi` trait boundary keeps the door open).

## R2. Webhook endpoint shape and verification

**Decision**: New public routes in the `whatsapp` module, addressed by the 028 connection token: `GET /integrations/whatsapp/webhook/{token}` implements Meta's subscription handshake (verify `hub.verify_token` matches the connection's stored `verify_token` secret, echo `hub.challenge` as 200 text); `POST /integrations/whatsapp/webhook/{token}` receives deliveries, verified by recomputing HMAC-SHA256 of the **raw body** with the connection's `app_secret` and constant-time-comparing against `X-Hub-Signature-256: sha256=<hex>`.

**Rationale**: Meta's signature scheme (app-secret HMAC, `X-Hub-Signature-256` header) and GET handshake differ from the 028 generic webhook (`X-Webhook-Signature`, no handshake), so the generic endpoint cannot be reused as-is. But token-in-path addressing, SHA-256 token-hash lookup (`integration_connections_token_hash_uq`), uniform 404 on unknown/inactive/unverified (no existence leak), per-connection `InMemoryRateLimitStore` limiting, and delivery/event logging all reuse the 028 machinery (`integrations::webhook::hash_token`, queries, event writers). `hmac::Mac::verify_slice` gives constant-time comparison, same as 028.

**Alternatives considered**: Reusing `POST /integrations/webhooks/{token}` with per-catalog dispatch inside the generic handler (rejected — bloats the generic path with Meta-specific handshake/headers and couples integrations to channel logic; integrations must stay channel-agnostic); a fixed per-tenant URL without token (rejected — token addressing is the established secret-URL scheme and gives the connection lookup for free).

## R3. Inbound intake pipeline

**Decision**: Synchronous intake per delivery, kept minimal: verify → look up active connection → insert `integration_webhook_deliveries` row + `delivery_accepted` event (reuse 028 writers) → for each message entry in the payload: dedupe on provider message id (`wamid`) via unique index; resolve customer identity; create-or-append conversation; insert `customer` message via `conversations::queries::add_message_in_tx`; emit `conversation.customer_message` outbox event with `channel: "whatsapp"` (existing `emit_customer_message_in_tx`). Media entries additionally enqueue a media-fetch job (R6). Status entries (R5) update outbound message meta. Ack 200 after commit.

**Rationale**: The existing AI pipeline is triggered entirely by the `conversation.customer_message` outbox event — the agent responder already gates on `channel` against agent-config `enabled_channels`, and `whatsapp` is already in `CATALOG_CHANNELS` (`ai/src/agent_config.rs:11`). So AI response on WhatsApp requires zero AI-module changes. Message insert amounts follow the widget precedent (`widgets/src/public_routes.rs` uses the same `*_in_tx` services). Everything runs in one transaction per message so provider retries are all-or-nothing (spec edge case).

**Alternatives considered**: Queue-then-process (persist raw delivery, process in a worker) — rejected for v1: adds latency against SC-002 and a second failure surface; Meta's retry semantics plus wamid dedupe already give at-least-once → exactly-once conversion. Revisit if intake work grows (e.g., outbound media).

## R4. Message deduplication and ordering

**Decision**: New table `whatsapp_message_meta` linking each WhatsApp-channel `messages` row to its provider identity: unique index on `(tenant_id, wamid)` makes duplicate deliveries no-ops (`INSERT ... ON CONFLICT DO NOTHING` detected before message insert; the meta row and message insert share the intake transaction). Conversation ordering keeps platform arrival order (`messages.seq`); the provider timestamp is stored on the meta row for display/diagnostics but does not reorder the timeline.

**Rationale**: SC-003 requires exactly-once representation. A DB uniqueness guarantee is the only race-safe dedupe under concurrent webhook redeliveries. Reordering the append-only timeline by provider timestamps would break the `seq`-based pagination and SSE increments used by the dashboard; out-of-order arrival is rare and visible via the stored provider timestamp.

**Alternatives considered**: In-memory dedupe cache (rejected — lost on restart, not multi-instance-safe); storing wamid on `messages` directly (rejected — channel-specific columns on the shared append-only table pollute every other channel; a side table mirrors how `message_citations` extends messages).

## R5. Outbound delivery and status lifecycle

**Decision**: Outbound is outbox-driven. When a `reply` (agent) or `ai` message is inserted into a conversation whose `channel = 'whatsapp'`, the same transaction emits a new `whatsapp.outbound_message` outbox event (new `conversations::outbox::emit_whatsapp_outbound_in_tx`; called from the conversations reply route and from the AI responder's reply-insert path). A new `whatsapp` sender worker (spawned in `server/main.rs` beside `agent_responder`) claims these events `FOR UPDATE SKIP LOCKED`, loads the tenant's connection + decrypted `access_token`, pre-checks the 24h window (R7), calls the Graph API via the `WhatsAppApi` trait, and writes `whatsapp_message_meta` for the outbound message: `pending → sent` (Graph accept, store returned wamid) or `failed` + reason (Graph error). Later `statuses` webhook entries (`sent`/`delivered`/`read`/`failed`) update the meta row by wamid with monotonic status progression (never downgrade, e.g. ignore `delivered` after `read`). Status changes and failures surface to the dashboard through the existing tenant SSE stream (escalations events worker pattern) as a `conversation.message_status` event.

**Rationale**: Clarification chose full lifecycle tracking. The outbox pattern is the platform's established cross-module trigger (Constitution I; AI responder and escalations both work this way) and guarantees no send is lost between message insert and dispatch. Emitting from the AI responder by hand mirrors the notifications precedent (hand-rolled emits to avoid crate cycles — `ai` cannot depend on `whatsapp`). SKIP LOCKED claiming matches `agent_responder`'s concurrency model.

**Alternatives considered**: Synchronous send inside the reply request (rejected — couples agent UX latency to Meta latency, loses retry, and the AI path has no request to fail); polling `messages` for undelivered rows (rejected — no clean "needs delivery" marker without scanning; outbox events are the existing idiom).

## R6. Inbound media pipeline

**Decision**: Inbound media messages (image/audio/video/document) insert their message row immediately (body = caption if present, else a type placeholder) plus a `message_attachments` row with status `pending` and the provider media id. A media-fetch worker claims pending attachments, calls Graph `GET /{media-id}` for the short-lived download URL, streams the bytes to S3-compatible storage via `shared/storage` under a tenant-prefixed key (`whatsapp-media/{tenant_id}/{attachment_id}`), and marks the row `stored` (with mime/size) or `failed` after bounded retries. The dashboard fetches media through an authenticated tenant endpoint `GET /tenant/conversations/{id}/attachments/{attachment_id}` that streams from storage. Non-media special types (location, contacts, stickers, unknown) become typed placeholder messages with no attachment row.

**Rationale**: Clarification chose full inbound media support. Graph media URLs expire in ~5 minutes, so fetch must be prompt but need not be synchronous with the webhook ack (Meta times out slow handlers). The knowledge module already established the S3 put-then-persist pattern and bucket configuration; `message_attachments` is deliberately channel-generic (email attachments later reuse it). Serving via backend proxy (not presigned public URLs) keeps tenant isolation enforcement server-side (Constitution II).

**Alternatives considered**: Synchronous media download during webhook handling (rejected — risks Meta timeout/retry storms on large files); presigned S3 URLs to the browser (rejected for v1 — bypasses tenant-scoped authorization path; can be added later behind the same endpoint).

## R7. 24-hour customer-service window enforcement

**Decision**: Derive the window from data already present: the latest `customer`-kind message timestamp in the conversation (`messages` timeline index makes this cheap). Enforce twice: (a) the conversations reply route rejects agent free-form replies into WhatsApp conversations whose last customer message is older than 24h with a structured validation error the UI renders ("messaging window expired"); (b) the sender worker re-checks before dispatch and maps Meta's re-engagement error (code `131047`) to a `failed` status with the same human-readable reason, covering AI replies and races.

**Rationale**: Spec FR-013 requires blocking or failing with a clear explanation. No new state table is needed — the last inbound customer message *is* the window anchor, and deriving it avoids a denormalized column that could drift. Double enforcement gives agents an immediate, friendly pre-check while the worker-side check remains the source of truth.

**Alternatives considered**: A `last_customer_message_at` column on conversations (rejected — denormalization without a measured need; `last_activity_at` exists but counts outbound too, so it cannot anchor the window); relying solely on Meta's rejection (rejected — poor agent UX, burns a Graph call per attempt).

## R8. Customer identity resolution

**Decision**: Normalize the sender's `wa_id` (Meta already supplies digits-only E.164 without `+`) to a canonical `+`-prefixed E.164 string in one `identity.rs` helper. Resolution order within the intake transaction: (1) live `customer_channel_identifiers` row with `channel='whatsapp'` and matching identifier → that customer; (2) live row with `channel='phone'` and matching normalized identifier → attach a new `whatsapp` identifier to that customer; (3) else create a customer (name from the WhatsApp profile name when provided, else the phone number) with a `whatsapp` identifier. All lookups tenant-scoped; the existing live-unique partial index `(tenant_id, channel, identifier)` makes concurrent first-messages race-safe (`ON CONFLICT` → re-fetch).

**Rationale**: Clarification chose exact-number auto-link with no fuzzy heuristics. The `customer_channel_identifiers` table (0025) already models exactly this, with `whatsapp` in its channel vocabulary. Storing canonical E.164 makes phone↔whatsapp cross-channel matching a string equality; normalization at the single write/read boundary keeps stored data comparable.

**Alternatives considered**: Fuzzy matching on profile name/email (rejected in clarification — privacy hazard of merging strangers); storing raw `wa_id` without normalization (rejected — breaks equality against `phone` identifiers entered with `+`).

## R9. Conversation mapping

**Decision**: Append to the customer's most recent non-closed (`status IN ('open','escalated')`, `deleted_at IS NULL`) conversation with `channel='whatsapp'`; else create one via `conversations::queries::create_conversation_in_tx` with `channel='whatsapp'`. One active WhatsApp conversation per customer per tenant at a time (matching the check performed at lookup, not a DB constraint — closed→new-message reopens as a fresh conversation per spec US2 scenario 3).

**Rationale**: `conversations.channel` already includes `whatsapp` in its CHECK constraint (0026) — no migration needed for the enum. The widget module set the open-conversation-reuse precedent, and the inbox (`inbox_query`) needs no changes to list WhatsApp conversations.

**Alternatives considered**: Always-new conversation per message burst (rejected — fragments history, breaks AI conversation memory); DB-level partial unique index enforcing one open whatsapp conversation per customer (deferred — intake is the only writer and is serialized per customer by the identifier insert; add if a second writer appears).

## R10. Frontend surfaces

**Decision**: Three touch areas, all extending existing features: (1) **Integrations pages** — WhatsApp becomes a connectable catalog entry; the schema-driven connect form (from 028) renders its fields; the detail page shows the WhatsApp-specific webhook URL (`/integrations/whatsapp/webhook/{token}`) — the existing webhook-URL display is parameterized by catalog slug rather than duplicated. (2) **Inbox** — channel badge/icon for `whatsapp` added to the existing channel presentation and filter vocabulary (inbox already carries `channel` on conversation rows). (3) **Conversation view** — a reusable attachment component (image preview, audio/video element, document download link, pending/failed states), delivery-status ticks on outbound messages (pending/sent/delivered/read/failed + reason tooltip), and the window-expired send error surfaced inline at the composer. Timeline API responses gain optional `attachments` and `delivery` fields (loaded batch-wise like citations — no N+1).

**Rationale**: Constitution IX (extend components, never fork pages); the analytics/inbox channel vocabulary already includes whatsapp values server-side. Batch-loading attachments/status mirrors `load_citations_for_messages`.

**Alternatives considered**: A dedicated WhatsApp settings page outside integrations (rejected in clarification); polling for status updates (rejected — tenant SSE stream already exists and the escalations worker shows the fan-out pattern).

## R11. Testing strategy

**Decision**: Unit: signature verification (valid/invalid/malformed header), verify-handshake, E.164 normalization, window math, status monotonicity. Integration (DB required, narrow per-crate suites — the workspace-wide gate hides failures): intake end-to-end (new customer / existing whatsapp identity / phone-identity attach / dedupe on redelivery / closed-conversation reopen), outbound event emission from reply route and AI responder, sender worker against a `WhatsAppApi` test double (success, Graph error, 131047 window error), media worker double (stored/failed), attachment endpoint authorization (cross-tenant 404). Frontend: store/component tests for attachment rendering and status ticks.

**Rationale**: Matches Constitution VII and the repo's established per-crate test layout; the trait double keeps tests hermetic (no Meta calls in CI).

**Alternatives considered**: Live Meta sandbox tests (rejected for CI — credentials and flakiness; quickstart.md documents a manual end-to-end check instead).
