# Research: Website Chat Widget

All Technical Context unknowns resolved. Decisions below were made against the existing codebase (modules surveyed: `conversations`, `customers`, `escalations`, `ai`, `server/router.rs`) and the constitution v1.2.0.

## R1. Embed architecture: loader script + iframe

**Decision**: A dependency-free loader (`widget.js`, ≤10 KB) is what tenants paste. Responsibilities are split as follows, and this split is authoritative:

| Concern | Owner |
|---|---|
| Read `data-widget-id`, guard against double-inclusion | Loader |
| Fetch `GET /widget/v1/config` | Loader (and, independently, the Angular app) |
| Silent failure on 404 / 403 / `enabled:false` — no iframe is ever injected | Loader |
| Launcher button (plain DOM, inline styles, positioned and colored from config) | Loader |
| Inject / show / hide / resize the iframe | Loader |
| Everything inside the chat window (header, message list, composer, states) | Angular app in the iframe |

**Rationale**: An iframe gives hard style/JS isolation both directions (FR-006) — host CSS cannot corrupt the widget and vice versa — and is the pattern proven by Intercom/Zendesk/Crisp. Putting config fetch **and** the launcher in the loader is what makes FR-005 truly silent: an invalid or disabled widget injects no iframe at all, rather than injecting one and retracting it after a visible beat. It also lets the launcher be positioned correctly on first paint with no round-trip through the frame.

The Angular app fetches config a second time rather than receiving it by `postMessage`. This is deliberate: the endpoint is public, tiny, and cacheable, and one redundant request is far cheaper than a handshake protocol that a small model would have to get exactly right on both sides.

**Alternatives considered**: Shadow-DOM web component (weaker isolation: inherited CSS custom properties, host-page CSP/JS conflicts, shared main thread globals); rendering the whole app inline (no isolation, bundle collides with host frameworks).

## R2. Widget UI: build out the existing `apps/widget` Angular app

**Decision**: Implement the chat UI in the already-scaffolded `frontend/apps/widget` Angular application (standalone components, OnPush, signals; no Taiga UI, no NgRx, no dashboard libs). Own minimal token sheet `--wgt-*` with light/dark themes and a tenant-injected primary color. Keep within the preconfigured 97 KB initial budget.

**Rationale**: The constitution mandates the Angular/TS stack; the workspace already has the app and budget wired (`angular.json` → `dist/widget`). Excluding Taiga/dashboard libs keeps the bundle small and honors the frontend rule that `libs/*` scaffolding stays out of new work.

**Alternatives considered**: Preact/vanilla micro-app (smaller, but a stack deviation requiring Complexity Tracking justification and a second toolchain); reusing dashboard design-system tokens (pulls Taiga-adjacent weight into a third-party bundle).

## R3. Anonymous session: opaque bearer token, hashed at rest

**Decision**: `POST /widget/v1/sessions` mints a 256-bit random opaque token, returned once; the DB stores only its SHA-256 hash in `widget_sessions` with `tenant_id`, `widget_instance_id`, `expires_at` (24 h inactivity sliding window, refreshed on use — FR-010). The iframe stores the token in its own origin's `localStorage` and sends it as `Authorization: Bearer` on every public call.

**Rationale**: Opaque+hashed matches the platform's credential hygiene (no secrets recoverable from DB), supports revocation/expiry trivially, and avoids third-party-cookie blocking entirely (cookies set inside a third-party iframe are blocked/partitioned by modern browsers; `localStorage` in the iframe is partitioned per top-level site, which exactly matches FR-008's "same browser, same site" continuity and adds a privacy benefit).

**Alternatives considered**: JWT (stateless but revocation/expiry-refresh awkward; no benefit at this scale); httpOnly cookie (broken by third-party cookie policies inside iframes); fingerprinting (privacy-hostile, unreliable).

## R4. Tenant identification & origin enforcement

**Decision**: Public widget identifier is a dedicated URL-safe public ID (`wgt_` + 22 random base62 chars) on `widget_instances`, distinct from the row UUID. Every public endpoint resolves instance → tenant server-side; no client-supplied tenant ID is ever read on the public surface (FR-025). When `allowed_domains` is non-empty the server validates the `Origin` header (fallback `Referer`) against it — exact host or `*.` wildcard subdomain match — returning 403 with the standard error envelope; the widget then stays silent (FR-005). CORS for `/widget/v1/*` allows any origin (`Access-Control-Allow-Origin: *`, no credentials — the bearer header carries auth), since allowlisting is enforced explicitly per instance.

**Rationale**: A prefixed public ID is recognizable in support/log contexts and leaks nothing; explicit origin checks make the allowlist enforceable server-side (Q1 clarification) where CORS alone would only protect browsers.

