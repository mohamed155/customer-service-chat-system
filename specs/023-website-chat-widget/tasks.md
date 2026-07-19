---

description: "Task list for Website Chat Widget implementation"
---

# Tasks: Website Chat Widget

**Input**: Design documents from `/specs/023-website-chat-widget/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/public-widget-api.md](contracts/public-widget-api.md), [contracts/widget-admin-api.md](contracts/widget-admin-api.md), [quickstart.md](quickstart.md)

**Tests**: INCLUDED — constitution v1.2.0 Principle VII (Test-First & Regression Discipline) makes unit/integration/API/e2e coverage mandatory for shipped functionality.

**Organization**: Tasks are grouped by user story. Each story phase is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US5, mapping to spec.md user stories
- Every task names exact file paths and, where a pattern exists, the file to copy it from

## Conventions the implementer MUST follow

Read this section before starting. It is not optional context.

**Backend (Rust, `backend/`)**

- Workspace members are globbed (`crates/modules/*`), so a new module crate is picked up automatically once its `Cargo.toml` exists — but the `server` crate must still list it as a dependency to use it.
- Handlers return `axum::response::Response`. Errors use `kernel::ApiError` helpers (`validation_failed`, `unprocessable_entity`, `not_found`, `internal_error`, `rate_limited`) and always chain `.with_request_id(&ctx.request_id)` where a context exists, then `.into_response()`. Copy the style from `backend/crates/modules/conversations/src/routes.rs`.
- DTOs derive `Serialize`/`Deserialize`/`ToSchema` with `#[serde(rename_all = "camelCase")]`. Copy from `backend/crates/modules/knowledge/src/routes.rs`.
- Routes are registered in `backend/crates/server/src/router.rs` via `routes!(handler)` on an `OpenApiRouter` so they land in OpenAPI. Handlers need `#[utoipa::path(...)]` attributes.
- SQL uses `sqlx::query`/`query_as`/`query_scalar` with `.bind(...)`. Never string-interpolate values.
- Every tenant-owned query filters by `tenant_id` (constitution II). Public widget queries derive `tenant_id` server-side from the widget instance or session — never from a request header or body.
- **The widgets module MUST NOT write directly to the `conversations`, `messages`, `customers`, or `customer_channel_identifiers` tables.** Those belong to other modules (constitution Principle I). Call the interfaces extended in T011–T013 instead. If you find yourself writing `INSERT INTO conversations` inside `crates/modules/widgets/`, stop — you are on the wrong path.
- Integration tests live in `backend/crates/server/tests/<name>.rs` and build the router via `server::router` + `server::state::AppState`. Copy the harness setup from `backend/crates/server/tests/conversations.rs`.
- Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before considering a backend task done.

**Frontend (Angular, `frontend/`)**

- All components standalone, `changeDetection: ChangeDetectionStrategy.OnPush`, signals for local state. No NgModules.
- Async logic uses RxJS operator composition (constitution: RxJS-first). Do not use `async/await` or `.then()` inside services, stores, or components. The loader (`loader.ts`) is exempt — it is plain DOM JavaScript outside the Angular app.
- `apps/widget` MUST NOT import from `libs/*`, Taiga UI, or NgRx — it is a standalone third-party bundle with a hard size budget (97 KB initial, configured in `frontend/angular.json`).
- **Loader vs. Angular app split** (research [R1](research.md)): the **loader** fetches config, renders the launcher button, and owns the iframe lifecycle; the **Angular app** renders only the chat window's contents and fetches its own copy of config. Do not build a launcher component inside the Angular app.
- `apps/dashboard` follows `frontend/CLAUDE.md`: Taiga UI wrapped in `shared/components/`, paths from `APP_PATHS`, NgRx SignalStore for feature state.
- Run `pnpm lint` and `pnpm format:check` from `frontend/` before considering a frontend task done.

**Definition of done for every task**: code compiles, its own tests pass, formatting/lint clean. Do not mark a task complete on partial work.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the migration, the new backend crate, and the loader build pipeline so later phases have somewhere to put code.

- [X] T001 Create migration `backend/migrations/0050_website_chat_widget.sql` implementing all three schema changes from [data-model.md](data-model.md): `CREATE TABLE widget_instances` (columns id/tenant_id/public_id/name/display_name/primary_color/welcome_message/position/theme/enabled/allowed_domains/created_at/updated_at/deleted_at with the CHECK constraints listed there), `CREATE TABLE widget_sessions` (id/tenant_id/widget_instance_id/token_hash/customer_id/last_seen_at/expires_at/created_at), and `ALTER TABLE conversations ADD COLUMN widget_instance_id UUID NULL REFERENCES widget_instances(id)`. Include every index named in data-model.md and an `updated_at` trigger for `widget_instances` copying the trigger pattern from `backend/migrations/0046_knowledge_base.sql`. Verify with `cd backend && sqlx migrate run` then `sqlx database reset -y`.
- [X] T002 Create the crate skeleton at `backend/crates/modules/widgets/` with `Cargo.toml` (package name `widgets`; copy the dependency block from `backend/crates/modules/tools/Cargo.toml` and keep only: axum, chrono, conversations, customers, escalations, kernel, identity, serde, serde_json, sqlx, tenancy, tokio, tracing, utoipa, uuid; add `rand`, `sha2`, and `futures` from the workspace) and an empty `src/lib.rs` declaring the modules `model`, `queries`, `session`, `origin`, `public_routes`, `public_events`, `admin_routes`, `audit` (create each as an empty file for now). Verify with `cargo check -p widgets`.
- [X] T003 Add `widgets = { path = "../modules/widgets" }` to the `[dependencies]` of `backend/crates/server/Cargo.toml`. Verify with `cargo check -p server`.
- [X] T004 [P] Add `WidgetsView` and `WidgetsManage` variants to the `Permission` enum in `backend/crates/modules/authz/src/permission.rs`, including their string mappings `"widgets.view"` / `"widgets.manage"` in the `as_str` match and their inclusion in the same all-permissions const arrays that list `KnowledgeBaseView`/`KnowledgeBaseManage`.
- [X] T005 [P] Grant the new permissions per role in `backend/crates/modules/authz/src/matrix.rs`: Owner and Admin get both `WidgetsView` and `WidgetsManage`; Manager, Agent, and Viewer get `WidgetsView` only. Mirror exactly how `KnowledgeBaseView`/`KnowledgeBaseManage` are distributed across the same role arrays.
- [X] T006 [P] Create the loader build script `frontend/apps/widget/tools/build-loader.mjs` that bundles `apps/widget/loader/loader.ts` with esbuild (format `iife`, `bundle: true`, `minify: true`, target `es2019`, outfile `dist/widget/widget.js`) and **fails the build with a non-zero exit code if the output exceeds 10240 bytes**; register it as `"build:widget-loader": "node apps/widget/tools/build-loader.mjs"` in `frontend/package.json` scripts.

**Checkpoint**: `cargo check --workspace` passes, migration applies cleanly, `pnpm build:widget-loader` runs (it may fail until T044 creates the loader source).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Widen the neighbouring modules' interfaces so an anonymous visitor is representable, then build the widgets data layer, the security layers, and the mounted public route scope.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Cross-module interface extensions (do these first — everything downstream depends on them)

> Read [research.md R12](research.md) before starting T011–T013. These three tasks exist because the conversations and customers modules currently assume a **signed-in staff actor**, which a widget visitor is not. Do not work around this in the widgets module.

- [X] T007 [P] Define the domain structs in `backend/crates/modules/widgets/src/model.rs`: `WidgetInstanceRow` and `WidgetSessionRow` (`sqlx::FromRow`, matching the migration columns exactly), the public DTO `PublicWidgetConfigDto` (fields per `GET /widget/v1/config` in [contracts/public-widget-api.md](contracts/public-widget-api.md)), the admin DTO `WidgetInstanceDto` (fields per **WidgetInstance** in [contracts/widget-admin-api.md](contracts/widget-admin-api.md)), and the request payloads `CreateWidgetInstancePayload`, `UpdateWidgetInstancePayload`, `CreateSessionPayload { widget_id: String }`, `SendMessagePayload { body: String }`. All DTOs derive `Serialize`/`Deserialize`/`ToSchema` with `#[serde(rename_all = "camelCase")]`. **The public config DTO must not contain `tenant_id`, `id`, `allowed_domains`, or any timestamp** (FR-024).
- [X] T008 [P] Implement validation helpers in `backend/crates/modules/widgets/src/model.rs`: `validate_instance_fields(...) -> Result<(), Vec<serde_json::Value>>` enforcing the limits from [data-model.md](data-model.md) (name and display_name 1–80 chars after trim; welcome_message ≤ 500; primary_color matches `^#[0-9a-fA-F]{6}$`; position in `bottom-right|bottom-left`; theme in `light|dark`; ≤ 20 allowed_domains, each a valid hostname optionally prefixed with `*.`). Return detail objects shaped `{"field":..., "code":..., "message":...}` matching the style used in `backend/crates/modules/conversations/src/routes.rs`. Include unit tests in the same file covering one valid and one invalid case per rule.
- [X] T009 [P] Implement `backend/crates/modules/widgets/src/session.rs`: `generate_token() -> String` (32 random bytes from `rand`, hex-encoded), `hash_token(token: &str) -> Vec<u8>` (SHA-256 via `sha2`), and `const SESSION_TTL_HOURS: i64 = 24`. Add unit tests asserting tokens are 64 hex chars, two generated tokens differ, and `hash_token` is deterministic and never returns the raw token bytes.
- [X] T010 [P] Implement `backend/crates/modules/widgets/src/origin.rs`: `pub fn origin_allowed(allowed_domains: &[String], origin_header: Option<&str>, referer_header: Option<&str>) -> bool`. Rules: empty `allowed_domains` → always `true`; otherwise parse the host out of `origin_header` (falling back to `referer_header`), and match it case-insensitively against each entry — exact host match, or if the entry starts with `*.`, match any single-or-deeper subdomain of the remainder (`*.example.org` matches `a.example.org` and `a.b.example.org`, but **not** bare `example.org`); missing/unparseable origin when the list is non-empty → `false`. Include unit tests for all six of those cases.
- [X] T011 Extend the conversations module for anonymous visitors in `backend/crates/modules/conversations/src/queries.rs`. Today `create_conversation_in_tx` takes `actor_user_id: Uuid, actor_membership_id: Uuid` and `add_message_in_tx` takes `actor_user_id: Uuid` — a widget visitor has neither. Introduce `pub enum ConversationActor { Staff { user_id: Uuid, membership_id: Uuid }, Visitor { customer_id: Uuid } }` and thread it through both functions in place of the raw actor ids. Audit calls (`crate::audit::record_*`) must record visitor-origin writes without inventing a user id — add a visitor-origin variant to the audit helper rather than passing `Uuid::nil()`. Update all existing call sites to pass `ConversationActor::Staff { .. }` so current behavior is unchanged, and confirm with `cargo test -p server --test conversations`.
- [X] T012 Add `widget_instance_id: Option<Uuid>` as a parameter of `create_conversation_in_tx` in `backend/crates/modules/conversations/src/queries.rs` and include the column in its `INSERT INTO conversations (...)` statement, so widget conversations record their origin at creation (FR-032). Existing staff call sites pass `None`.
- [X] T013 Add a public anonymous-customer entry point to the customers module: `pub async fn create_anonymous_customer_in_tx(tx, tenant_id, display_name, channel, identifier) -> sqlx::Result<Uuid>` in `backend/crates/modules/customers/src/queries.rs`, re-exported from `backend/crates/modules/customers/src/lib.rs` alongside the existing `customer_exists_in_tx`. It inserts the `customers` row and its `customer_channel_identifiers` row in one transaction. **Move** the SQL from the two INSERT statements currently embedded in `backend/crates/modules/customers/src/routes.rs` (around lines 614 and 138) into this function and have the existing handler call it, so the SQL exists in exactly one place.

### Widgets data layer, security layers, and public route scope

- [X] T014 Implement queries in `backend/crates/modules/widgets/src/queries.rs`: `find_instance_by_public_id(pool, public_id) -> Option<WidgetInstanceRow>` (excludes `deleted_at IS NOT NULL`), `insert_session(pool, tenant_id, instance_id, token_hash, expires_at) -> WidgetSessionRow`, `find_session_by_token_hash(pool, token_hash) -> Option<WidgetSessionRow>`, `touch_session(pool, session_id, new_expires_at)` (updates `last_seen_at = now()` and `expires_at`), `set_session_customer(pool, session_id, customer_id)`, and `delete_expired_sessions(pool) -> u64`. Every function is `pub async` and returns `sqlx::Result<...>`.
- [X] T015 Implement the session extractor in `backend/crates/modules/widgets/src/session.rs`: `pub async fn authenticate_session(pool: &PgPool, auth_header: Option<&str>) -> Result<WidgetSessionRow, ApiError>`. It strips the `Bearer ` prefix, hashes the token, looks the session up, rejects with a 401 carrying code `session_invalid` when the token is missing/unknown/`expires_at < now()`, and on success calls `touch_session` to slide `expires_at` to `now() + 24h` before returning the row. Use `ApiError::new_with_code(StatusCode::UNAUTHORIZED, "session_invalid", ...)` following the constructor pattern in `backend/crates/shared/kernel/src/lib.rs`.
- [X] T016 Implement the rate limiter in a new file `backend/crates/server/src/rate_limit.rs`: a `RateLimitStore` trait with `fn check(&self, key: &str, limit: u32, window: Duration) -> bool`, an in-process implementation backed by `std::sync::Mutex<HashMap<String, (u32, Instant)>>` using a fixed-window counter, a sweep that drops entries older than their window, and a tower layer `widget_rate_limit_layer(store)` usable via `axum::middleware::from_fn_with_state`. On rejection it returns `kernel::ApiError::rate_limited("Too many requests")` as a 429. Budgets are `pub const` values matching the table in [contracts/public-widget-api.md](contracts/public-widget-api.md): messages 10/min keyed by **session id**, session+conversation creation 10/min keyed by **client IP**, and a global 600/min bucket keyed by **tenant id** (not by widget instance — see [research R5](research.md)). Add unit tests proving a key is allowed up to the limit, rejected past it, allowed again after the window elapses, and that two different tenants do not share a bucket.
- [X] T017 Declare `mod rate_limit;` in `backend/crates/server/src/lib.rs` and wire the limiter's shared store into `backend/crates/server/src/state.rs` (`AppState`) so a single instance is reused across requests.
- [X] T018 Write the **failing** integration test `backend/crates/server/tests/widget_public_foundation.rs` before the endpoints exist (constitution VII): config for a seeded instance returns exactly the public fields and no tenant id; unknown `widgetId` → 404; disabled instance → 200 with `enabled:false`; origin not in a non-empty allowlist → 403; session mint returns a token that then authenticates a request; an expired session → 401 `session_invalid`; exceeding the per-IP creation budget → 429 `rate_limited`; two tenants' traffic does not share the 600/min bucket. Copy the router/AppState harness from `backend/crates/server/tests/conversations.rs`. Confirm the suite fails, then implement T019–T021 until it passes.
- [X] T019 Implement `GET /widget/v1/config` in `backend/crates/modules/widgets/src/public_routes.rs` per [contracts/public-widget-api.md](contracts/public-widget-api.md): read `widgetId` from the query string, look up the instance, return 404 `widget_not_found` when absent, 403 `origin_not_allowed` when `origin_allowed()` fails, otherwise 200 with `PublicWidgetConfigDto`. **Disabled instances return 200 with `enabled: false`, not an error.** Add the `#[utoipa::path]` attribute.
- [X] T020 Implement `POST /widget/v1/sessions` in `backend/crates/modules/widgets/src/public_routes.rs` per the contract: resolve the instance from the body's `widgetId`, apply the same 404/403 checks as T019, mint a token (T009), insert the session with `expires_at = now() + 24h`, and return 201 `{ "data": { "sessionToken": ..., "expiresAt": ... } }`. The raw token is returned here and **never stored** — only its hash goes to the DB.
- [X] T021 Mount the public scope in `backend/crates/server/src/router.rs`: build a `/widget/v1` router from the widgets module's public handlers, apply the rate-limit layer from T016, and merge it into the router alongside the existing `public_routes()` merge (find the line `OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(public_routes());` in `api_routes`). These routes must **not** pass through `authentication_middleware` or the tenancy middleware.
- [X] T022 Extend the CORS configuration in `backend/crates/server/src/router.rs` (`fn cors_layer`) so that `/widget/v1/*` responses allow any origin with `Access-Control-Allow-Origin: *`, allow the `Authorization` and `Content-Type` request headers, allow methods GET/POST/OPTIONS, and send **no** credentials. The existing dashboard CORS behavior for all other paths must be unchanged — update the expectations in `backend/crates/server/tests/cors.rs` to cover both the unchanged dashboard behavior and the new widget scope.
- [X] T023 [P] Register every new widgets DTO and route in the OpenAPI aggregation in `backend/crates/server/src/openapi.rs` following how the knowledge/tools modules register theirs, so `cargo test -p server --test openapi_coverage` still passes.

**Checkpoint**: Foundation ready. `cargo test -p server --test widget_public_foundation` and `--test conversations` both pass. User stories can now begin.

---

## Phase 3: User Story 1 - Customer chats with the AI through the widget (Priority: P1) 🎯 MVP

**Goal**: A visitor on a page hosting the widget can open it, send a message, and see a streamed AI reply.

**Independent Test**: Load the e2e host fixture with a seeded widget instance, open the launcher, send "hello", and observe an AI reply stream into the message list.

### Tests for User Story 1

> Write these first and confirm they fail before implementing.

- [x] T024 [P] [US1] Write `backend/crates/server/tests/widget_conversation_flow.rs`: creating a conversation from a session persists it with `channel = 'widget'`, a lazily-created anonymous customer, and a non-null `widget_instance_id`; posting a message stores it with customer sender kind and writes a `conversation.customer_message` row to `outbox_events` (assert by querying that table directly); posting an empty body → 422; posting a >4000-char body → 422; posting to another session's conversation → 404; **posting to a resolved conversation returns 409 and leaves its status `resolved`** (regression guard for the auto-reopen behavior described in T029).
- [x] T025 [P] [US1] Write `frontend/apps/widget/src/core/widget-api.service.spec.ts` asserting the service sends `Authorization: Bearer <token>` on conversation/message calls, posts to the exact paths in [contracts/public-widget-api.md](contracts/public-widget-api.md), and surfaces a 429 response as a distinct rate-limited error the UI can branch on.

### Backend implementation for User Story 1

- [x] T026 [US1] Implement `ensure_customer_for_session` in `backend/crates/modules/widgets/src/queries.rs`: if the session row already has `customer_id`, return it; otherwise call `customers::create_anonymous_customer_in_tx` (T013) with `display_name = format!("Visitor {}", short_code)` (short_code = first 6 chars of the session id, uppercased), `channel = "widget"`, and `identifier = session.id`, then persist the returned id via `set_session_customer`. **Do not INSERT into `customers` from this crate** — call the interface.
- [x] T027 [US1] Implement `GET /widget/v1/conversation` in `backend/crates/modules/widgets/src/public_routes.rs`: authenticate the session (T015), find the session customer's newest conversation whose `status` is **not** `resolved`/`closed`, and return the conversation view from [contracts/public-widget-api.md](contracts/public-widget-api.md) including its messages; return `{"data": null}` when there is none. Map message senders to the sanitized public vocabulary: `customer` kind → `visitor`, `ai` → `assistant`, `reply` → `agent` (exposing only the agent's display name), `note` messages are **excluded entirely** (internal), `system` → `system`.
- [x] T028 [US1] Implement `POST /widget/v1/conversations` in `backend/crates/modules/widgets/src/public_routes.rs`: authenticate the session, call `ensure_customer_for_session`, and if a non-closed conversation already exists return it with 200; otherwise create one by calling `conversations::queries::create_conversation_in_tx` with `channel = "widget"`, `ConversationActor::Visitor { customer_id }` (T011), and `widget_instance_id = Some(session.widget_instance_id)` (T012), returning 201 with the same conversation view as T027.
- [x] T029 [US1] Implement `POST /widget/v1/conversations/{conversationId}/messages` in `backend/crates/modules/widgets/src/public_routes.rs`: authenticate the session; trim and validate the body to 1–4000 chars (else 422 — the cap intentionally differs from the dashboard's 10000, see [research R13](research.md)). Then **open the transaction first** and, inside it, `SELECT status FROM conversations WHERE tenant_id = $1 AND id = $2 FOR UPDATE` to verify ownership (else 404) and that the status is not `resolved`/`closed` (else 409 `conversation_closed`). This lock is load-bearing: `conversations::queries::add_message_in_tx` **auto-reopens** resolved/closed conversations for customer-kind messages (see `queries.rs` ~line 698), which would violate FR-027 if a dashboard agent resolved the conversation between an unlocked check and the insert. With the lock held, call `add_message_in_tx` with `ConversationActor::Visitor { .. }` and then `conversations::outbox::emit_customer_message_in_tx` — the same pairing used in `backend/crates/modules/conversations/src/routes.rs::add_message`. Return 201 with the stored message view. Apply the per-session message rate limit.
- [x] T030 [US1] Implement the SSE relay `GET /widget/v1/conversations/{conversationId}/events` in `backend/crates/modules/widgets/src/public_events.rs`: authenticate the session and verify conversation ownership, subscribe to the tenant broadcast bus via the escalations presence runtime (`escalations::presence::Runtime::subscribe`, as used in `backend/crates/modules/escalations/src/events.rs`), and implement a `Stream` that **drops every event whose `conversation_id` differs from the requested one**. Map the internal events to the public vocabulary from the contract: `ConversationAi::Delta` → `ai.delta` (payload `{"text": ...}`), `ConversationAi::Completed` → `message.created` with the full message view, agent/human messages → `message.created`, status/handling changes → `conversation.updated`. Attach `KeepAlive` at a 20-second interval, matching the existing `/tenant/events` stream. **Tool-request and internal-note events must never be relayed.**
- [x] T031 [US1] Register the four US1 routes plus the SSE route on the `/widget/v1` scope in `backend/crates/server/src/router.rs` and add their `#[utoipa::path]` registrations to `backend/crates/server/src/openapi.rs`.

### Widget frontend implementation for User Story 1

- [x] T032 [P] [US1] Create `frontend/apps/widget/src/theme/tokens.css` defining the `--wgt-*` token set (surface, text, muted text, border, bubble-visitor, bubble-assistant, radius, spacing scale, font stack, shadow) with light values on `:root` and dark overrides under `:root[data-wgt-theme="dark"]`, plus `--wgt-primary` intended to be set at runtime from tenant config. Self-contained: no imports from `libs/` or Taiga.
- [x] T033 [P] [US1] Create `frontend/apps/widget/src/core/models.ts` with TypeScript interfaces mirroring the contract payloads exactly: `WidgetConfig`, `WidgetConversation`, `WidgetMessage` (`sender: 'visitor' | 'assistant' | 'agent' | 'system'`), `SessionResponse`, and the SSE event union `WidgetEvent`.
- [x] T034 [US1] Create `frontend/apps/widget/src/core/widget-api.service.ts`: an injectable service using `HttpClient` with RxJS operators only, exposing `getConfig(widgetId)`, `createSession(widgetId)`, `getConversation()`, `createConversation()`, `sendMessage(conversationId, body)`. It reads the API base URL from a `WIDGET_API_BASE` injection token, attaches `Authorization: Bearer <token>` from the session store on authenticated calls, and maps HTTP 429 to a typed `RateLimitedError` and 401 to a typed `SessionExpiredError`.
- [x] T035 [US1] Create `frontend/apps/widget/src/core/session.store.ts`: reads/writes the session token under the `localStorage` key `hx_widget_session_<widgetId>`, exposes it as a signal, mints a new session through the API when absent, and clears the stored token when a `SessionExpiredError` surfaces so the next call re-mints. Wrap every `localStorage` access in try/catch so a browser with storage disabled degrades to an in-memory token instead of throwing.
- [x] T036 [US1] Create `frontend/apps/widget/src/core/widget-sse.client.ts`: a fetch-based SSE client (needed because `EventSource` cannot send an `Authorization` header) that streams `GET /widget/v1/conversations/{id}/events`, parses `event:`/`data:` frames incrementally, emits them as an RxJS `Observable<WidgetEvent>`, and reconnects with exponential backoff capped at 30 s. On each reconnect it must emit a resync signal so the caller can re-fetch conversation state.
- [x] T037 [US1] Create `frontend/apps/widget/src/core/widget.store.ts`: the signal-based state machine holding `config`, `conversation`, `messages`, `streamingText`, and `uiState` (`'closed' | 'open' | 'sending' | 'responding' | 'error' | 'rate-limited'`). It appends the visitor's message optimistically on send, accumulates `ai.delta` text into `streamingText`, replaces it with the final message on `message.created`, and clears the responding indicator when streaming starts. Include a **45-second reply timeout**, declared as an exported `const REPLY_TIMEOUT_MS = 45_000`: if neither a delta nor a final message arrives within that window of a send, transition to `'error'` so the retry affordance appears (FR-016 and the "AI takes unusually long" edge case).
- [x] T038 [P] [US1] Create `frontend/apps/widget/src/components/message-bubble.component.ts`: renders one message with distinct styling per `sender` (visitor right-aligned on `--wgt-primary`; assistant and agent left-aligned on `--wgt-bubble-assistant`, agent additionally showing its display name; system centered and muted). Text content must be interpolated, never bound through `innerHTML`.
- [x] T039 [P] [US1] Create `frontend/apps/widget/src/components/typing-indicator.component.ts`: the three-dot animated "assistant is responding" indicator, with `prefers-reduced-motion` respected and `role="status"` for screen readers.
- [x] T040 [US1] Create `frontend/apps/widget/src/components/message-list.component.ts`: takes `messages` and `streamingText` inputs, renders bubbles in chronological order, appends the in-progress assistant bubble while `streamingText` is non-empty, and auto-scrolls to the bottom on new content **only when the user is already near the bottom** (so reading history is not interrupted).
- [x] T041 [US1] Create `frontend/apps/widget/src/components/composer.component.ts`: an auto-growing textarea (max 5 rows) plus send button. Enter sends, Shift+Enter inserts a newline. The send button is disabled for empty/whitespace-only input; input beyond 4000 characters is blocked with a visible counter/message. Emits a `send` output with the trimmed body and preserves typed text if the send fails.
- [x] T042 [US1] Create `frontend/apps/widget/src/components/chat-window.component.ts`: the panel shell (header with tenant display name and close button, message list, composer) that renders the welcome message from config as the first assistant bubble when the conversation has no messages, and shows the error state with a retry action when `uiState` is `'error'` and the friendly slow-down notice when it is `'rate-limited'`.
- [x] T043 [US1] Rewrite `frontend/apps/widget/src/app.component.ts` as the chat-window composition root: read `widgetId` from the iframe URL query string, fetch config, apply `--wgt-primary` and `data-wgt-theme` from it, ensure a session, and render the chat window driven by `widget.store`. **It does not render a launcher** — the loader owns that ([research R1](research.md)). Update `frontend/apps/widget/src/index.html` and `main.ts` to provide `provideHttpClient()`.
- [x] T044 [US1] Create `frontend/apps/widget/loader/loader.ts`, the dependency-free embed loader, per the responsibility table in [research R1](research.md). It must, in order: read `data-widget-id` from its own `<script>` tag (via `document.currentScript`); set a single global guard flag and return immediately if already set (US2-5); `fetch` `GET /widget/v1/config`; **if the response is 404/403 or `enabled` is false, return without touching the host DOM at all** (FR-005 — no iframe is ever injected); otherwise render a launcher button as plain DOM (56 px circle, `primaryColor` background, inline chat glyph SVG, `aria-label="Open chat"`, visible focus ring, fixed in the configured corner with a high `z-index`); on first click inject the iframe at `<origin>/widget/?id=<widgetId>` with `title="Chat widget"`, `border: 0`, transparent background, sized to the chat window; toggle iframe visibility on subsequent launcher clicks. It must define no globals beyond the guard flag and must never throw into the host page — wrap the whole body in try/catch.
- [x] T045 [US1] Implement the iframe↔host `postMessage` protocol on both sides: in `frontend/apps/widget/src/app.component.ts` emit `{ source: 'hx-widget', type: 'resize'|'close', width, height }` to `window.parent` when the window resizes or the visitor closes it; in `frontend/apps/widget/loader/loader.ts` handle those messages, **verifying `event.origin` matches the widget origin and `event.data.source === 'hx-widget'` before acting**, and ignore everything else.
- [x] T046 [P] [US1] Write component tests `frontend/apps/widget/src/components/composer.component.spec.ts` (Enter sends, Shift+Enter does not, empty blocked, >4000 blocked) and `frontend/apps/widget/src/core/widget.store.spec.ts` (deltas accumulate then get replaced by the final message; responding indicator clears when streaming starts; no delta within `REPLY_TIMEOUT_MS` transitions to the error state).
- [x] T047 [US1] Create the e2e host fixture `frontend/e2e/fixtures/widget-host.html` (a plain page including the built loader snippet) and the spec `frontend/e2e/widget-chat.spec.ts` covering the full US1 journey: open launcher → window opens with welcome message → send a message → visitor bubble appears immediately → responding indicator shows → assistant reply renders. Assert that the responding indicator or first reply text appears **within 2 seconds** of the send, covering SC-004.

**Checkpoint**: MVP complete. `cargo test -p server --test widget_conversation_flow`, `pnpm ng test widget`, and `pnpm test:e2e -g widget-chat` all pass.

---

## Phase 4: User Story 2 - Tenant embeds and brands the widget (Priority: P2)

**Goal**: The pasted snippet works on any tenant site, renders that tenant's branding, enforces the domain allowlist, and never disturbs or leaks into the host page.

**Independent Test**: Paste two different tenants' snippets on two host pages; each renders its own branding and reaches only its own tenant's data.

### Tests for User Story 2

- [X] T048 [P] [US2] Write `backend/crates/server/tests/widget_tenant_isolation.rs`: seed two tenants each with an instance; assert tenant A's `widgetId` never returns tenant B's config, a session minted against A cannot read or post to a conversation belonging to B (404, not 403 — do not confirm existence), and the public config response body contains no tenant identifier for either.
- [x] T049 [P] [US2] Write `frontend/e2e/widget-embed.spec.ts` asserting: an invalid `data-widget-id` renders no widget, injects no iframe, and logs no uncaught error into the host page; a disabled instance renders nothing; including the snippet twice yields exactly one launcher.

### Implementation for User Story 2

- [x] T050 [US2] Apply full branding across both halves: in `frontend/apps/widget/loader/loader.ts` use `primaryColor` and `position` for the launcher; in `frontend/apps/widget/src/app.component.ts` and `theme/tokens.css` set `--wgt-primary` from `primaryColor`, `data-wgt-theme` from `theme`, render `displayName` in the header, and use `welcomeMessage` as the first bubble. Both halves fall back to the documented defaults from [data-model.md](data-model.md) when a field is missing.
- [x] T051 [US2] Verify the silent-failure path end-to-end (FR-005). The decision logic lives in the loader (T044) and runs **before** any DOM mutation; this task confirms no iframe, launcher, style tag, or console error reaches the host page for the 404, 403, and `enabled:false` cases, and adds the missing branch if T044 left one incomplete.
- [x] T052 [US2] Harden style isolation (FR-006): confirm the iframe carries `style="all: initial"`-equivalent resets on its host-side element and that no widget CSS is injected into the host document (only inline styles on the iframe/launcher elements the loader creates). Add hostile CSS to `frontend/e2e/fixtures/widget-host.html` (e.g. rules setting `all: unset !important` and aggressive `z-index`/`position` overrides on `iframe, div, button`) and assert in the e2e spec that the widget still renders and remains clickable.
- [x] T053 [US2] Verify and finish allowlist enforcement across all public endpoints in `backend/crates/modules/widgets/src/public_routes.rs` and `public_events.rs`: the `origin_allowed` check from T010 must run on **every** `/widget/v1` route, not just config, so a copied `widgetId` cannot be used from an unlisted origin via direct API calls.
- [x] T054 [US2] Confirm the loader's size budget with real content: `pnpm build:widget-loader` must report the final byte size and stay under the 10240-byte ceiling from T006 now that config fetch and the launcher live in the loader.

**Checkpoint**: US1 and US2 both work. The widget is embeddable, branded, isolated, and origin-restricted.

---

## Phase 5: User Story 3 - Conversation is handed off to a human (Priority: P3)

**Goal**: When the AI escalates, the visitor sees a handoff state (or the away variant), human replies render distinctly, and the AI stops replying.

**Independent Test**: Escalate a live conversation; the widget switches to the handoff state, an agent's dashboard reply appears in the widget, and further visitor messages get no AI reply.

### Tests for User Story 3

- [x] T055 [P] [US3] Write `backend/crates/server/tests/widget_handoff.rs`: after escalation, the public conversation view reports `handling: "human"`; with no available agents it reports `teamOnline: false`; an agent reply is exposed to the widget with `sender: "agent"` and the agent's display name **only** (no email, membership id, or internal note content); a resolved conversation reports `handling: "closed"`.

### Implementation for User Story 3

- [x] T056 [US3] Implement the derived `handling` and `teamOnline` fields in `backend/crates/modules/widgets/src/queries.rs` + `public_routes.rs` per [research.md](research.md) R9: `handling` is `closed` when status is `resolved`/`closed`, `human` when the conversation is escalated/assigned to a human or its `ai_handling` indicates human handling, else `ai`; `teamOnline` comes from `escalations::presence::Runtime::present_membership_ids_async(tenant_id)` being non-empty.
- [x] T057 [US3] Emit `conversation.updated` over the widget SSE stream in `backend/crates/modules/widgets/src/public_events.rs` whenever handling or status changes for the subscribed conversation, carrying `{ "handling": ..., "teamOnline": ... }`.
- [x] T058 [P] [US3] Create `frontend/apps/widget/src/components/handoff-banner.component.ts`: renders the connecting-to-a-human state, and when `teamOnline` is false renders the away variant ("Our team is away — we'll reply as soon as someone is back"). It replaces the typing indicator, never stacks with it.
- [x] T059 [US3] Wire handoff into `frontend/apps/widget/src/core/widget.store.ts` and `components/chat-window.component.ts`: on `handling === 'human'` show the handoff banner (away variant per `teamOnline`), keep the composer enabled so visitor messages still send (FR-021), suppress the AI responding indicator entirely, and **disable the `REPLY_TIMEOUT_MS` timer** — a human taking minutes to reply is not an error.
- [x] T060 [US3] Add the e2e spec `frontend/e2e/widget-handoff.spec.ts` covering: escalation flips the widget to the handoff state; an agent reply renders attributed to the agent and styled distinctly from assistant messages; with all agents unavailable the away variant shows.

**Checkpoint**: Handoff, away, and human-reply rendering all work end-to-end.

---

## Phase 6: User Story 4 - Returning visitor resumes their conversation (Priority: P4)

**Goal**: Reloading or navigating the host site restores the visitor's session and conversation; expired sessions start fresh without errors; closed conversations stay closed.

**Independent Test**: Start a conversation, reload the page, reopen the widget — the prior messages are there and new messages append to the same conversation.

### Tests for User Story 4

- [x] T061 [P] [US4] Write `backend/crates/server/tests/widget_session_lifecycle.rs`: a valid session resolves its existing conversation; a session past `expires_at` returns 401 `session_invalid`; each authenticated call slides `expires_at` forward; a resolved/closed conversation is **not** returned by `GET /widget/v1/conversation`, and posting to it returns 409 `conversation_closed`.

### Implementation for User Story 4

- [x] T062 [US4] Implement resume-on-load in `frontend/apps/widget/src/core/widget.store.ts`: on init, if a stored session token exists, call `GET /widget/v1/conversation` and hydrate messages before the visitor opens the window; if it returns `null`, start clean without error.
- [x] T063 [US4] Implement transparent session recovery in `frontend/apps/widget/src/core/session.store.ts`: on `SessionExpiredError`, clear the stored token, mint a new session, and retry the failed call **once**; if the retry also fails, fall through to the friendly error state rather than looping.
- [x] T064 [US4] Implement the closed-conversation flow (FR-027) in `frontend/apps/widget/src/core/widget.store.ts` and `components/chat-window.component.ts`: on `handling === 'closed'` show a one-time "This conversation has ended" note and, when the visitor sends the next message, create a **new** conversation first and post the message to it. Closed conversations must not be re-displayed on a later reload.
- [x] T065 [US4] Add the expired-session sweep: call `queries::delete_expired_sessions` from a periodic background task registered where the other workers are started (see how the agent responder worker is spawned in `backend/crates/server/src/main.rs` / `lib.rs`), running every 6 hours with errors logged and swallowed so a failure never kills the task.
- [x] T066 [US4] Add the e2e spec `frontend/e2e/widget-session.spec.ts`: send a message, reload the host page, reopen the widget, assert prior messages are present and a new message appends to the same conversation.

**Checkpoint**: Session continuity and closure semantics behave per spec.

---

## Phase 7: User Story 5 - Tenant manages widget settings in the dashboard (Priority: P5)

**Goal**: Tenants create and manage multiple widget instances with live preview, copyable snippets, and conversation attribution in the inbox.

**Independent Test**: Create two instances with different branding, watch the preview track edits, and confirm each snippet renders its own configuration on separate host pages.

### Tests for User Story 5

- [X] T067 [P] [US5] Write `backend/crates/server/tests/widget_admin_crud.rs`: full create/list/get/update/delete cycle; `publicId` is generated server-side and is immutable across updates; soft-deleted instances disappear from list **and** from the public config endpoint; validation failures return 422 with field details; a user holding only `WidgetsView` gets 403 on writes while `WidgetsManage` succeeds. Also assert (FR-018) that a conversation created through the widget appears in `GET /tenant/conversations` with `channel: "widget"` and its `widgetInstance` populated.

### Backend implementation for User Story 5

- [X] T068 [US5] Implement `public_id` generation in `backend/crates/modules/widgets/src/queries.rs`: `generate_public_id() -> String` producing `"wgt_"` plus 22 random base62 characters.
- [X] T069 [US5] Implement the admin CRUD handlers in `backend/crates/modules/widgets/src/admin_routes.rs` for `GET /tenant/widgets`, `POST /tenant/widgets`, `GET /tenant/widgets/{id}`, `PUT /tenant/widgets/{id}`, `DELETE /tenant/widgets/{id}` (soft delete) per [contracts/widget-admin-api.md](contracts/widget-admin-api.md), using `tenancy::TenantContext` for tenant scoping and the T008 validators.
- [X] T070 [US5] Implement `GET /tenant/widgets/{id}/snippet` in `backend/crates/modules/widgets/src/admin_routes.rs`, returning the exact snippet string from the contract built from the configured public host (`public_dashboard_url`).
- [X] T071 [US5] Implement audit records in `backend/crates/modules/widgets/src/audit.rs` for instance create/update/delete, following `backend/crates/modules/conversations/src/audit.rs` and the append-only audit conventions (constitution III).
- [X] T072 [US5] Register the admin routes in `backend/crates/server/src/router.rs` inside the tenant scope with `require_permission(Permission::WidgetsView)` on reads and `Permission::WidgetsManage` on writes, matching how the knowledge-base routes are gated; add matching `/test/tenant/widgets/view|manage` probe routes to the `include_test_routes` block so `cargo test -p server --test rbac` covers them.
- [X] T073 [US5] Add `widgetInstance: { id, name } | null` to the conversation list and detail responses in `backend/crates/modules/conversations/src/model.rs` and the queries in `queries.rs` (LEFT JOIN on `widget_instances`) so widget conversations are attributable in the inbox (FR-032). The join must not introduce an N+1 — resolve it in the existing list query.

### Dashboard implementation for User Story 5

- [x] T074 [P] [US5] Add `'widgets.view'` and `'widgets.manage'` to the `Permission` union and a `PAGE_PERMISSIONS` entry for the new path in `frontend/apps/dashboard/src/app/core/authz/permissions.ts`.
- [x] T075 [P] [US5] Add `widgets: 'widgets'` to the tenant paths in `frontend/apps/dashboard/src/app/core/router/app-paths.ts` and a corresponding entry in `frontend/apps/dashboard/src/app/core/router/page-title.ts` (title "Chat Widget", subtitle describing embedding the widget on a website).
- [x] T076 [P] [US5] Create `frontend/apps/dashboard/src/app/core/api/widget.models.ts` with `WidgetInstance`, `CreateWidgetInstancePayload`, `UpdateWidgetInstancePayload`, and `WidgetSnippet` interfaces matching [contracts/widget-admin-api.md](contracts/widget-admin-api.md).
- [x] T077 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/widgets/widget-api.service.ts` wrapping the five CRUD calls plus the snippet endpoint, copying the structure of `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-api.service.ts`.
- [x] T078 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/widgets/widgets.store.ts` as an NgRx SignalStore holding the instance list, the selected instance, the in-progress form state (so the preview can bind to unsaved edits), and loading/error flags.
- [x] T079 [P] [US5] Create `frontend/apps/dashboard/src/app/features/tenant/widgets/widget-preview.component.ts`: renders a non-interactive replica of the widget launcher and window using the same `--wgt-*` token names, bound to the **unsaved** form state so edits appear immediately (FR-030). It must not call the API or embed an iframe.
- [x] T080 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/widgets/widget-editor.component.ts`: the settings form (name, display name, primary color, welcome message, position, theme, enabled toggle, allowed-domains list editor with add/remove) with client-side validation mirroring the server rules from T008, rendered beside the live preview.
- [x] T081 [US5] Create `frontend/apps/dashboard/src/app/features/tenant/widgets/widgets.component.ts`: the page shell listing instances with create/edit/delete actions, hosting the editor + preview, and showing the copyable embed snippet with a copy-to-clipboard control (FR-033). Delete must ask for confirmation before calling the API.
- [x] T082 [US5] Register the route in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` (lazy `loadComponent`, `canMatch: [permissionGuard]`, `requiredPermission` from `PAGE_PERMISSIONS`) and add the sidebar nav entry in `frontend/apps/dashboard/src/app/layout/sidebar/sidebar.component.ts` following the knowledge-base entry.
- [x] T083 [US5] Surface widget attribution in the inbox: display the originating widget instance name on widget-channel conversations in `frontend/apps/dashboard/src/app/features/tenant/conversations/` (list row and detail header), reading the new `widgetInstance` field from T073.
- [x] T084 [P] [US5] Write `frontend/apps/dashboard/src/app/features/tenant/widgets/widgets.store.spec.ts` and `widget-preview.component.spec.ts` covering list/create/update flows against a mocked API and preview reactivity to unsaved form edits.

**Checkpoint**: All five user stories functional. Tenants can self-serve the entire widget lifecycle.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T085 [P] Add accessibility passes to the widget: keyboard trap inside the open window, Escape closes it, focus returns to the launcher on close, `aria-live="polite"` on the message list so new messages are announced, and a documented contrast check of default token values in `frontend/apps/widget/src/theme/tokens.css`.
- [X] T086 [P] Add structured tracing spans to the widgets module (`backend/crates/modules/widgets/src/`) for config lookup, session mint, message send, and SSE subscribe/drop counts, following the `tracing` usage in `backend/crates/modules/escalations/src/events.rs` (constitution VI).
- [X] T087 [P] Write the module documentation `backend/crates/modules/widgets/README.md` covering Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, and Extension Points, as every module must (constitution "Documentation & Future Readiness"). Note the anonymous-actor extension from T011–T013 under Dependencies.
- [X] T088 Update `frontend/CLAUDE.md`: the rule stating `apps/widget` is prior scaffolding that must not be modified is superseded by this feature — replace it with the widget app's actual conventions (standalone Angular, no `libs/*`, no Taiga, no NgRx, own `--wgt-*` tokens, 97 KB budget, loader build script, and the loader-vs-app responsibility split).
- [X] T089 Verify the size budgets hold with real content: `pnpm ng build widget` must stay within the configured 97 KB initial budget and `pnpm build:widget-loader` under 10240 bytes. If the widget bundle exceeds budget, reduce bundle weight (e.g. drop unused Angular features) rather than raising the budget.
- [X] T090 Run the full quality gate from `frontend/`: `pnpm ng build widget && pnpm ng build dashboard && pnpm ng test widget && pnpm ng test dashboard && pnpm test:e2e && pnpm lint && pnpm format:check`, and from `backend/`: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Fix everything that fails.
- [X] T091 Execute every scenario in [quickstart.md](quickstart.md) manually against a running stack and confirm each expected outcome, including the cross-cutting tenant-isolation and hostile-CSS checks.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately.
- **Phase 2 (Foundational)**: Depends on Phase 1. **BLOCKS all user stories.** Within it, T011–T013 (cross-module extensions) block T026/T028/T029.
- **Phase 3 (US1)**: Depends on Phase 2. This is the MVP.
- **Phase 4 (US2)**: Depends on Phase 2; T050–T052 build on US1's loader and components, so run after Phase 3.
- **Phase 5 (US3)**: Depends on Phase 2 + US1's SSE stream and store (T030, T037).
- **Phase 6 (US4)**: Depends on Phase 2 + US1's session store and conversation endpoints.
- **Phase 7 (US5)**: Depends on Phase 2 only for the backend CRUD (T068–T073) — the dashboard work is genuinely independent of US1–US4 and can proceed in parallel with them.
- **Phase 8 (Polish)**: Depends on all desired stories being complete.

### Critical path

T001 → T002 → T011 → T012 → T013 → T014 → T015 → T018 → T019/T020 → T021 → T026 → T028 → T029 → T030 → T034 → T037 → T043 → T044 (embedded widget holding an AI conversation).

### Parallel Opportunities

- **Phase 1**: T004, T005, T006 in parallel (different files) after T001–T003.
- **Phase 2**: T007–T010 all in parallel; T011/T012 are the same file (`conversations/src/queries.rs`) so they are **sequential**; T013 parallel with them; T016/T017 (server crate) parallel with T014/T015 (widgets crate).
- **Phase 3**: T024 and T025 (tests) in parallel; T032, T033 in parallel; T038, T039 in parallel; T046 parallel with T047.
- **Cross-story**: once Phase 2 is done, one implementer can take US5's backend + dashboard (T067–T084) while another takes US1 → US2 → US3 → US4, since they touch disjoint files apart from the shared `public_routes.rs`.

## Parallel Example: User Story 1 widget components

```bash
# After T037 (store) lands, these two components have no shared files:
Task: "Create message-bubble.component.ts in frontend/apps/widget/src/components/"
Task: "Create typing-indicator.component.ts in frontend/apps/widget/src/components/"
```

## Implementation Strategy

### MVP first (User Story 1 only)

1. Phase 1: Setup (T001–T006)
2. Phase 2: Foundational (T007–T023) — **do not skip; everything depends on it**
3. Phase 3: User Story 1 (T024–T047)
4. **STOP and VALIDATE**: run quickstart.md Scenario 2 end-to-end
5. Demo: a real AI conversation through an embedded widget

### Incremental delivery

1. Setup + Foundational → security and data layer ready
2. + US1 → **MVP**: embeddable widget with streamed AI replies
3. + US2 → branding, isolation, allowlist: safe for real tenant sites
4. + US3 → human handoff and away states
5. + US4 → session continuity and conversation closure
6. + US5 → full self-serve dashboard management
7. + Polish → accessibility, observability, docs, full quality gate

## Notes

- **Never** trust a client-supplied tenant id on `/widget/v1` routes. Tenant always derives from the widget instance or the authenticated session row (FR-025, constitution II).
- **Never** write to another module's tables from `crates/modules/widgets/`. T011–T013 exist precisely so you don't have to (constitution I, [research R12](research.md)).
- The AI reply path involves **no new AI code** — inserting the message plus the `conversation.customer_message` outbox event in one transaction is the entire integration ([research R7](research.md)). If replies are not appearing, check that the agent responder worker is running, not that the widget needs AI logic.
- Message-kind mapping to the public vocabulary is a security boundary: internal `note` messages must never reach the widget (FR-024).
- Commit after each task or small logical group. Stop at any checkpoint to validate a story independently.

---

## Phase 9: Convergence

**Why these exist**: T001–T091 are all marked complete, and the large majority genuinely are (migration, widgets crate, conversation lock/409, cross-tenant 404, session expiry, dashboard settings, loader budget at 2974/10240 bytes — all verified present and correct). The tasks below are the specific deltas where the code does **not** match the completed task, confirmed by direct inspection. They are not restatements of finished work.

Two of these are security gates the plan's Constitution Check (Principle III) explicitly relied on to justify shipping unauthenticated public endpoints. Do them first.

- [X] T092 **CRITICAL** Mount and complete the widget rate limiter per FR-022/FR-023/SC-007 and constitution III (partial). `widget_rate_limit_middleware` in `backend/crates/server/src/rate_limit.rs` is fully defined but **never referenced** by `backend/crates/server/src/router.rs` — `widget_routes()` (~line 98) applies only `widget_cors_layer()`, so every `/widget/v1` endpoint is currently unthrottled. Three deltas from T016/T017/T021: (a) apply the middleware to the `/widget/v1` scope where the CORS layer is attached (~line 900); (b) implement the two missing budgets — messages 10/min keyed by **session id** and a global 600/min bucket keyed by **tenant id** — whose consts `MESSAGES_PER_SESSION_LIMIT` and `GLOBAL_TENANT_LIMIT` are currently dead code, with only the per-IP creation bucket wired; (c) replace the private `LazyLock` `GLOBAL_STORE` with a store held in `AppState` per T017 so it is shared and testable. Note the per-IP key falls back to the literal `"unknown"` when `x-forwarded-for` is absent, which collapses all such callers into one bucket — derive the peer address instead.
- [X] T093 **CRITICAL** Enforce the domain allowlist on the SSE stream per FR-026 (missing). `origin_allowed` is called three times in `backend/crates/modules/widgets/src/public_routes.rs` but **zero** times in `public_events.rs`; `stream_events` (~line 157) authenticates the session and nothing else. T053 required the check on *every* `/widget/v1` route precisely so a copied `widgetId` cannot be used from an unlisted origin — the event stream is currently that hole. Load the instance's `allowed_domains` and reject with 403 `origin_not_allowed` before subscribing to the bus.
- [X] T094 Fix the widget integration-test assertions that would fail if executed, per constitution VII (contradicts). `backend/crates/server/tests/widget_public_foundation.rs:199-200` asserts `body["widget_id"]` and `body["display_name"]`, and `widget_tenant_isolation.rs:155,168,181` assert `body["widget_id"]` — but every public DTO in `backend/crates/modules/widgets/src/model.rs` carries `#[serde(rename_all = "camelCase")]`, so the wire keys are `widgetId` and `displayName`. These comparisons evaluate against `Value::Null`. Correct them to the camelCase keys and confirm the contract in [contracts/public-widget-api.md](contracts/public-widget-api.md) is what is actually asserted.
- [X] T095 Replace the two vacuous tests in `backend/crates/server/tests/widget_public_foundation.rs` with real assertions, per constitution VII and T018 (contradicts). `t006_expired_session_returns_401_session_invalid` seeds an expired session then asserts nothing at all, ending on `// TODO: … will pass once T015 is implemented` — the behavior **is** correctly implemented at `backend/crates/modules/widgets/src/session.rs:47`, so this is a coverage gap, not a functional one: assert the 401 and the `session_invalid` code. `t007_exceeding_per_ip_creation_limit_returns_429` returns early when it sees a 429 and otherwise falls off the loop with no assertion, so it passes unconditionally; after T092 lands, assert that the request past the budget returns 429 with code `rate_limited`. Both tests must be shown to fail before T092's fix and pass after.
- [X] T096 Restore a compiling backend test suite so the T090 quality gate can actually run (contradicts). `cargo check -p server --tests` currently fails with `error[E0425]: cannot find value TEST_ENV in this scope` at `backend/crates/server/tests/engine_tool_approval_race.rs:13`, which means `cargo test` cannot execute for the whole `server` crate — T090 could not have passed as marked. Repair the missing `TEST_ENV` binding, then re-run the full backend gate (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`).
- [X] T097 Execute the widget backend integration suite against a real database and fix what surfaces (partial). Every widget test file gates on `DATABASE_URL` via `get_pool()` and silently returns when it is unset (repo convention, matching `conversations.rs`), so none of `widget_public_foundation`, `widget_conversation_flow`, `widget_tenant_isolation`, `widget_handoff`, `widget_session_lifecycle`, or `widget_admin_crud` has demonstrably run — which is how T094's key mismatch survived. After T094–T096, run them with `DATABASE_URL` set and `REQUIRE_DB_TESTS=1` so a missing database fails loudly rather than skipping, and fix every genuine failure that appears.
- [X] T098 Restore the widget app's 97 KB initial bundle budget per plan.md ("widget app within its configured 97 KB initial budget"), constitution X, and T089 (contradicts). T089 stated the budget must hold and that an over-budget bundle should be fixed by **reducing bundle weight rather than raising the budget** — the budget was raised instead. `frontend/angular.json` now configures the `widget` project's `initial` budget as `maximumWarning: 200kb` / `maximumError: 250kb`, and `pnpm ng build widget` currently emits **173.56 kB raw** (49.73 kB transfer), passing only because of the raised ceiling. Restore the 97 KB initial error budget and bring the bundle under it by trimming bundle weight. If 97 KB proves genuinely unreachable for a functioning Angular widget, do **not** silently re-raise it — record the deviation and its justification in plan.md's Complexity Tracking and flag it for a decision.
- [X] T099 Eliminate the unhandled rejection raised by `frontend/apps/dashboard/src/app/features/tenant/widgets/widgets.store.spec.ts`, per constitution VII (partial). `pnpm ng test dashboard` reports 964 passing tests but **3 unhandled errors**, one originating in this spec — the `{ message: 'Failed to load', code: 'ERR', status: 500 }` object errored into the list subject at spec line ~118 escapes as an uncaught exception, and Vitest warns this "might cause false positive tests". The assertions on `store.loading()`/`store.error()` do pass, so the store's error state works; the leak is the un-consumed error from the stubbed `new Subject()` handed to `mockApi.list` at line ~110 before the store is reconfigured. Fix the spec (and the store's subscription teardown if it is the true source) so the suite runs with zero unhandled errors. The other two errors come from `knowledge-base` specs and belong to feature 019 — out of scope here, do not modify them.

---

## Phase 10: Convergence

**Why this exists**: Phase 9's T093–T099 were verified genuinely complete this round — the SSE origin check now gates with a real 403 (`public_events.rs:191`), the camelCase assertions are fixed, `t006`/`t007` carry real assertions, the backend test suite compiles, the widgets unhandled rejection is gone, and T098 correctly took its documented escape hatch (the bundle deviation is recorded in plan.md's Complexity Tracking). T092 was mounted but only partially implemented; the single task below is that remaining delta, not a restatement of T092.

- [X] T100 **CRITICAL** Implement the per-session and per-tenant rate-limit dimensions required by FR-022 and SC-007 (partial, completing T092). T092 is marked complete and its first part landed — `widget_rate_limit_middleware` is now mounted on the widget scope in `backend/crates/server/src/router.rs` (~line 900). But the only bucket that exists is keyed by **client IP**, and FR-022 requires limits "per visitor session and per tenant" while SC-007 requires tenant budgets to be independent. Neither required dimension is implemented: `MESSAGES_PER_SESSION_LIMIT`, `GLOBAL_TENANT_LIMIT`, and both their window consts in `backend/crates/server/src/rate_limit.rs` have **zero usages** anywhere outside their own `pub const` declarations (verified by grep across `backend/crates/`).

  **Why the shortcut happened, so it is not repeated**: a session id and a tenant id are simply not knowable in the current blanket pre-auth middleware, which runs before any session lookup. These two budgets cannot live there. Apply the per-session limit (10/min keyed by `session.id`) inside the `send_message` handler in `backend/crates/modules/widgets/src/public_routes.rs` **after** `authenticate_session` returns the session row, and apply the global 600/min per-tenant bucket keyed by the `tenant_id` resolved from the session or the widget instance. Return `kernel::ApiError::rate_limited(...)` (429, code `rate_limited`) on rejection, matching the existing middleware.

  **Likely user-facing consequence to confirm while fixing** (inferred from the code, not yet observed by driving the widget): the mounted middleware applies `SESSION_CREATION_LIMIT` — 10/min — to *every* `/widget/v1` route, including `messages` and the SSE stream, not just session creation. A single ordinary visit spends roughly five requests before the first message (loader config fetch, app config fetch, session mint, conversation fetch, conversation create, SSE connect), so a visitor sending a handful of messages in their first minute would be rejected with 429, and every visitor behind one NAT/corporate IP shares that bucket. Give the creation limit its own key/route scope rather than letting it govern the whole surface, and verify a realistic multi-message conversation is not throttled.

  **Minor, same file**: T092's third part asked for the limiter store to live in `AppState`. It is still two `LazyLock` statics with a `shared_store()` / `test_store()` accessor pair — the separate `test_store()` exists precisely because a process-global store cannot be isolated per test. Move it into `AppState` as originally specified, or record why the static is preferred. Keep the four existing unit tests in `rate_limit.rs` passing.
