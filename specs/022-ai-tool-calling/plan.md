# Implementation Plan: AI Tool Calling

**Branch**: `022-ai-tool-calling` | **Date**: 2026-07-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/022-ai-tool-calling/spec.md`

## Summary

Give the 021 conversation engine the ability to act, not just answer. Backend: extend the `ai-providers` contract with provider-agnostic tool calling (tool specs on requests, tool-call finishes on streams, tool-result messages) mapped onto OpenAI, Anthropic, and Gemini; turn the placeholder `tools` module crate into the real tool subsystem — a built-in tool trait + catalog, tenant-defined external (endpoint-backed) tools with sealed credentials, per-tenant enablement/approval policy resolution, a validated executor with bounded timeouts, and a `tool_requests` lifecycle table that doubles as the audit trail. The engine gains a bounded multi-step tool loop (validate → execute → feed result back → continue, max N calls per generation); approval-required requests follow the clarified two-phase flow — interim holding message, generation ends, staff decision (approve/deny/expire/cancel) emits an outbox event that the existing responder worker consumes to run the follow-up generation. Tool activity broadcasts on the existing tenant SSE stream. Frontend: tool execution timeline entries in the conversation thread, an approval card with approve/deny actions, a collapsible tool result display, and a tenant settings page for enabling built-in tools and registering tenant-defined tools.

## Technical Context

**Language/Version**: Backend Rust (2021 edition, workspace in `backend/`); Frontend TypeScript / Angular 22 (standalone components, Signals, RxJS-first)

**Primary Dependencies**: Axum, Tokio, SQLx, `futures`; `ai-providers` crate (extended: `ToolSpec` on `ChatRequest`, tool-call stream events, tool-result messages across OpenAI/Anthropic/Gemini); `reqwest` (already a workspace dependency via providers) for tenant-defined external tool calls; AES-256-GCM `MasterKey` sealing (relocated from `ai::crypto` to `ai-providers::crypto`, see research §3); Taiga UI, NgRx SignalStore, existing fetch-based SSE realtime client

**Storage**: PostgreSQL — migration `0049_ai_tool_calling.sql`: `tenant_tools` (tenant-defined external tools, sealed credentials), `tenant_tool_policies` (per-tenant built-in tool enablement/tightening), `tool_requests` (full request lifecycle = audit trail, FK to `ai_generations`), plus an `ai_generations.outcome` CHECK extension adding `'awaiting_tool_approval'` (data-model.md migration note); reuses `outbox_events` for decision-driven follow-up generations

**Testing**: `cargo test` (unit + Postgres-backed integration tests in `backend/crates/server/tests`); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` in `frontend/`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — modular-monolith Rust backend + Angular dashboard

**Performance Goals**: Tool chain bounded at 5 calls/generation (platform default); built-in tool execution timeout 10 s, tenant-defined endpoint timeout 15 s with 1 MiB response cap; approval decision → follow-up generation begins ≤ 5 s (outbox worker poll cadence); approval window default 5 min, expiry sweep cadence ≤ 30 s; SC-004 timeline data loads with the conversation detail (no extra navigation)

**Constraints**: Principle IV — the LLM only ever sees tool names/descriptions/schemas and results, never endpoints, credentials, or the DB; deterministic prompt/tool-spec assembly (stable ordering); at-most-once execution per request (status-gated conditional UPDATE, no retry for side-effecting tools); single effective decision under concurrency; credentials sealed at rest, never in logs/responses/AI context (SC-008); tenant isolation on every query (Principle II); SSE reuses the existing tenant event runtime — no new transport