**Alternatives considered**: Using the instance UUID directly (works, but indistinguishable from internal IDs in logs and unpleasant in snippets); signed embed tokens per domain (heavier setup, contradicts 5-minute embed goal).

## R5. Rate limiting: in-process token bucket, tower layer

**Decision**: New `server/src/rate_limit.rs` tower layer applied only to the `/widget/v1` scope. Keys and default budgets: per widget session (messages: 10/min burst 5), per IP for session/conversation creation (10/min), and a global bucket keyed by **tenant** (600 req/min) — constants in one place, overridable via config.

The global bucket is keyed by tenant rather than by widget instance so that FR-022 ("per visitor session and per tenant") is satisfied literally: a tenant with five widget instances gets one shared budget, not five. Tenant budgets are independent of each other, which is what SC-007 guarantees. Counters in an in-process concurrent map with periodic sweep. Emits 429 via the existing `ApiError::rate_limited` envelope (already in `kernel`); widget maps it to the friendly slow-down state (FR-023).

**Rationale**: The repo has no rate limiting today and no Redis usage yet; an in-process limiter is sufficient for a single-process modular monolith, keeps the layer self-contained, and its interface (a `RateLimitStore` trait) is the seam for a Redis-backed implementation when the monolith is scaled out.

**Alternatives considered**: `tower-governor` (new dependency for behavior a ~150-line layer covers, less control over keying by body-derived session); Redis-backed now (introduces first Redis runtime dependency for no current multi-process need — noted as the extraction path).

## R6. Realtime to the visitor: per-conversation public SSE

