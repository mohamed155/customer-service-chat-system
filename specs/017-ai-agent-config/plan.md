# Implementation Plan: AI Agent Configuration

**Branch**: `017-ai-agent-config` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/017-ai-agent-config/spec.md`

## Summary

Give every tenant a configurable AI agent — name, avatar (preset or ≤256 KB upload), curated tone, system prompt, ordered business rules, escalation rules (explicit-human-request + topic/keyword triggers), per-channel enablement, and an optional provider/model override — with exactly one live agent per tenant in v1 on a schema that is already multi-agent-shaped (dropping a single partial unique index is the whole multi-agent unlock). The tenant's own agent is inactive until the first save; before that, arriving customer messages get a one-time `system`-kind auto-acknowledgment and staff with `conversations.manage` choose per conversation between platform-provided AI (default persona over the 015 AI layer) and human escalation (`conversations.ai_handling`, new decision endpoint — FR-004a–c). A new outbox-driven responder in `modules/ai` reacts to `conversation.customer_message` events: it gates on channel/state, evaluates escalation rules deterministically (baseline human-request catalog always first, tenant rules in order, first match wins → escalation via the 014 routing entry with the rule name as reason), otherwise composes a byte-deterministic prompt (prompt + tone directive + numbered rules + guardrail), resolves provider/model (agent override → 015 AI-layer resolution), calls `AiService::complete`, and inserts a new `ai`-kind message. All config writes are audited, version-guarded against concurrent edits, and restricted to Owner/Admin (matrix narrowing). The dashboard's fixture-based AI Agent page becomes the real settings surface: prompt editor, tone selector, avatar picker/upload, rules editors with broken-skill-ref flags, channel toggles, provider/model selector with staleness warning, and 409-conflict handling.

## Technical Context

**Language/Version**: Backend Rust (Cargo workspace, edition 2021); frontend Angular 22 standalone (TypeScript, Signals, RxJS-first, Taiga UI) in `frontend/apps/dashboard`

**Primary Dependencies**: Backend — Axum via the deny-by-default `.guarded()` builders, SQLx/PostgreSQL, existing crates `modules/ai` (015: `AiService`, credential/config resolution, usage), `modules/conversations` (`outbox.rs`, message insertion), `modules/escalations` (`route_new_escalation_in_tx`), `authz` matrix, `tenancy::audit::record_in_tx`, utoipa. **No new workspace dependencies** — the only Cargo.toml additions are existing workspace deps newly referenced by a crate (`ai` gains `conversations`/`escalations` paths; `conversations` gains `async-trait` for its `AiAgentStatus` port). Frontend — existing typed `ApiResponse<T>` HTTP layer, NgRx SignalStore per feature, `APP_PATHS` routing, Taiga-wrapped shared components

**Storage**: PostgreSQL — migration `0041_agent_configurations.sql` (`agent_configurations`: NOT NULL `tenant_id`, name ≤80, `is_default`, avatar kind/preset, tone CHECK vs 5-value catalog, `system_prompt` ≤8000 CHECK, `business_rules`/`escalation_rules`/`enabled_channels` JSONB, nullable paired provider/model override with catalog CHECK, `version` counter; three partial unique indexes — v1 single-agent cap `(tenant_id)`, one-default `(tenant_id) WHERE is_default`, unique names `(tenant_id, lower(name))`; plus `agent_avatar_uploads`: BYTEA ≤256 KB, content-type CHECK, one live per agent) `0042_message_kinds_ai_system.sql` (extend `messages_kind_check` + `messages_kind_consistency` with `'ai'` and `'system'`, NULL membership arms), and `0043_conversation_ai_handling.sql` (`conversations.ai_handling TEXT NULL CHECK IN ('platform_ai','human')` — per-conversation fallback decision, consulted only while no live agent exists). Outbox reuses the existing `outbox_events` table — no migration. See [data-model.md](./data-model.md)

**Testing**: `cargo test` — unit: payload validation (rules shapes, channels, tone, prompt bounds), prompt-composition byte-equality, rule matching (baseline precedence, first-match-wins, keyword case-insensitivity), options assembly; integration `server/tests/ai_agent.rs` (DB-gated per `REQUIRE_DB_TESTS` pattern): GET-defaults/first-save-201/round-trip, validation 422 atomicity, version 409, avatar upload/serve/limits, cross-tenant isolation matrix, audit rows incl. platform-actor attribution, RBAC narrowing, and responder pipeline end-to-end via `process_agent_responder_once` + wiremock vendor mock (reply inserted, channel gating, escalation rule → 014 queue with rule-name reason, baseline with zero rules, stale-override fallback, unconfigured branch: single auto-ack idempotency, `platform_ai` replies under the default persona, `human` decision escalates, decision endpoint 409/422 matrix, configured agent supersedes); `rbac.rs` route→permission additions + narrowed matrix expectations; `shared/db/tests/schema.rs` 0041–0043 assertions; `openapi_contract.rs` additions. Frontend: `pnpm ng test dashboard` store/component specs (form mapping, 409 handling, stale/broken-ref banners), plus lint/format/build gates

**Target Platform**: Linux server (backend) + evergreen-browser dashboard

**Project Type**: Web application — existing Cargo workspace backend + Angular dashboard frontend

**Performance Goals**: `add_message` latency unchanged (responder is outbox-async — one extra transactional INSERT per customer message); responder adds two indexed single-row reads (agent config, conversation state) + bounded history fetch (last 20 messages, indexed) ahead of vendor latency, which dominates; rule evaluation is in-memory string matching over ≤20 rules; settings GET/PUT are single-row indexed operations; avatar serving is one indexed row with private caching

**Constraints**: Deterministic prompt composition and pure string-match escalation decisions — no LLM in the escalate/respond gate (Constitution IV, R7/R12); conversations module gains zero AI knowledge (emits a domain event only — Constitution I); every config write audited in-transaction, Owner/Admin only, deny-by-default routes, cross-tenant → `not_found` (Constitution II/III); schema via migrations only, v1 single-agent enforced by droppable index not code redesign (FR-002/FR-003); saves bind at the next responder run (FR-016); no streaming in v1 responder (timeline has no streaming surface; 015 keeps that door open); message/prompt content never in logs or traces (015 invariant)

**Scale/Scope**: 3 migrations, 2 new tables + 1 CHECK extension + 1 conversations column; 0 new permission codes (1 matrix narrowing); 6 new tenant endpoints (5 agent + 1 ai-handling decision); 1 new outbox event type + 1 new worker; ~6 new files in `modules/ai`, moderate diffs in `conversations` (outbox emit, `ai`/`system` kinds, ai-handling endpoint + DTO fields) and `authz` (matrix); audit vocabulary +4 actions; frontend: 1 rebuilt feature page (~8 files in `features/tenant/ai-agent/`) + an AI-handling decision banner in the conversations feature, no new libs; 1 new integration suite + 4 extended test files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Ownership stays clean: `modules/ai` owns agent config + responder; conversations only emits `conversation.customer_message` on its existing outbox (no AI imports); escalations is entered through its existing transactional routing function. **No module reads or writes another's tables**: the responder calls `pub` application-service helpers on `conversations`/`escalations` rather than raw SQL, and where `conversations` needs an `ai`-owned fact for its DTOs it declares an `AiAgentStatus` trait port that `ai` implements and the server injects (dependency arrow stays `ai → conversations`). The responder consumes a domain event — exactly the sanctioned inter-module channel | ✅ Pass |
| II. Multi-Tenant Isolation | `agent_configurations.tenant_id` and `agent_avatar_uploads.tenant_id` NOT NULL (no platform scope this time); every query middleware-tenant-filtered; cross-tenant reads answer `not_found`; responder scopes config, history, skills, and escalation writes to the event's tenant; isolation matrix in `ai_agent.rs` | ✅ Pass |
| III. Zero-Trust Security & RBAC | All six routes via `.guarded()` — the five agent routes on the existing `ai_agent.view/manage` codes, the ai-handling decision on `conversations.manage`; matrix narrowed to Owner/Admin per spec clarification (R11); every create/update/avatar change audited via `tenancy::audit::record_in_tx` with platform-actor attribution; prompt-injection content in config cannot widen capability — the responder only ever calls `AiService::complete` with composed text (FR-018) | ✅ Pass |
| IV. AI Provider Independence & Tool-Mediated Access | Prompt composition is a fixed-order byte-deterministic template (R12, unit-verified); escalation decisions are pure string matching, never LLM-delegated (R7); provider/model selection rides 015's abstraction — the responder never sees vendor types; no DB access by the LLM (it receives composed messages only) | ✅ Pass |
| V. API-First & Contract Consistency | Six REST endpoints in [contracts/rest-api.md](./contracts/rest-api.md) with the standard envelope/error vocabulary; PUT is idempotent full-replace; version field makes concurrency explicit; OpenAPI registration extended; the internal pipeline is versioned in [contracts/agent-runtime.md](./contracts/agent-runtime.md) | ✅ Pass |
| VI. Observability by Default | Responder emits structured trace events per run (gates, rule fired, provider resolved, latency) with request-id propagated into 015's vendor call and usage record; config/prompt content excluded from logs/traces by construction | ✅ Pass |
| VII. Test-First & Regression Discipline | Unit (validation, composition byte-equality, rule matching), integration (CRUD/RBAC/isolation/audit/concurrency/avatar + full responder pipeline via `process_agent_responder_once` + wiremock), schema assertions, rbac map, OpenAPI contract, frontend store/component specs | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migrations 0041–0043 only; 005 conventions (UUID v7, timestamps, trigger, soft delete); partial unique indexes enforce the v1 cap, one-default, and unique names; JSONB rule arrays are a recorded denormalization (see Complexity Tracking); responder query paths ride existing indexes + the new partial uniques | ⚠️ Justified deviation |
| IX. Design System Discipline | Settings page composes existing shared/Taiga-wrapped components and `--app-*` tokens; new editors (rules list, tone selector, avatar picker) built as feature components reusing shared form patterns — no raw Taiga in feature pages, no duplicated UI logic | ✅ Pass |
| X. Performance & Efficiency | AI work is off the request path (outbox worker); no N+1 (rules ride the config row; skill staleness is one `ANY($ids)` query; history is one bounded fetch); settings surface is single-row ops; avatar bytes kept off the hot config row | ✅ Pass |

**Initial gate**: PASS — two justified deviations recorded in Complexity Tracking (JSONB rule arrays; avatars in PostgreSQL instead of the stack's object storage).

**Post-design re-check (after Phase 1, re-verified after `/speckit-analyze`)**: PASS — no new deviations introduced by the design artifacts; the two recorded in Complexity Tracking remain the only ones. Three Principle I pressure points surfaced during task breakdown and were resolved *toward* the principle rather than deviated around (see Structure Decision): raw cross-module SQL in the responder → owning-module helpers; the ai-handling handler's crate home → `ai`; `conversations` needing agent-configured/AI-resolvable facts → an injected trait port. Nuanced calls, grounded in spec: (1) the unconfigured fallback (FR-004a–c) keeps the auto-acknowledgment out of the `ai` kind — it is platform-authored fixed text, so it ships as a `system` message and never masquerades as the agent; the decision endpoint lives in the conversations module under `conversations.manage` because it is conversation handling, not AI settings (Owner/Admin-only AI settings stay intact, R11); baseline human-request escalation applies under `platform_ai` handling since it guards any AI participation; (2) vendor failure after 015's retries yields silence + existing human flow rather than an apologetic AI message — an unconfigurable canned reply would itself be un-audited agent behavior; failures stay visible in usage records and traces; (3) the RBAC narrowing intentionally also tightens 015's provider/key/usage routes (same permission codes) — one conceptual "AI settings" surface, one access rule (R11).

## Project Structure

### Documentation (this feature)

```text
specs/017-ai-agent-config/
├── plan.md                  # This file
├── research.md              # Phase 0 — R1–R13 decisions
├── data-model.md            # Phase 1 — agent_configurations, agent_avatar_uploads, kind='ai'/'system',
│                            #           conversations.ai_handling, audit actions
├── quickstart.md            # Phase 1 — automated gates + 15-step manual walkthrough
├── contracts/
│   ├── rest-api.md          # 6 tenant endpoints (5 agent + ai-handling decision), payloads, errors, permissions, matrix narrowing
│   └── agent-runtime.md     # outbox event, responder pipeline incl. unconfigured-fallback branch, prompt template, determinism
└── tasks.md                 # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0041_agent_configurations.sql   # NEW — agent_configurations + agent_avatar_uploads + partial uniques
│   ├── 0042_message_kinds_ai_system.sql # NEW — messages kind CHECKs gain 'ai' + 'system'
│   └── 0043_conversation_ai_handling.sql # NEW — conversations.ai_handling (fallback decision)
└── crates/
    ├── modules/
    │   ├── ai/src/
    │   │   ├── lib.rs                  # MODIFIED — module docs + exports (agent config, responder)
    │   │   ├── agent_config.rs         # NEW — model, validation (rules/channels/tone/prompt), queries, staleness/broken-ref derivation
    │   │   ├── agent_routes.rs         # NEW — GET/PUT agent, avatar PUT/GET, options; version guard; audit calls;
    │   │   │                           #        + set_conversation_ai_handling (POST /tenant/conversations/{id}/ai-handling —
    │   │   │                           #        handler lives here: `ai` is the only crate reaching both conversations
    │   │   │                           #        and escalations without a cycle; guarded by conversations.manage)
    │   │   ├── agent_prompt.rs         # NEW — deterministic composer (tone directives, rules block, guardrail)
    │   │   ├── agent_rules.rs          # NEW — human-request phrase catalog, rule evaluation (baseline-first, first-match)
    │   │   ├── agent_responder.rs      # NEW — outbox worker + process_agent_responder_once; gates, escalate-or-reply,
    │   │   │                           #        unconfigured branch (auto-ack, ai_handling, platform persona), idempotency
    │   │   │                           #        (3-phase: lock-free read → vendor call off-transaction → short insert tx)
    │   │   └── agent_audit.rs          # NEW — agent_config.* + conversation.ai_handling_set audit helpers
    │   │                               #        (015's ai_config.*/ai_credential.* stay in the existing audit.rs)
    │   ├── conversations/src/
    │   │   ├── lib.rs (or ports.rs)    # MODIFIED/NEW — `AiAgentStatus` trait port (agent_configured /
    │   │   │                           #            platform_ai_available); implemented in `ai`, injected by the
    │   │   │                           #            server — keeps conversations from ever reading ai-owned tables
    │   │   ├── outbox.rs               # MODIFIED — emit_customer_message_in_tx
    │   │   ├── routes.rs               # MODIFIED — emit on add_message + create_conversation initial message;
    │   │   │                           #            detail/list handlers compose awaiting_ai_decision via the port
    │   │   ├── model.rs                # MODIFIED — MessageKind::{Ai,System}, ai_agent/system participant projections,
    │   │   │                           #            ai_handling + awaiting_ai_decision on conversation DTOs
    │   │   └── queries.rs              # MODIFIED — timeline/preview handle new kinds; conversations-owned helpers the
    │   │                               #            responder calls (auto-ack/AI-reply inserts, ai state, history,
    │   │                               #            idempotency guard); set_ai_handling_in_tx
    │   └── authz/src/matrix.rs         # MODIFIED — R11 narrowing (Manager/Viewer/staff-Developer lose ai_agent.*)
    ├── shared/db/tests/schema.rs       # MODIFIED — 0041–0043 assertions (CHECKs, partial uniques)
    └── server/
        ├── src/router.rs               # MODIFIED — mount agent + ai-handling routes under mount_tenant;
        │                               #            inject the AiAgentStatus adapter as an Extension
        ├── src/lib.rs                  # MODIFIED — spawn run_agent_responder_worker
        └── tests/
            ├── ai_agent.rs             # NEW — integration suite (see Testing)
            ├── rbac.rs                 # MODIFIED — new routes + narrowed matrix expectations
            └── openapi_contract.rs     # MODIFIED — new DTOs/paths