**Scale/Scope**: ~5 built-in tools ceiling for v1 (2 shipped: one auto-approved read-only, one approval-required side-effecting); tenant-defined tools expected O(10) per tenant; `tool_requests` grows with AI traffic — indexed on `(tenant_id, conversation_id, created_at)` and `(status, expires_at)` for the sweep

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Modular Monolith | PASS — the `tools` module crate owns registry, policy, approval, and execution; `ai` (engine) consumes it only through its public API (`resolve_tools`, `validate`, `execute`, `create_approval`, `decide`); `tools` depends on `conversations`/`customers` public query functions for built-in tool implementations, never on `ai` (no cycle). Follow-up generation is triggered through the existing outbox contract owned by `conversations`. |
| II. Multi-Tenant Isolation | PASS — all three new tables carry `tenant_id`; tool resolution, execution, approval decisions, and timeline queries are tenant-scoped; tenant-defined tools are invisible outside their tenant (FR-002); built-in tool implementations execute through tenant-scoped module functions. Isolation integration tests are mandatory (SC-003). |
| III. Zero-Trust & RBAC | PASS — approval decisions require an active membership with conversation-handling access (Agent+ via existing `authz`); tool settings CRUD requires tenant Admin+; endpoint credentials sealed with the existing master-key envelope and never returned after write; every tool policy/definition change and every approval decision is audited (existing audit conventions + `tool_requests` lifecycle). |
| IV. AI Provider Independence & Tool-Mediated Access | PASS — this feature *implements* the principle's tool-calling mandate. Tool support is added to the `ChatProvider` contract once and mapped per provider (research §1); the engine contains zero provider-specific code. The LLM receives only tool specs and results; executor mediates all data access; direct DB/network access by the model remains impossible. |
| V. API-First & Contract Consistency | PASS — new REST endpoints (tool settings CRUD, policy updates, approval decide, conversation tool activity) follow existing tenant-route conventions, error envelopes, and OpenAPI coverage tests; approval decide is idempotent-safe (second decision returns the settled state); SSE event additions documented in `contracts/`. |
| VI. Observability | PASS — `tool_requests` is the inspectable per-request lifecycle record (FR-016/FR-017); `tools.execute` tracing spans carry tenant/conversation/generation/request ids and correlate with the existing `engine.generate` span; the conversation timeline (FR-018) is the staff-facing rendering of the same records — directly satisfying the constitution's "inspectable execution timeline" requirement for tool calls. |
| VII. Test-First & Regression | PASS — unit (policy resolution incl. tighten-only, schema validation, chain bound, decision state machine, credential sealing round-trip, provider mapping per vendor); integration (auto tool end-to-end, approval two-phase flow, deny/expire/cancel, at-most-once under concurrent decisions, isolation, endpoint failure modes); API (settings CRUD + RBAC, decide contract, timeline); frontend specs (timeline entries, approval card actions, result display, settings page). |
| VIII. DB Integrity & Migrations | PASS — single migration `0049_ai_tool_calling.sql`; normalized tables with FKs (`tool_requests.generation_id → ai_generations`, `tenant_tool_id → tenant_tools`); status vocabulary CHECK-constrained; partial unique index for per-tenant tool-name uniqueness among live rows (soft-delete convention); indexes for timeline and expiry-sweep query paths. |
| IX. Design System Discipline | PASS — new `shared/components`: `tool-timeline-entry`, `tool-approval-card`, `tool-result-viewer` (collapsible JSON), reusing Helix tokens and existing card/badge/dialog patterns; settings page reuses `data-table`, `form-field`, `dialog-shell`, `inline-alert` shared components. |
| X. Performance & Efficiency | PASS — tool activity loads batched with conversation detail (no N+1); chain/timeout/response-size bounds cap worst-case latency and memory; expiry sweep uses the indexed `(status, expires_at)` path; SSE piggybacks the existing per-tenant broadcast. |

