# Implementation Plan: Prompt Management

**Branch**: `018-prompt-management` | **Date**: 2026-07-16 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/018-prompt-management/spec.md`

## Summary

Make the tenant AI agent's system prompt safe to change: every save becomes an immutable, attributed version; history is browsable with roll-forward restore; content supports a curated `{{variable}}` catalog with live sample-value preview; validation (unknown/malformed placeholders, empty, over-length) blocks anything broken from ever activating; and every save/restore is audited. Mechanically, prompt content moves out of `agent_configurations.system_prompt` into two new `modules/ai`-owned tables (`agent_prompts` + append-only `agent_prompt_versions`, migration 0045 with backfill + column drop) so the new five-endpoint prompt API is the *only* write path (clarification #1, FR-018); saves are optimistic-concurrency-guarded (`baseVersion` + parent-row lock + unique `(prompt_id, version_number)`), no-op-detecting, and save = activate (clarification #2). The outbox responder now loads active content via one indexed read and renders runtime variables (`agent_name`, `tenant_name`, `customer_name`, `channel`) deterministically before the unchanged 017 composer. The dashboard gains an `ai-agent/prompt` child page — editor with inline validation, variables panel, live preview panel, version-history drawer with restore — while the agent settings page's inline prompt editor becomes a summary card that navigates there.

## Technical Context

**Language/Version**: Backend Rust (Cargo workspace, edition 2021); frontend Angular 22 standalone (TypeScript, Signals, RxJS-first, Taiga UI) in `frontend/apps/dashboard`

**Primary Dependencies**: Backend — Axum via `.guarded()`/`require_permission` route pattern, SQLx/PostgreSQL, existing crates `modules/ai` (017: agent config, composer, responder; 015: `AiService`), `modules/conversations` (responder-helpers block in `queries.rs` — gains one customer-display helper), `tenancy` (`audit::record_in_tx`, `authorize::fetch_tenant`), `authz` (existing `ai_agent.view/manage` codes), utoipa. **No new workspace dependencies and no new crates** — the M0 placeholder `modules/prompts` stays untouched (research R1). Frontend — existing typed `ApiResponse<T>` HTTP layer, NgRx SignalStore per feature, `APP_PATHS` routing, Taiga-wrapped shared components

**Storage**: PostgreSQL — migration `0045_agent_prompts.sql`: `agent_prompts` (tenant-keyed, `prompt_kind='system'` CHECK, `active_version`, partial unique `(tenant_id, prompt_kind)` = v1 one-prompt cap) + `agent_prompt_versions` (append-only audit-logs-style: content 1–8000 CHECK, `change_note` ≤ 500, `restored_from`, author id + display snapshot, `UNIQUE (prompt_id, version_number)`); backfill of version 1 from non-empty `agent_configurations.system_prompt`, then **DROP COLUMN system_prompt** (single source of truth, research R2). See [data-model.md](./data-model.md)

**Testing**: `cargo test` — unit: placeholder scanner/validation codes with offsets (`required`/`too_long`/`malformed_placeholder`/`unknown_variable`), `render_prompt` determinism + injection-safety (inserted values never re-scanned), composer byte-equality over rendered input, no-op byte-compare; integration `server/tests/ai_agent_prompt.rs` (DB-gated per `REQUIRE_DB_TESTS`): first-save v1 / save v2 / no-op / 409 conflict matrix, history pagination (`limit`/`before`, `hasMore`), version detail, restore + restored-from + blocked restore (validation) + no-op restore, RBAC (view vs manage), cross-tenant isolation → `not_found`, audit rows (both actions, no content in details, actor attribution incl. platform actor), responder end-to-end via `process_agent_responder_once` + wiremock (active version bound, runtime substitution with customer-name fallback, new save binds at next event, backfilled-legacy passthrough); `ai_agent_prompt.rs` RBAC tests for all 5 routes (view vs manage, per 017's precedent — `rbac.rs` untouched, see contracts/rest-api.md); `openapi_contract.rs` new paths/DTOs + agent-DTO `systemPrompt` removal; `shared/db/tests/schema.rs` 0045 assertions (tables, CHECKs, uniques, column drop). Frontend: `pnpm ng test dashboard` — store specs (load/save/409/no-op flows), scanner/renderer TS-mirror table-driven fixture shared with Rust cases, component specs (editor inline errors, variables insertion, preview chips, drawer restore confirm); lint/format/build gates

**Target Platform**: Linux server (backend) + evergreen-browser dashboard

**Project Type**: Web application — existing Cargo workspace backend + Angular dashboard frontend

**Performance Goals**: Prompt save/restore = one short transaction (parent-row lock, ≤ 3 single-row statements + audit insert); history list = one indexed descending scan per page; responder adds exactly one indexed single-row read (active content) + at most one indexed read (customer display) ahead of vendor latency, which dominates; preview/validation are client-side pure functions — zero network per keystroke (research R9)

**Constraints**: Deterministic rendering and composition — same `(content, vars)` ⇒ same bytes, no LLM anywhere in validation/rendering (Constitution IV); single write path — after 0045 no code can change prompt content without creating a validated version (FR-018, SC-004); versions immutable and append-only, restore is roll-forward only (FR-003/FR-006); every save/restore audited in-transaction, Owner/Admin only via existing `ai_agent.*` codes, cross-tenant → `not_found` (Constitution II/III); prompt content never in logs, traces, or audit details (015 invariant); schema via migration 0045 only; editor content never lost on rejected save (FR-011)

**Scale/Scope**: 1 migration, 2 new tables + 1 column drop; 5 new tenant endpoints, 0 new permission codes, 0 matrix changes; audit vocabulary +2 actions; backend ~3 new files in `modules/ai` (`prompt_store`, `prompt_validate`, `prompt_routes`) + focused edits (composer input, responder read, agent payload/DTO slimming, 1 conversations helper); frontend 1 new child page (~10 files under `features/tenant/ai-agent/prompt/`) + settings-card replacement; 1 new integration suite + 4 extended test files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Prompt tables, validation, routes, and rendering all live in `modules/ai`, which already owns the agent config and responder — one owner, no new cross-module edges. The single new cross-module need (customer display name for `{{customer_name}}`) is met by adding a `pub` helper to `conversations`' existing responder-helpers block, keeping the arrow `ai → conversations` (017's sanctioned pattern). Placeholder crate `modules/prompts` deliberately unused (R1) — extraction later remains a table+function move | ✅ Pass |
| II. Multi-Tenant Isolation | `tenant_id` NOT NULL on both new tables (versions carry it too, so no join is needed to enforce isolation on reads); all queries tenant-scoped; cross-tenant answers `not_found`; isolation cases in `ai_agent_prompt.rs` | ✅ Pass |
| III. Zero-Trust Security & RBAC | All five routes behind `require_permission` (`ai_agent.view`/`manage` — already Owner/Admin-only per 017's narrowing, which is exactly FR-015); every save/restore audited in the same transaction with actor attribution; prompt content excluded from audit details and logs; variable rendering is injection-safe by construction (inserted values never re-scanned) | ✅ Pass |
| IV. AI Provider Independence & Tool-Mediated Access | This feature *implements* the constitution's prompt-versioning mandate. Rendering and composition are pure deterministic functions (byte-equality tested); validation is a lexical scan, never LLM-delegated; the responder still only ever hands composed text to `AiService::complete` | ✅ Pass |
| V. API-First & Contract Consistency | Five REST endpoints in [contracts/rest-api.md](./contracts/rest-api.md) with the standard envelope/error vocabulary; PUT replay-safe (`created: false`), restore POST idempotent in effect via no-op detection; `baseVersion` makes concurrency explicit (017's 409 pattern); OpenAPI updated including the agent DTO change | ✅ Pass |
| VI. Observability by Default | Responder trace events gain `prompt_version`; save/restore handlers emit structured events (action, version numbers, latency) with request-id; content never logged | ✅ Pass |
| VII. Test-First & Regression Discipline | Unit (scanner/validation/render/composer determinism), integration (full endpoint + responder matrix), schema, RBAC map, OpenAPI contract, frontend store/component specs, shared Rust/TS validation fixture preventing mirror drift | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration 0045 only; 005 conventions on the parent table; versions table follows the `audit_logs` append-only precedent (no `updated_at`/soft delete — immutability is the requirement, not a shortcut); backfill + column drop keep one source of truth (removing 017's would-be dual-write risk); all query paths ride the two unique indexes | ✅ Pass |
| IX. Design System Discipline | Prompt page composes existing shared/Taiga-wrapped components and `--app-*` tokens; editor/panel/drawer are feature components following the 017 feature anatomy; the settings prompt card reuses the shared card patterns — no raw Taiga in feature pages, no duplicated UI logic (validation/render logic lives once in a pure TS util) | ✅ Pass |
| X. Performance & Efficiency | Save path is a short single transaction; history is cursor-paginated (no offsets, no N+1 — author display is denormalized onto the version row by design R8); responder hot path gains one indexed read; preview is client-side | ✅ Pass |

**Initial gate**: PASS — no deviations; Complexity Tracking intentionally empty.

**Post-design re-check (after Phase 1)**: PASS — the design introduced no deviations. Two calls worth surfacing: (1) dropping `agent_configurations.system_prompt` is a breaking change to 017's API DTOs, accepted deliberately because FR-018/SC-004 are unenforceable with a second writable copy, and the platform is pre-release with no external API consumers; (2) `created_by_display` on version rows is a denormalized snapshot, not a normalization violation — it records a historical fact (who the author *was* at save time) that a live join to `users` cannot reproduce (US5 scenario 2).

## Project Structure

### Documentation (this feature)

```text
specs/018-prompt-management/
├── plan.md                  # This file
├── research.md              # Phase 0 — R1–R12 decisions
├── data-model.md            # Phase 1 — agent_prompts, agent_prompt_versions, 0045 backfill + column drop, audit actions
├── quickstart.md            # Phase 1 — automated gates + 15-step manual walkthrough
├── contracts/
│   ├── rest-api.md          # 5 prompt endpoints + 017 agent-contract changes + RBAC matrix additions
│   └── prompt-runtime.md    # placeholder grammar, render/composition pipeline, runtime variable resolution, legacy edge
└── tasks.md                 # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0045_agent_prompts.sql          # NEW — agent_prompts + agent_prompt_versions + backfill + DROP system_prompt
└── crates/
    ├── modules/
    │   ├── ai/src/
    │   │   ├── lib.rs                  # MODIFIED — exports (prompt store/validate/routes)
    │   │   ├── prompt_store.rs         # NEW — parent/version queries: active-content read, save_version_in_tx
    │   │   │                           #        (FOR UPDATE lock, conflict check, no-op byte-compare, insert +
    │   │   │                           #        active_version bump), history page, version detail, restore
    │   │   ├── prompt_validate.rs      # NEW — variables catalog constant, placeholder scanner, validate_prompt
    │   │   │                           #        (required/too_long/malformed_placeholder/unknown_variable + offsets),
    │   │   │                           #        render_prompt (single-pass, injection-safe)
    │   │   ├── prompt_routes.rs        # NEW — 5 handlers (utoipa-annotated), audit calls, starter-default GET shape
    │   │   ├── agent_config.rs         # MODIFIED — payload/row/DTO lose system_prompt; create/update drop the bind;
    │   │   │                           #            agent GET gains activePrompt summary
    │   │   ├── agent_prompt.rs         # MODIFIED — composer consumes rendered content (signature unchanged otherwise)
    │   │   ├── agent_responder.rs      # MODIFIED — active-content read + runtime var resolution before compose;
    │   │   │                           #            trace event gains prompt_version; persona branch unchanged
    │   │   └── agent_audit.rs          # MODIFIED — agent_prompt.version_created / version_restored helpers
    │   └── conversations/src/queries.rs # MODIFIED — one pub responder-helper: conversation customer display name
    ├── shared/db/tests/schema.rs       # MODIFIED — 0045 assertions
    └── server/
        ├── src/router.rs               # MODIFIED — mount 5 prompt routes (view/manage split, 017 pattern)
        └── tests/
            ├── ai_agent_prompt.rs      # NEW — integration suite (see Testing)
            ├── ai_agent.rs             # MODIFIED — existing agent tests adjust to systemPrompt removal
            ├── rbac.rs                 # MODIFIED — +5 route→permission entries
            └── openapi_contract.rs     # MODIFIED — new DTOs/paths + agent DTO change

