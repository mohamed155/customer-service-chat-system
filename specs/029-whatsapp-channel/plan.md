# Implementation Plan: WhatsApp Channel

**Branch**: `029-whatsapp-channel` | **Date**: 2026-07-23 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/029-whatsapp-channel/spec.md`

## Summary

Add WhatsApp as the platform's second customer-facing channel by integrating directly with Meta's WhatsApp Business Cloud API. A new `whatsapp` backend module owns the Meta-specific webhook (GET verification handshake + POST deliveries signed with `X-Hub-Signature-256`), inbound message intake (dedupe by provider message id, exact-number customer identity resolution, conversation create-or-append, media fetch into S3-compatible storage), and an outbound sender worker that delivers agent/AI replies through the Graph API and tracks the sent→delivered→read/failed lifecycle from status webhooks. Connection lifecycle, secret storage, health status, and event logging reuse the 028 integrations foundation (WhatsApp becomes a connectable catalog entry). AI replies require no new pipeline: inbound messages emit the existing `conversation.customer_message` outbox event with `channel = "whatsapp"`, which the existing agent responder already handles (`whatsapp` is already in the agent-config channel catalog); only the delivery leg (a `whatsapp.outbound_message` outbox event + sender worker) is new. The dashboard gains WhatsApp channel identification/filtering in the existing inbox, attachment rendering and outbound delivery-status display in the conversation view, and a schema-driven connection form via the existing integrations pages.

## Technical Context

**Language/Version**: Backend: Rust (workspace at repo root `backend/`, Axum + Tokio + SQLx). Frontend: Angular 22, TypeScript, standalone components, SignalStores.

**Primary Dependencies**: Axum (HTTP + SSE), SQLx/PostgreSQL, `reqwest` (workspace dep; Graph API client), `hmac`/`sha2` (signature verification, already used by integrations), `aws-sdk-s3` via `backend/crates/shared/storage` (media storage), existing module crates: `integrations` (connection/secrets/events), `conversations` (messages, outbox), `customers` (identities), `ai` (agent responder, untouched), `escalations` (tenant SSE fan-out pattern).

**Storage**: PostgreSQL (new migration `0057`: catalog seed, `whatsapp_message_meta`, `message_attachments`, `whatsapp_contact_windows` is NOT needed — window derived from messages); S3-compatible object storage for inbound media (same bucket/config machinery as knowledge documents).

**Testing**: `cargo test` per-crate narrow suites with a PostgreSQL instance up (workspace-wide gate is unreliable — run module suites directly); Graph API isolated behind a trait with a test double; Angular unit tests via existing vitest/karma setup in `frontend/`.

**Target Platform**: Linux server (single modular-monolith binary `backend/crates/server`), Angular dashboard web app.

**Project Type**: Web application (Rust backend + Angular frontend monorepo).

**Performance Goals**: Webhook acknowledgment fast (< 1s; Meta retries on slow responses — do heavy work after intake row commit or in workers); inbound message visible in inbox ≤ 5s (SC-002); outbound handed to Graph API ≤ 5s from message creation (SC-005) via short-interval sender worker polling like the agent responder.

**Constraints**: Meta signs payloads with the tenant's **app secret** (HMAC-SHA256 over raw body) — signature scheme differs from the 028 generic webhook but reuses its token-addressed connection lookup; webhook GET verification handshake must respond with `hub.challenge`; media URLs from Graph API expire (~5 min) so media fetch happens promptly in a worker with retry; free-form sends outside the 24h customer-service window are rejected (pre-check + map Meta error `131047`); outbound is text-only in v1; per-connection intake rate limiting reuses `kernel::InMemoryRateLimitStore`.

**Scale/Scope**: One WhatsApp number per tenant (unique active connection per tenant already enforced by 028's `(tenant_id, catalog_id)` uniqueness); message volume comparable to widget channel; no rollup tables; 90-day delivery-record retention via existing integrations sweeper.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Modular Monolith | PASS | New `whatsapp` module crate behind clear interfaces; talks to `conversations`/`customers` via their public `*_in_tx` application services and to the AI pipeline only via outbox events (same pattern as `widgets`). No cycle: `whatsapp` depends on `integrations`, never the reverse. |
| II. Multi-Tenant Isolation | PASS | Every new table carries `tenant_id`; webhook resolves tenant via connection token exactly like 028; media storage keys are tenant-prefixed; media served only through tenant-scoped authenticated endpoints. |
| III. Zero-Trust & RBAC | PASS | Connection management inherits integrations permissions (`integrations.view`/`manage`); conversation surfaces inherit existing conversation permissions; webhook is unauthenticated by nature but signature-verified, rate-limited, and 404-uniform like 028; config changes audited via existing integrations audit writer. |
| IV. AI Provider Independence & Tool Mediation | PASS | AI pipeline untouched; WhatsApp is a transport. Meta Graph API access isolated behind a `WhatsAppApi` trait (send + media fetch) so tests use a double and future providers slot in. |
| V. API-First & Contracts | PASS | New REST endpoints follow existing conventions (`/integrations/whatsapp/webhook/{token}` public; attachment fetch under tenant API); idempotent intake (dedupe on provider message id). |
| VI. Observability | PASS | Tracing spans on webhook intake, sender worker, media worker; integration event log records accepted/rejected deliveries and send failures; request-id middleware already global. |
| VII. Test-First & Regression | PASS | Signature/verification unit tests, intake integration tests (dedupe, identity resolution, conversation mapping), sender worker tests against the trait double, window-enforcement tests. |
| VIII. DB Integrity & Migrations | PASS | Single migration `0057_whatsapp_channel.sql`; normalized tables; composite FKs mirror `messages` conventions; indexes on all query paths (wamid lookup, attachment-by-message, status-update path). |
| IX. Design System Discipline | PASS | Frontend reuses integrations pages (schema-driven form), existing inbox/conversation components extended with channel badge, attachment renderer, and status ticks as reusable pieces — no duplicated UI logic. |
| X. Performance & Efficiency | PASS | Webhook does minimal synchronous work; batch message fetch already avoids N+1 (attachments loaded like citations, one query per timeline page); workers poll with the same cadence pattern as `agent_responder`. |

**Post-design re-check (after Phase 1)**: PASS — no violations introduced; Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/029-whatsapp-channel/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── whatsapp-api.md  # Phase 1 output — webhook + tenant endpoints + events
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0057_whatsapp_channel.sql        # catalog seed, whatsapp_message_meta,
│                                        # message_attachments, indexes
└── crates/
    ├── modules/whatsapp/                # NEW module crate
    │   └── src/
    │       ├── lib.rs                   # module docs (purpose/interfaces/deps)
    │       ├── model.rs                 # rows, payload DTOs, status vocab
    │       ├── webhook.rs               # GET verify handshake + POST intake
    │       ├── inbound.rs               # parse → dedupe → identity → conversation → message
    │       ├── identity.rs              # E.164 normalization + exact-match resolution
    │       ├── api.rs                   # WhatsAppApi trait + Graph API impl (reqwest)
    │       ├── sender.rs                # outbound worker: claim outbox → send → status
    │       ├── media.rs                 # media fetch worker → shared/storage S3
    │       ├── window.rs                # 24h customer-service window checks
    │       ├── routes.rs                # tenant-scoped routes (attachment fetch)
    │       └── queries.rs               # SQLx queries for new tables
    ├── modules/conversations/src/
    │   ├── outbox.rs                    # + emit_whatsapp_outbound_in_tx
    │   ├── queries.rs                   # timeline: attach attachments + delivery status
    │   └── routes.rs                    # reply path: emit whatsapp outbound when channel=whatsapp;
    │                                    #             window pre-check error
    ├── modules/ai/src/agent_responder.rs # insert_ai_reply path: emit whatsapp outbound (small hook)
    ├── modules/integrations/src/        # untouched logic; whatsapp catalog row via migration;
    │                                    # webhook.rs helpers (token hash lookup) reused by whatsapp crate
    └── server/src/main.rs               # mount whatsapp routes; spawn sender + media workers

frontend/apps/dashboard/src/app/
├── features/tenant/integrations/       # schema-driven form already renders catalog config;
│                                       # + WhatsApp detail extras (webhook URL, verify token display)
├── features/tenant/conversations/      # inbox channel badge/filter for whatsapp;
│                                       # conversation view: attachment renderer, delivery-status ticks,
│                                       # failed-send reason, window-expired error surface
└── core/ | ui/                         # attachment + status-tick presentational components (reusable)
```

**Structure Decision**: Follow the established module-crate-per-feature layout. WhatsApp transport logic lives in a new `backend/crates/modules/whatsapp` crate (mirrors how `widgets` owns the web-chat transport). It depends on `integrations` (connection/token/secret helpers), `conversations` (message/conversation `*_in_tx` services + outbox emits), `customers` (identity attach/create), and `shared/storage` (media). The AI module is touched only to emit the outbound-delivery outbox event after inserting an AI reply into a WhatsApp conversation (one small hook; the notifications feature set the precedent for hand-rolled emits to avoid crate cycles).

## Complexity Tracking

*No constitution violations — table intentionally empty.*
