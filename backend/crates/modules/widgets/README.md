# Widgets Module

## Purpose

Website Chat Widget module — enables tenant websites to embed a branded, real-time chat widget that lets visitors converse with the AI customer service agent.

## Responsibilities

- **Public config** — serve branded widget configuration (colors, position, welcome message) to embed snippets without authentication.
- **Session management** — mint anonymous sessions with bearer tokens, enforce origin allowlists, slide session expiry on each authenticated call, sweep expired sessions.
- **Conversation relay** — provide public REST + SSE endpoints that let a visitor send messages, receive streamed AI replies, and observe handoff/status changes through the widget's own vocabulary (visitor/assistant/agent/system — never internal note or tool events).
- **Admin CRUD** — full create/read/update/soft-delete lifecycle for widget instances via the dashboard API, plus copy‑to‑clipboard embed snippets.
- **Rate limiting** — per‑session message budget, per‑IP creation budget, per‑tenant global budget (in‑memory fixed‑window counters).

## Public Interfaces

### Routes (mounted at `/widget/v1`)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/widget/v1/config` | Public widget config (no auth) |
| POST | `/widget/v1/sessions` | Mint an anonymous session |
| GET | `/widget/v1/conversation` | Get the session's current conversation |
| POST | `/widget/v1/conversations` | Create or reuse a conversation |
| POST | `/widget/v1/conversations/{id}/messages` | Send a message |
| GET | `/widget/v1/conversations/{id}/events` | SSE stream of conversation events |

### Admin routes (mounted at `/tenant/widgets`)

| Method | Path | Permission |
|--------|------|------------|
| GET | `/tenant/widgets` | `WidgetsView` |
| POST | `/tenant/widgets` | `WidgetsManage` |
| GET | `/tenant/widgets/{id}` | `WidgetsView` |
| PUT | `/tenant/widgets/{id}` | `WidgetsManage` |
| DELETE | `/tenant/widgets/{id}` | `WidgetsManage` |
| GET | `/tenant/widgets/{id}/snippet` | `WidgetsView` |

### Key queries (re‑exported from `queries.rs`)

- `find_instance_by_public_id(pool, public_id)` — public config lookup
- `find_instance_by_id(pool, tenant_id, instance_id)` — admin CRUD
- `insert_session(pool, tenant_id, instance_id, token_hash, expires_at)`
- `ensure_customer_for_session(tx, pool, session)` — lazy customer creation
- `delete_expired_sessions(pool)` — periodic sweep

### Session utilities (`session.rs`)

- `generate_token() -> String` — 32 random bytes hex‑encoded (64 hex chars)
- `hash_token(token) -> Vec<u8>` — SHA‑256 of the raw token (never stored)
- `authenticate_session(pool, auth_header) -> Result<WidgetSessionRow, ApiError>` — validates bearer token, slides expiry
- `SESSION_TTL_HOURS: i64` — 24‑hour session window

### Origin checking (`origin.rs`)

- `origin_allowed(allowed_domains, origin_header, referer_header) -> bool` — domain allowlist with `*.` wildcard support

## Dependencies

| Crate | What for |
|-------|----------|
| `conversations` | `create_conversation_in_tx`, `add_message_in_tx`, `timeline_query_in_tx`, `emit_customer_message_in_tx` |
| `customers` | `create_anonymous_customer_in_tx` (anonymous visitor support, extended in T013) |
| `escalations` | `presence::Runtime::subscribe`, `presence::Runtime::present_membership_ids_async`, `presence::Event` — SSE relay and team‑online status |
| `kernel` | `ApiError`, `ApiJson`, `ErrorEnvelope` |
| `tenancy` | `TenantContext` extractor for admin routes |
| `identity` | `Principal` for admin route auth |
| `authz` | `Permission::WidgetsView`, `Permission::WidgetsManage` |

### Cross‑module extension point (T011–T013)

The `conversations` and `customers` modules were widened to accept an **anonymous visitor actor** (the widget user) who has no staff `user_id` or `membership_id`. This is accessed via:
- `conversations::queries::ConversationActor::Visitor { customer_id }`
- `customers::create_anonymous_customer_in_tx(tx, tenant_id, display_name, "widget", session_id)`

The widgets module **must never** write directly to `conversations`, `messages`, `customers`, or `customer_channel_identifiers` tables — it calls the above interfaces instead.

## Data Model

### `widget_instances`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID | PK |
| tenant_id | UUID | FK → tenants |
| public_id | TEXT | `wgt_` + 22 base62 chars, unique, immutable |
| name | TEXT | 1–80 chars |
| display_name | TEXT | 1–80 chars |
| primary_color | TEXT | `#[0-9a-fA-F]{6}`, nullable |
| welcome_message | TEXT | ≤ 500 chars, nullable |
| position | TEXT | `bottom-right` / `bottom-left`, nullable |
| theme | TEXT | `light` / `dark`, nullable |
| enabled | BOOLEAN | Default true |
| allowed_domains | TEXT[] | Max 20 entries, `*.` wildcard support |
| created_at / updated_at | TIMESTAMPTZ | updated_at via trigger |
| deleted_at | TIMESTAMPTZ | Soft‑delete |

### `widget_sessions`

| Column | Type | Notes |
|--------|------|-------|
| id | UUID | PK |
| tenant_id | UUID | FK → tenants |
| widget_instance_id | UUID | FK → widget_instances |
| token_hash | BYTEA | SHA‑256 of the bearer token |
| customer_id | UUID | FK → customers, set lazily on first message |
| last_seen_at | TIMESTAMPTZ | Updated on each auth'd request |
| expires_at | TIMESTAMPTZ | Now + 24h, slid on each auth |
| created_at | TIMESTAMPTZ | |

### `conversations`

- `widget_instance_id` column (nullable UUID, FK → widget_instances) added via ALTER TABLE for attribution in the inbox.

## Extension Points

- **Anonymous actor** (T011–T013): `ConversationActor::Visitor` and `create_anonymous_customer_in_tx` enable widget visitors to participate without a staff identity. Any future public‑facing channel (e.g., WhatsApp, Messenger) can reuse the same mechanisms.
- **Rate‑limiter store**: `RateLimitStore` trait can be backed by Redis (or another shared store) for horizontal scaling — the trait is already extracted.
- **SSE event vocabulary**: the widget's event stream maps internal events to a public vocabulary (`ai.delta`, `message.created`, `conversation.updated`). Adding new event types requires only a new match arm in `WidgetEventStream::filter_and_map`.
