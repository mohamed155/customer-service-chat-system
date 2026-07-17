# Implementation Plan: AI Conversation Engine

**Branch**: `021-ai-conversation-engine` | **Date**: 2026-07-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/021-ai-conversation-engine/spec.md`

## Summary

Turn the existing outbox-driven agent responder (features 017 + 020) into a complete conversation engine. Backend: refactor `ai::agent_responder` into an engine pipeline that streams provider output (via the existing `AiService::stream`), broadcasts progressive `ai.message.*` events on the existing `/tenant/events` SSE stream, supersedes in-flight generations when new customer messages or escalations arrive, resumes interrupted streams via continuation requests, retries within a bounded budget and falls back (fallback message + escalation routing) on exhaustion, records a per-attempt `ai_generations` trace row, and attaches a deterministic heuristic confidence score to every stored AI reply. New `POST /tenant/conversations/{id}/summary` endpoint generates an on-demand, staff-only summary through the same provider abstraction. Frontend: distinct AI response card with confidence badge, thinking indicator + live streaming text in the conversation thread (driven by the extended realtime service), and a conversation summary panel.

## Technical Context

**Language/Version**: Backend Rust (2021 edition, workspace in `backend/`); Frontend TypeScript / Angular 22 (standalone components, Signals, RxJS-first)

**Primary Dependencies**: Axum, Tokio, SQLx, `futures` streams (existing), existing `ai-providers` crate (`ChatProvider::stream` + SSE decoding already implemented for OpenAI/Anthropic/Gemini); Taiga UI, NgRx SignalStore, existing fetch-based SSE realtime client

**Storage**: PostgreSQL (messages, `ai_generations` trace table, `ai_usage_records`); no new storage systems

**Testing**: `cargo test` (unit + Postgres-backed integration tests in `backend/crates/server/tests`); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` in `frontend/`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — modular-monolith Rust backend + Angular dashboard

**Performance Goals**: SC-002 thinking indicator + first streamed content ≤ 5 s, complete responses ≤ 20 s typical; SC-005 fallback message + human routing ≤ 60 s under total provider outage (engine outer deadline 45 s, ≤ 3 provider attempts with backoff); SC-007 summary ≤ 10 s for a 50-message conversation

**Constraints**: Deterministic prompt construction (Principle IV — same config + history + chunks ⇒ same prompt structure); at most one in-flight generation per conversation (FR-016, supersede semantics per clarification); partial streamed content never stands as final (clarifications Q4/Q5); tenant isolation on every query (Principle II); streaming reuses the existing tenant SSE stream — no new transport infrastructure

**Scale/Scope**: History window 20 messages (existing `recent_history` cap), retrieval top-k ≤ 5 chunks (existing), summary window ≤ 50 messages; one engine worker loop (existing single-claim `FOR UPDATE SKIP LOCKED` pattern scales to multiple server instances)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Modular Monolith | PASS — engine stays inside the `ai` module, consuming `conversations`, `knowledge`, and `escalations` only through their existing public query/routing/runtime functions (already the 017/020 pattern). Realtime AI events ride the existing tenant event runtime owned by `escalations` via a new dedicated event variant (research §2 records the boundary trade-off and the future `realtime` extraction path). |
| II. Multi-Tenant Isolation | PASS — `ai_generations` carries `tenant_id`; every engine query is tenant-scoped; SSE broadcast is per-tenant (existing runtime keying); summary endpoint resolves the conversation under `TenantContext`. FR-014/SC-004 isolation integration tests are mandatory. |
| III. Zero-Trust & RBAC | PASS — summary endpoint requires an active tenant membership with conversation read access via existing `authz`; no new secrets; confidence/generation data exposed only on authenticated tenant routes; fallback/escalation writes are audited through the existing escalation audit path. |
| IV. AI Provider Independence & Tool-Mediated Access | PASS — engine calls providers only through `AiService` / `ChatProvider` (streaming variant already provider-abstracted); prompt assembly is deterministic and code-driven; the LLM never touches the DB; confidence is a deterministic backend heuristic, not a provider-specific feature. One addition: `AiService::stream_with_override` mirroring the existing `complete_with_override` (same abstraction, no provider-specific code in the engine). |
| V. API-First & Contract Consistency | PASS — additive `confidence` fields on the existing message contract; new `POST /tenant/conversations/{id}/summary` (side-effect-free generation, safe to repeat); new SSE event types documented in `contracts/`; standard error envelopes; OpenAPI coverage tests extended. |
| VI. Observability | PASS — `ai_generations` is the inspectable execution record (FR-015/SC-008): trigger message, provider/model, outcome, attempts, latency, retrieval stats, usage-record linkage, request_id; plus `engine.generate` tracing spans correlated to conversation/message, alongside the existing `rag.retrieve` span. |
| VII. Test-First & Regression | PASS — unit (confidence formula, prompt assembly determinism, supersede decision logic, continuation stitching), integration (end-to-end respond/store, supersede, escalation-cancel, fallback + routing, isolation), API (summary contract + RBAC, message confidence fields), frontend specs (card, badge, indicator, summary, realtime reducer). |
| VIII. DB Integrity & Migrations | PASS — single migration `0048_ai_conversation_engine.sql`: `ai_generations` table (normalized, `tenant_id`, FKs, indexed on `(tenant_id, conversation_id, created_at)`) and nullable `ai_confidence_score` on `messages` (attribute of the message row itself, not a join-away detail — justified denormalization consistent with the citation-snapshot precedent). |
| IX. Design System Discipline | PASS — confidence badge and thinking indicator are reusable `shared/components`; AI response card styling extends the existing thread message patterns with Helix tokens; summary panel reuses existing panel/card patterns. |
| X. Performance & Efficiency | PASS — streaming end-to-end (provider → SSE → UI) per the principle's explicit call-out; batched citation/confidence loading on timelines (no N+1); bounded history/summary windows; single-claim worker avoids lock contention. |