frontend/apps/dashboard/src/app/
├── core/router/
│   ├── app-paths.ts                    # MODIFIED — tenant.aiAgentPrompt ('ai-agent/prompt')
│   └── page-title.ts                   # MODIFIED — aiAgentPrompt entry
├── core/authz/permissions.ts           # MODIFIED — path→permission entry reusing ai_agent.view
├── features/tenant/tenant.routes.ts    # MODIFIED — child route registration
└── features/tenant/ai-agent/
    ├── ai-agent.component.ts|spec      # MODIFIED — prompt section becomes summary card → navigates to prompt page
    ├── ai-agent.store.ts|spec          # MODIFIED — systemPrompt removed from form state; activePrompt summary
    ├── ai-agent-api.service.ts|spec    # MODIFIED — agent DTO change
    ├── prompt-editor.component.ts|spec # REMOVED — superseded by the prompt page (FR-018)
    └── prompt/                         # NEW — the prompt management page
        ├── prompt-page.component.ts|spec        # layout: editor + variables panel + preview panel + drawer trigger
        ├── prompt.store.ts|spec                 # SignalStore: load (prompt+catalog), dirty state, save/restore,
        │                                        #   409 review flow, no-op notice, history pages
        ├── prompt-api.service.ts|spec           # typed client for the 5 endpoints
        ├── prompt-lang.ts|spec                  # pure TS scanner/validator/renderer mirror (shared fixture w/ Rust)
        ├── variables-panel.component.ts|spec    # catalog list + insert-at-cursor
        ├── preview-panel.component.ts|spec      # live sample-substituted preview, error chips
        └── version-history-drawer.component.ts|spec  # paginated list, detail/diff view, restore confirm
```

**Structure Decision**: Prompt management is agent configuration, so it extends the surfaces 017 built rather than opening new ones: backend code joins `modules/ai` beside the config/composer/responder it modifies (R1 — the placeholder `prompts` crate stays empty; a dedicated module would force cross-module transactional writes for zero isolation benefit), routes nest under `/tenant/ai/agent/prompt` behind the existing `ai_agent.*` permissions (R12), and the frontend page is a child route of the existing AI Agent feature directory, reusing its nav slot, guard wiring, and API-service/SignalStore anatomy. The one deliberate architectural move is R2: migration 0045 backfills history and then drops `agent_configurations.system_prompt`, making `agent_prompt_versions` the only place prompt content exists — FR-018 enforced by schema, not convention. The pure scanner/validator/renderer exists twice by contract (Rust authoritative, TS mirror for inline UX) with a shared table-driven fixture to prevent drift (R5).

## Complexity Tracking

No constitutional deviations — table intentionally empty.
