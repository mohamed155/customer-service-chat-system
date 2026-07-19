# Contract: Public Widget API (unauthenticated surface)

Base path: `/widget/v1`. No dashboard auth, no `X-Tenant-ID` (FR-025). Error envelope, `X-Request-Id`, and naming follow `specs/001-ai-customer-service-platform/contracts/rest-api.md`. CORS: `Access-Control-Allow-Origin: *`, no credentials. Every endpoint enforces the rate limits from research R5 (429 `rate_limited` on excess) and, when the instance has a non-empty `allowed_domains`, the Origin/Referer allowlist check (403 `origin_not_allowed`).

Auth model: endpoints marked **[session]** require `Authorization: Bearer <widget session token>`; 401 `session_invalid` on missing/expired/unknown tokens (widget reacts by minting a new session and starting fresh).

## GET /widget/v1/config?widgetId={publicId}

Public widget configuration (FR-002). No session required.

- 200:
  ```json
  { "data": {
      "widgetId": "wgt_…",
      "displayName": "Support",
      "primaryColor": "#4F46E5",
      "welcomeMessage": "Hi! How can we help?",
      "position": "bottom-right",
      "theme": "light",
      "enabled": true
  } }
  ```
- 404 `widget_not_found` (unknown/deleted publicId), 403 `origin_not_allowed`. Disabled instances return 200 with `"enabled": false` (loader renders nothing). Never includes tenant IDs or internal fields (FR-024).

## POST /widget/v1/sessions

Mint an anonymous session (FR-007). Body: `{ "widgetId": "wgt_…" }`. Rate-limited per IP.

- 201: `{ "data": { "sessionToken": "<opaque, shown once>", "expiresAt": "…" } }`
- 404/403 as above.

## GET /widget/v1/conversation **[session]**

Resolve the session's current conversation (US4 resume; FR-027 lock semantics: closed conversations are never returned).

- 200 with conversation view, or 200 `{ "data": null }` when none/closed:
  ```json
  { "data": {
      "conversation": {
        "id": "…", "handling": "ai|human|closed",
        "teamOnline": true,
        "endedNote": false,
        "messages": [ { "id": "…", "sender": "visitor|assistant|agent",
                        "senderDisplayName": "…", "body": "…", "createdAt": "…" } ]
  } } }
  ```
  `sender` is the sanitized mapping of participant types; agent messages expose display name only (FR-024). `endedNote: true` accompanies `handling: "closed"` so the widget can show the "conversation ended" note once (Q2).

## POST /widget/v1/conversations **[session]**

Create a conversation (FR-011); idempotent-by-state: if the session already has a non-closed conversation, returns it (200) instead of creating (409 never used). Lazily creates the anonymous customer (R8).

- 201: same conversation view as above (empty `messages` plus the welcome message is client-rendered, not stored).

## POST /widget/v1/conversations/{conversationId}/messages **[session]**

Send a visitor message (FR-012). Body: `{ "body": "…" }`, 1–4000 chars after trim (FR-017). Conversation must belong to the session and be non-closed (409 `conversation_closed` → widget starts a new conversation flow per FR-027).

- 201: `{ "data": { "message": { …message view… } } }` — the visitor's own message echo. The AI reply arrives via SSE (R7: outbox → responder worker; FR-021: no AI reply when handling=human).

## GET /widget/v1/conversations/{conversationId}/events **[session]**

Per-conversation SSE stream (R6). `text/event-stream`, keep-alive comment every 20 s. Token passed via `Authorization` header (fetch-based SSE client, matching `libs/realtime`). Events, all scoped to this conversation only:

| event | data | Purpose |
|---|---|---|
| `message.created` | full message view | AI final messages, human agent replies, system notes (FR-013/FR-020) |
| `ai.delta` | `{ "messageId": null, "text": "…" }` | Incremental AI output chunks, ~4/s (FR-014); terminated by the corresponding `message.created` |
| `conversation.updated` | `{ "handling": "…", "teamOnline": bool }` | Handoff, away, closed transitions (FR-019, FR-028, FR-027) |

Reconnection: client reconnects with backoff and re-fetches `GET /conversation` to resync missed state (edge case: network loss).

## Rate-limit defaults (R5)

| Scope | Key | Limit |
|---|---|---|
| messages per session | session id | 10/min (burst 5) |
| session + conversation creation | client IP | 10/min |
| all `/widget/v1` requests | **tenant id** | 600/min |

The global bucket is keyed by tenant, not by widget instance, so a tenant with several instances shares one budget (FR-022). 429 body uses code `rate_limited`; existing conversations are never terminated by limiting (FR-022).
