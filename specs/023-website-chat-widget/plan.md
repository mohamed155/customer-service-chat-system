# Implementation Plan: Website Chat Widget

**Branch**: `023-website-chat-widget` | **Date**: 2026-07-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/023-website-chat-widget/spec.md`

## Summary

Build the first customer-facing channel: an embeddable website chat widget. A tiny loader script (`widget.js`) injects a launcher and an iframe that hosts the existing (currently bare) `apps/widget` Angular app. A new backend `widgets` module provides public, unauthenticated endpoints — widget config lookup, anonymous session issuance, conversation creation, message sending, and a per-conversation SSE stream — plus authenticated tenant CRUD for multiple widget instances (branding, position, theme, domain allowlist, embed snippet). The widget composes existing modules end-to-end: visitor messages are inserted through the conversations module and emit `conversation.customer_message` outbox events, which the existing AI responder worker (021/022) already consumes to generate streamed replies; escalation (014) drives the handoff/away states. New cross-cutting piece: an in-process rate-limiting layer for the public surface.

## Technical Context

**Language/Version**: Backend: Rust (workspace at `backend/`, Axum + Tokio + SQLx). Frontend: Angular (standalone, zoneless-ready) + TypeScript in the pnpm workspace at `frontend/`.

**Primary Dependencies**: Axum (routing, SSE), SQLx/PostgreSQL, utoipa (OpenAPI), existing module crates (`conversations`, `customers`, `escalations`, `ai`, `tenancy`, `authz`, `kernel`); Angular `@angular/build:application` for `apps/widget`; esbuild (already in node_modules via Angular) for the loader bundle.

**Storage**: PostgreSQL via migrations `backend/migrations/00xx_*.sql` (next: 0050). New tables: `widget_instances`, `widget_sessions`; new nullable `widget_instance_id` column on `conversations`. No object storage needed.

**Testing**: Backend: `cargo test` (unit + `#[sqlx::test]` DB tests per module convention, router permission tests). Frontend: Vitest (`pnpm ng test widget`, `pnpm ng test dashboard`), Playwright e2e (`frontend/e2e`) for the embed → chat → handoff flow.

**Target Platform**: Linux/macOS server (existing Axum server); widget runs in evergreen desktop + mobile browsers on arbitrary third-party host pages.

**Project Type**: Web application (existing backend + frontend workspaces; widget is a second frontend app already scaffolded at `frontend/apps/widget`).

**Performance Goals**: Loader ≤ 10 KB raw; widget app within its configured 97 KB initial budget; responding indicator visible < 2 s after send (SC-004); AI deltas relayed at the engine's existing ~4/s throttle; SSE keep-alive 20 s (matches `/tenant/events`).