**Post-design re-check (after Phase 1)**: PASS — data model and contracts introduce no violations; Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/021-ai-conversation-engine/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── ai-events-sse.md
│   ├── conversation-summary.md
│   └── message-confidence.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0048_ai_conversation_engine.sql        # ai_generations + messages.ai_confidence_score
└── crates/
    ├── modules/ai/src/
    │   ├── agent_responder.rs                 # refactored: claim/gate phases delegate to engine
    │   ├── engine.rs                          # NEW: streaming generation pipeline (assemble → stream →
    │   │                                      #   supersede-check → resume → store | fallback)
    │   ├── confidence.rs                      # NEW: deterministic confidence heuristic + banding
    │   ├── generation_record.rs               # NEW: ai_generations writes/reads
    │   ├── summary.rs                         # NEW: summary generation + route handler
    │   ├── service.rs                         # + stream_with_override (mirrors complete_with_override)
    │   └── lib.rs                             # wire new submodules
    ├── modules/conversations/src/
    │   ├── model.rs                           # Message.confidence (additive), fallback message kind reuse
    │   └── queries.rs                         # insert_ai_reply_in_tx + confidence; newer-message check;
    │                                          #   insert_fallback_in_tx; summary history query
    ├── modules/escalations/src/
    │   ├── presence.rs                        # Event::ConversationAi(...) broadcast variant
    │   └── model.rs                           # AI message event payload structs (serialized to SSE)
    └── server/
        ├── src/router.rs                      # + POST /tenant/conversations/{id}/summary
        └── tests/                             # engine integration + contract tests

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts               # Message.confidence, summary response, SSE event types
│   └── realtime/realtime.service.ts           # parse/dispatch ai.message.* events
├── features/tenant/conversations/
│   ├── conversation-thread.component.ts       # AI response card rendering + streaming block
│   ├── conversation-detail.store.ts           # in-flight generation state (start/delta/done/superseded)
│   ├── conversation-detail.component.ts       # summary trigger + panel placement
│   ├── conversation-summary.component.ts      # NEW: summary panel
│   └── conversations-api.service.ts           # + requestSummary()
└── shared/components/
    ├── ai-confidence-badge/                   # NEW: band badge (high/medium/low)
    └── ai-thinking-indicator/                 # NEW: animated thinking indicator
```

**Structure Decision**: Web application layout (existing). The engine remains in `backend/crates/modules/ai` — 021 is an evolution of the responder that module already owns; `conversations` keeps exclusive ownership of message-table writes via `_in_tx` helpers; `escalations` keeps ownership of the tenant SSE runtime. Frontend follows the established feature + shared-component split.

## Complexity Tracking

> No constitution violations — table intentionally left empty.