frontend/apps/dashboard/src/app/
├── features/tenant/ai-agent/           # REBUILT — fixture page becomes the real settings surface
│   ├── ai-agent.component.ts|spec      # MODIFIED — settings page: sections, save/conflict flow, inactive notice
│   ├── ai-agent.store.ts|spec          # MODIFIED — SignalStore: config+options load, dirty form state, save, 409/stale/broken-ref
│   ├── ai-agent-api.service.ts|spec    # NEW — typed client for the 6 endpoints (escalations-api.service pattern)
│   ├── prompt-editor.component.ts      # NEW — bounded textarea + counter
│   ├── tone-selector.component.ts      # NEW — catalog-driven selector
│   ├── avatar-picker.component.ts      # NEW — presets + upload (size/type client hints)
│   ├── rules-editor.component.ts       # NEW — business + escalation rules (order, keywords, skills, broken-ref flags)
│   └── provider-model-selector.component.ts  # NEW — credential-gated providers, curated models, follow-default option, stale banner
├── features/tenant/conversations/      # MODIFIED — AI-handling decision banner on awaiting conversations
│   ├── ai-handling-banner.component.ts|spec  # NEW — "Use platform AI / Assign to human" actions, disabled-with-reason state
│   └── (api service + store)           # MODIFIED — ai_handling/awaiting_ai_decision fields + decision call
└── core/authz                          # MODIFIED only if nav visibility needs the narrowed permission (verify; no new code expected)
```

**Structure Decision**: Everything AI-behavioral lands in `modules/ai` (R1) — it already owns the AI admin surface, permissions, and `AiService`, so the dependency graph stays acyclic in one direction only: `ai → conversations` and `ai → escalations` (which itself already depends on `conversations`). Three consequences follow, and they are what keep Principle I real rather than asserted: (1) the responder never writes SQL against conversations/escalations tables — every message insert, state read, and escalation goes through a `pub` application-service function on the owning module; (2) the ai-handling decision endpoint's *handler* lives in `ai` even though its path and permission are conversation-shaped, because `ai` is the only crate that can reach both collaborators without a cycle; (3) where `conversations` needs a fact that `ai` owns (is the tenant's agent configured? does the platform AI layer resolve?) it declares an `AiAgentStatus` trait port and the server injects an `ai`-backed adapter — the dependency arrow stays `ai → conversations`, and no module reads another's tables. The frontend rebuilds the existing `features/tenant/ai-agent` page in place, following the established feature anatomy (flat feature dir: api service + SignalStore + components), so routing (`APP_PATHS.tenant.aiAgent`), page title, and nav slot all already exist.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `business_rules` / `escalation_rules` / `enabled_channels` as JSONB arrays on the agent row instead of normalized child tables (Principle VIII: normalized by default) | Lists are small (≤20, code-validated), strictly ordered, always read whole-row by both the settings page and the responder hot path, and never queried relationally; matches the accepted 015 `fallbacks` precedent | Child tables would add joins to every responder run and settings read, CRUD fan-out for three tables, and buy nothing until rules need independent audit trails or cross-agent reuse — neither is in any spec |
| Uploaded avatars stored as bounded BYTEA rows in PostgreSQL (`agent_avatar_uploads`) instead of the stack's S3-compatible object storage | No object-storage integration exists in the repo yet; uploads are capped at 256 KB, one per agent, served through an authenticated endpoint — a dedicated table isolates the bytes so a later move to object storage is a backfill + reference swap invisible to the API (R4) | Standing up S3/MinIO client, config, credentials, and failure handling for one cosmetic thumbnail field is disproportionate; deferring upload entirely was overruled by clarification #3 (upload is in v1) |