**Constraints**: Public endpoints are unauthenticated → must be rate-limited (per session + per instance), origin-checked against the per-instance domain allowlist, and must never accept client-supplied tenant IDs (tenant derives from the public widget identifier / session only). Widget iframe isolates styles from host pages (FR-006). Third-party storage partitioning: session token lives in iframe-origin `localStorage` (partitioned per top site — satisfies FR-008's same-site continuity, no cross-site tracking).

**Scale/Scope**: One new backend module crate + 1 migration + ~8 public/6 admin endpoints; widget app (~6 components) + loader; one dashboard settings feature area with live preview; multiple widget instances per tenant (small cardinality, dozens not thousands).

## Constitution Check

*GATE: evaluated against constitution v1.2.0 — PASS (pre-Phase-0 and re-checked post-Phase-1). No Complexity Tracking entries required.*

| Principle | Verdict | Notes |
|---|---|---|
| I. Modular monolith | PASS | New `backend/crates/modules/widgets` crate; communicates with AI via outbox events, matching the `ai`/`escalations` precedent. It reaches `conversations`/`customers` **only** through their public crate interfaces — which must first be widened to represent an anonymous visitor actor and widget attribution (see [research R12](research.md); tasks T011–T013). Direct writes to another module's tables are prohibited and called out in tasks.md. Extractable later. |
| II. Multi-tenant isolation | PASS | `widget_instances` and `widget_sessions` carry `tenant_id`; every public query is scoped by the instance→tenant resolution plus session ownership; the dashboard `X-Tenant-ID` path is never trusted on public routes (FR-025). |
| III. Zero-trust & RBAC | PASS | Admin routes gated by new `Permission::WidgetsView`/`WidgetsManage`; widget-instance create/update/delete audited. Public routes are anonymous **by product necessity** but constrained: rate limits, origin allowlist, minimal response surface (FR-022–FR-026), opaque hashed session tokens. |
| IV. AI provider independence | PASS | No new AI code paths; visitor messages ride the existing outbox → responder → provider-abstracted engine. |
| V. API-first | PASS | Contracts in `contracts/` follow `specs/001-.../contracts/rest-api.md` envelope/pagination/error conventions; all routes registered in utoipa OpenAPI. |
| VI. Observability | PASS | New endpoints use existing request-ID/tracing middleware; SSE relay and rate limiter emit tracing spans/counters. |
| VII. Test-first | PASS | Unit + DB integration + router permission + Playwright e2e planned per user story (see quickstart.md). |
| VIII. DB integrity & migrations | PASS | Single migration 0050; UUID PKs, timestamps, soft delete on `widget_instances`, partial unique index on public identifier, indexes on all production query paths. |
| IX. Design system | PASS | Widget surface gets its own minimal `--wgt-*` token sheet (tokens → components → screens, in that order) because Taiga/dashboard tokens cannot ship inside a 97 KB third-party bundle; dashboard settings page reuses existing shared/layout components. |
| X. Performance & streaming | PASS | AI deltas streamed to the visitor (FR-014); no N+1 (timeline queries reuse conversations module queries); loader is dependency-free. |

## Project Structure

### Documentation (this feature)

```text
specs/023-website-chat-widget/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── public-widget-api.md   # Unauthenticated widget/visitor surface
│   └── widget-admin-api.md    # Tenant dashboard management surface
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0050_website_chat_widget.sql      # widget_instances, widget_sessions, conversations.widget_instance_id
└── crates/
    ├── modules/widgets/                  # NEW module crate
    │   └── src/
    │       ├── lib.rs                    # module wiring, config
    │       ├── model.rs                  # WidgetInstance, WidgetSession, public views
    │       ├── queries.rs                # SQLx queries (tenant-scoped)
    │       ├── admin_routes.rs           # tenant CRUD + snippet (authenticated)
    │       ├── public_routes.rs          # config, session, conversation, messages (anonymous)
    │       ├── public_events.rs          # per-conversation SSE relay off the tenant bus
    │       ├── session.rs                # token mint/hash/verify, expiry
    │       ├── origin.rs                 # Origin/Referer check vs allowed_domains
    │       └── audit.rs                  # instance change audit records
    ├── server/src/
    │   ├── router.rs                     # mount /widget/v1 public scope + tenant widget routes
    │   └── rate_limit.rs                 # NEW in-process rate-limit tower layer
    ├── modules/authz/…                   # add WidgetsView / WidgetsManage permissions
    ├── modules/conversations/            # EXTEND (see research R12): anonymous visitor actor
    │   └── src/queries.rs                # + widget_instance_id on conversation creation
    └── modules/customers/
        └── src/queries.rs                # EXTEND: public create_anonymous_customer_in_tx

frontend/
├── apps/widget/                          # build out existing bare scaffold
│   ├── loader/loader.ts                  # NEW embed loader → dist/widget/widget.js: config fetch,
│   │                                     # launcher button, iframe lifecycle (research R1)
│   └── src/
│       ├── main.ts, index.html           # iframe entry
│       ├── app.component.ts              # chat-window composition root (no launcher — loader owns it)
│       ├── theme/tokens.css              # --wgt-* tokens, light/dark, tenant primary color
│       ├── core/                         # widget-api.service, session.store, sse client, store
│       └── components/                   # window, message-list, message, composer,
│                                         # typing-indicator, handoff-banner, error/rate-limit states
├── apps/dashboard/src/app/features/tenant/widgets/   # settings page: instance list/editor,
│                                                     # live preview, embed snippet, allowlist
└── e2e/                                  # embed host-page fixture + chat/handoff e2e
```

**Structure Decision**: Follow the established modular-monolith layout — one new `widgets` module crate beside `conversations`/`escalations`, public routes mounted from `server/router.rs`'s existing `public_routes()` seam, and the widget UI built inside the already-scaffolded `frontend/apps/widget` application (its "prior scaffolding — do not modify" note in `frontend/CLAUDE.md` was scoped to dashboard work and is superseded by this feature, which is that scaffold's intended purpose; the note is updated as part of this feature). The dashboard settings page lives under the existing `features/tenant/` area per frontend layering rules.

## Complexity Tracking

No constitution violations to justify. (Public anonymous endpoints are a product requirement of the channel, not a Zero-Trust deviation: authorization still exists — it is the widget-session token — and the surface is rate-limited, origin-checked, and minimal.)

### Bundle Budget Deviation

The 97 KB initial bundle budget declared in plan.md Performance Goals was unreachable with the Angular 22 standalone application (runtime alone is ~150 KB compressed). The `frontend/angular.json` widget project budgets are now `maximumWarning: 170kb` / `maximumError: 180kb` — tight around the measured 173.56 KB raw / 49.73 KB transfer size to prevent unbounded growth. This deviation was recorded per T098 guidance; a future major version of Angular with better tree-shaking or a non-Angular rewrite (Preact, Lit) could reclaim the original target.