**Decision**: `GET /widget/v1/conversations/{id}/events` (session-authorized) subscribes to the existing tenant broadcast bus (the one behind `GET /tenant/events`, spec 014) and relays only events for that conversation, mapped to a minimal public vocabulary: `message.created` (agent/AI/system messages), `ai.delta` (engine's existing ~4/s streamed chunks), `conversation.updated` (status/handling changes → handoff, away, closed), plus SSE keep-alive every 20 s. Visitor's own sends are echoed synchronously in the POST response, not over SSE.

**Rationale**: The bus, the outbox→bus relay poller, and the AI engine's delta broadcasting already exist — the widget only needs a filtered, sanitized view. SSE matches the platform's chosen realtime mechanism and the fetch-based client pattern already proven in `libs/realtime`.

**Alternatives considered**: WebSockets (bidirectional not needed — sends are plain POSTs; new infra); polling (violates SC-003/SC-004 latency and wastes rate-limit budget).

## R7. AI reply generation & streaming: zero new AI code

**Decision**: Visitor message send inserts through the conversations module (sender = customer participant) and emits the `conversation.customer_message` outbox event in the same transaction — exactly what the dashboard path does. The existing agent responder worker claims it, runs the engine (RAG, tools, confidence, escalation rules intact), streams deltas to the bus (relayed to the widget via R6), and persists the final AI message. Handoff decisions flow from the same engine/escalation rules (014/021).

**Rationale**: Constitution IV + I: the AI subsystem stays behind its existing event interface; the widget is purely a new message source. This also guarantees dashboard/widget behavior parity (FR-013, FR-018, FR-019).

**Alternatives considered**: A synchronous public "chat completion" endpoint calling the engine directly (duplicates orchestration, bypasses escalation bookkeeping, holds public HTTP connections through provider latency).

## R8. Visitor identity & conversation attribution in existing modules

**Decision**: Each widget session creates (lazily, on first conversation) an anonymous `customers` row — display name `Visitor <short-code>`, no email/phone — linked via a `widget` channel identifier equal to the session public reference. Conversations are created with `channel = 'widget'` and a new nullable `conversations.widget_instance_id` FK for per-instance attribution (FR-032). Closed/resolved conversations are locked for the session (FR-027): the widget's "current conversation" resolver returns the newest non-closed conversation or none.

**Rationale**: Reuses the customers/conversations data model as designed (channel identifiers exist precisely for this); a real FK column beats metadata for the dashboard filter/attribution query paths (constitution VIII: indexed production paths).

**Alternatives considered**: A parallel `widget_conversations` table (violates single-inbox requirement FR-018); storing attribution in message metadata (unqueryable).

## R9. Handoff & away state derivation

**Decision**: The public conversation view exposes `handling: ai | human | closed` derived from existing `ai_handling`/status fields, plus `team_online: bool` computed from the escalations presence runtime (present membership count > 0 for the tenant). The widget renders: AI mode (default), handoff waiting, handoff-away variant when `team_online = false` (Q4 clarification), ended state when closed (Q2 clarification). While handed off, the AI responder already skips AI-handled=human conversations — no new suppression logic.

**Rationale**: All signals already exist (014's presence runtime and 021's `ai_handling`); the widget only needs them surfaced in sanitized form (agent display name only — FR-024).

**Alternatives considered**: Business-hours schedules (not in scope; presence is the live source of truth today).

## R10. Dashboard settings & permissions

**Decision**: New `Permission::WidgetsView` / `Permission::WidgetsManage` in the authz module, granted per existing role-mapping conventions (Owner/Admin manage; Manager view). Settings UI is a tenant feature area `features/tenant/widgets/` in the dashboard: instance list, editor form (name, color, welcome message, position, theme, enabled, allowed domains), copyable per-instance snippet, and a live preview rendering the actual widget window component styles inside the page (driven by the form state, no iframe round-trip). Instance changes are audited (constitution III).

**Rationale**: Dedicated permissions keep RBAC explicit rather than overloading `SettingsManage`; live preview reuses the widget's token sheet so preview ≡ reality (Q5 full-settings-experience clarification).

**Alternatives considered**: Reusing `SettingsManage` (cheaper but blurs auditability of a customer-facing surface); iframe-based preview of the real app (heavier, build-order coupling; can be a later enhancement).

## R11. Loader build & serving

**Decision**: `apps/widget/loader/loader.ts` is bundled by a small esbuild script (`pnpm build:widget-loader`) into `dist/widget/widget.js` (IIFE, no imports). In production both `widget.js` and the widget app are served as static assets under the platform's public host; the embed snippet is `<script src="https://<host>/widget.js" data-widget-id="wgt_..." async></script>`. Local dev serves the built loader from the Angular dev server's assets.

**Rationale**: esbuild ships inside the Angular toolchain already (no new dependency); an IIFE loader with zero imports is the only way to hit ≤10 KB and be safe on arbitrary pages.

**Alternatives considered**: Making the loader an extra Angular build entry (Angular runtime overhead defeats the size goal); hand-maintained prebuilt JS checked into the repo (drifts, unreviewable).

## R12. Anonymous actor on conversation writes (blocking interface gap)

**Decision**: Extend the `conversations` and `customers` modules with explicitly anonymous-safe entry points, and have the widgets module call **only** those. Three concrete changes:

1. `conversations::queries::create_conversation_in_tx` and `add_message_in_tx` currently require `actor_user_id: Uuid` (and, for creation, `actor_membership_id: Uuid`) — a staff identity a widget visitor does not have. Introduce an `Actor` enum (`Actor::Staff { user_id, membership_id }` / `Actor::Visitor { customer_id }`) or sibling `*_as_visitor_in_tx` functions, so visitor-origin writes are representable without inventing a fake user.
2. `create_conversation_in_tx` gains a `widget_instance_id: Option<Uuid>` parameter so FR-032 attribution can be persisted at creation time.
3. `customers` exposes a public `create_anonymous_customer_in_tx(...)`. Today the only customer INSERT lives inside an HTTP handler in `customers/src/routes.rs`, and the module's public surface is just `customer_exists`/`customer_exists_in_tx`.

**Rationale**: Constitution Principle I forbids direct cross-module data access — the widgets module must not INSERT into `conversations`, `messages`, or `customers` itself. Without these three extensions the implementer's only options are to pass `Uuid::nil()` as the actor (which silently poisons audit records, violating Principle III) or to duplicate another module's SQL (violating Principle I). Both are worse than widening the interface deliberately.

**Alternatives considered**: A dedicated "system user" row per tenant to act as the visitor's proxy (pollutes the users table and misattributes audit entries to a fake human); allowing `Uuid::nil()` by convention (unenforceable, and invisible in review).

## R13. Widget message length cap of 4000 characters

**Decision**: The public widget endpoint caps message bodies at 4000 characters, while the authenticated dashboard endpoint keeps its existing 10000-character cap.

**Rationale**: The two limits protect different things. The dashboard limit exists so staff can paste long replies; the widget limit is an abuse-surface control on an unauthenticated endpoint, where every accepted byte is attacker-controlled and feeds an LLM prompt. 4000 characters is far above any genuine support question. The divergence is intentional — do not "fix" it by aligning the two.

**Security note — deliberate omission of Subresource Integrity**: The snippet does not carry an `integrity` hash. SRI pins one exact file version, but the loader is an evergreen first-party asset the platform must update without every tenant re-pasting their snippet (the same reason Stripe.js documentation forbids SRI on its embed). Compensating controls: the script is served over HTTPS from the platform's own origin (not a third-party CDN), the loader is tiny and auditable, it injects only same-origin resources, and the chat surface itself runs in the platform-origin iframe — a compromised host page never gains access to session tokens or conversation data.