**Post-design re-check (after Phase 1)**: PASS — data model and contracts introduce no violations; Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/022-ai-tool-calling/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── provider-tool-contract.md   # internal ChatProvider extension
│   ├── tool-settings-api.md        # tenant tool registry + policy REST API
│   ├── tool-approvals-api.md       # pending approvals + decide REST API
│   └── tool-timeline.md            # conversation tool activity + SSE events
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0049_ai_tool_calling.sql          # tenant_tools, tenant_tool_policies, tool_requests
└── crates/
    ├── ai-providers/src/
    │   ├── contract.rs                    # + ToolSpec, ToolCall, Message tool parts,
    │   │                                  #   StreamEvent::ToolCall, FinishReason::ToolUse
    │   ├── crypto.rs                      # MOVED here from modules/ai (research §3)
    │   ├── openai.rs                      # tool_calls / role:"tool" mapping + SSE accumulation
    │   ├── anthropic.rs                   # tool_use / tool_result content-block mapping
    │   └── gemini.rs                      # functionCall / functionResponse mapping
    ├── modules/tools/src/                 # placeholder crate becomes the real module
    │   ├── lib.rs                         # module wiring + public API surface
    │   ├── model.rs                       # ToolDefinition, ToolSource, Classification,
    │   │                                  #   ToolRequest + status state machine
    │   ├── registry.rs                    # BuiltinTool trait + static catalog; spec assembly
    │   ├── builtin/                       # v1 built-ins: lookup_customer (auto),
    │   │   └── ...                        #   update_customer_contact (approval-required)
    │   ├── policy.rs                      # per-tenant resolution: enabled ∩ tightened
    │   ├── queries.rs                     # tenant_tools / policies / tool_requests SQL
    │   ├── executor.rs                    # validate → execute (builtin dispatch | sealed
    │   │                                  #   outbound endpoint call), timeouts, size caps
    │   ├── approval.rs                    # create pending, decide (conditional UPDATE),
    │   │                                  #   expiry sweep, outbox decision events
    │   ├── audit.rs                       # config-change audit records
    │   └── routes.rs                      # settings CRUD, decide, tool-activity endpoints
    ├── modules/ai/src/
    │   ├── engine.rs                      # tool loop: specs into request → ToolCall finish →
    │   │                                  #   validate/execute/append → continue (≤ max);
    │   │                                  #   approval → interim message + end generation
    │   ├── agent_responder.rs             # consume ai.tool_decision outbox events →
    │   │                                  #   follow-up generation with decision outcome
    │   ├── service.rs                     # thread tool specs through stream/complete calls
    │   └── crypto.rs                      # slimmed to re-export of ai_providers::crypto
    ├── modules/conversations/src/
    │   └── outbox.rs                      # emit_tool_decision_in_tx (new event type)
    ├── modules/escalations/src/
    │   ├── presence.rs                    # Event::ConversationTool(...) broadcast variant
    │   └── model.rs                       # tool activity / approval SSE payload structs
    └── server/
        ├── src/router.rs                  # + tool settings, approvals, tool-activity routes
        └── tests/                         # tool loop, approval flow, isolation, contract tests

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/tenant-api.models.ts           # tool defs, policies, requests, SSE event types
│   └── realtime/realtime.service.ts       # parse/dispatch tool.* events
├── features/tenant/conversations/
│   ├── conversation-thread.component.ts   # interleave tool timeline entries + approval card
│   ├── conversation-detail.store.ts       # tool activity state, live approval updates
│   └── conversations-api.service.ts       # + toolActivity(), decideToolRequest()
├── features/tenant/settings/
│   └── tools/                             # NEW: tool settings page — built-in enablement,
│       └── ...                            #   tenant-defined tool CRUD (no credential echo)
└── shared/components/
    ├── tool-timeline-entry/               # NEW: request lifecycle entry (status, duration)
    ├── tool-approval-card/                # NEW: pending approval w/ approve/deny actions
    └── tool-result-viewer/                # NEW: collapsible result / error details
```

**Structure Decision**: Web application layout (existing). The `tools` module crate — a placeholder since M0 — becomes the owning module for the entire tool subsystem, keeping `ai` focused on generation orchestration. Dependency direction: `ai → tools → (conversations, customers, ai-providers)`; no module depends on `ai`, so no cycles. `conversations` keeps exclusive ownership of message writes and the outbox; `escalations` keeps ownership of the tenant SSE runtime, gaining one event variant (the 021 precedent). Frontend follows the established feature + shared-component split; the settings page slots into the existing `features/tenant/settings` area.

## Complexity Tracking

> No constitution violations — table intentionally left empty.
