# Research: Prompt Management

**Feature**: 018-prompt-management | **Date**: 2026-07-16

Decisions R1–R12 resolve every open question in the Technical Context. Grounding facts: 017 is fully implemented — `agent_configurations.system_prompt` (TEXT ≤ 8000, may be empty) is today's active prompt, written by `PUT /tenant/ai/agent` under an `ai_agent.manage` + version-counter guard, and consumed by `agent_prompt::compose_system_message` inside the outbox responder (`agent_responder.rs`). The spec (FR-018, clarification #1) makes this feature the *only* prompt write path.

## R1 — Home module: `modules/ai`, not the placeholder `prompts` crate

**Decision**: All prompt-management code lands in `modules/ai` (new files `prompt_store.rs`, `prompt_validate.rs`, `prompt_routes.rs`; composer/responder edits in place). The M0 placeholder crate `modules/prompts` stays untouched.

**Rationale**: The system prompt is a property of the agent configuration `ai` already owns; the save transaction must atomically write a version row *and* retire/replace the config-side content (R2), and the responder composes from it every run. Housing it in `ai` keeps one owner for the prompt concept, zero cross-module transactional choreography, and the acyclic graph 017 established. Principle I extraction stays possible: the surface is plain `pub` functions + two tables, movable later exactly like any other module split.

**Alternatives considered**: `modules/prompts` as the owner — rejected: it would either write `agent_configurations` (forbidden cross-module write) or force a trait-port + same-transaction handshake with `ai` for no isolation gain; the placeholder crate has no routes, deps, or tests today. Revisit only when a second prompt *domain* (not a second prompt kind) appears.

## R2 — Single source of truth: drop `agent_configurations.system_prompt`

**Decision**: Prompt content lives only in `agent_prompt_versions`. Migration 0045 backfills version 1 from each live agent row whose `system_prompt` is non-empty, then `DROP COLUMN system_prompt`. `AgentConfigPayload`/DTOs lose `systemPrompt`; the agent PUT no longer accepts prompt content (FR-018). The responder loads active content via one indexed query.

**Rationale**: FR-018 + SC-004 demand that *no* path can change the prompt without versioning/validation. A mirrored column is a standing drift risk and a second writer; a column that exists but is ignored is a trap. One indexed single-row read per responder run is noise next to vendor latency (017 plan's own performance framing).

**Alternatives considered**: (a) keep the column as a same-transaction mirror — rejected: dual-write drift risk, and every future writer must remember the invariant; (b) keep the column, make agent PUT reject changes to it — rejected: dead field in the contract, confusing 422s.

## R3 — Schema shape: tenant-keyed `agent_prompts` + append-only `agent_prompt_versions`

**Decision**: Parent row `agent_prompts` (`tenant_id`, `prompt_kind = 'system'`, `active_version`, partial-unique `(tenant_id, prompt_kind)`) plus append-only `agent_prompt_versions` (`prompt_id`, `version_number`, content, author snapshot, `restored_from`, `UNIQUE (prompt_id, version_number)`). `active_version` always equals the highest `version_number` in v1 (save = activate, clarification #2) but is stored explicitly. The parent row is created lazily on first save (spec edge case: unconfigured tenant's first save creates version 1 — works even before an `agent_configurations` row exists).

**Rationale**: Tenant-keying avoids a nullable-`agent_id`-link-later dance while v1 caps both agents and prompts at one per tenant; the `prompt_kind` discriminator plus a documented additive migration (add `agent_id`, backfill from the default agent, extend the unique index) is the future multi-prompt/multi-agent unlock — additive, no redesign, mirroring 017's droppable-index pattern. Explicit `active_version` makes GET/conflict checks single-row and leaves the draft/publish door open (spec defers it). `UNIQUE (prompt_id, version_number)` makes concurrent saves race-safe at the database, not just the app.

**Alternatives considered**: versions keyed by `agent_id` — rejected: prompt must be savable before the agent row exists; no parent table (versions only, active = MAX) — rejected: no place for the v1-cap index, per-request MAX scans, and no future active-pointer flexibility; storing diffs instead of full content — rejected: full snapshots are the immutability requirement and prompts are ≤ 8 KB.

## R4 — Variable syntax and catalog

**Decision**: Placeholder syntax `{{variable_name}}` (lowercase snake_case names). v1 catalog is a code constant in `modules/ai`, containing only variables resolvable at composition time today:

| Name | Description | Sample (preview) | Runtime source | Runtime fallback |
|---|---|---|---|---|
| `agent_name` | The AI agent's customer-facing name | `Aria` | agent config row (or platform persona name) | persona default |
| `tenant_name` | The tenant's business name | `Acme Support` | `tenancy::authorize::fetch_tenant` | `""` never — tenant always exists |
| `customer_name` | The customer's display name | `Jamie Lee` | conversations-owned helper (customer display for the conversation) | `the customer` |
| `channel` | The conversation's channel | `web_chat` | outbox event payload | n/a — always present |

**Rationale**: `{{…}}` is the ubiquitous, user-recognizable convention and trivially lexable with a deterministic scanner. The catalog is deliberately restricted to values the responder can resolve deterministically from data it already touches — `business_hours` (a spec example) is excluded because no tenant business-profile field exists anywhere in the schema; it joins the catalog when such a field ships. Adding a variable later is a one-constant change; validation and preview are catalog-driven.

**Alternatives considered**: `{var}` / `%var%` — rejected: collide more easily with natural prose and JSON braces are already the platform's visual language; DB-stored catalog — rejected: variables require code to resolve them anyway, so a table adds drift without flexibility (constitution-IV determinism favors code constants, same as 017's tone/phrase catalogs).

## R5 — Validation rules (server-authoritative, client-mirrored)

**Decision**: One pure function `validate_prompt(content) -> Result<(), Vec<ValidationIssue>>` in `modules/ai`, reusing 017's `ValidationIssue {field, code, message}` shape, enforcing: non-empty after trim (`required`), ≤ 8000 chars (`too_long`, keeping 017's limit), well-formed placeholders (`malformed_placeholder` — unclosed `{{`, stray `}}`, empty or non-snake-case name; each with character offset), and catalog membership (`unknown_variable`, naming the offender and offset). Runs on save **and** restore (FR-010; spec edge case: restore of a version referencing a removed variable is blocked). The frontend mirrors the same rules in a pure TS function for inline feedback; the server remains authoritative.

**Rationale**: Identical semantics both sides gives the spec's inline-while-typing UX without trusting the client; offsets let the editor point at the exact fragment. Literal braces in prose: a single `{` or `}` is legal; only `{{`/`}}` sequences enter placeholder lexing — documented in the contract so the scanner is deterministic and testable.

**Alternatives considered**: regex-only validation — rejected: cannot report offsets/nesting cleanly; escaping syntax for literal `{{` — deferred: no evidence tenants need literal `{{` in prompts; revisit on demand rather than inventing escape grammar now.

## R6 — Concurrency and no-op semantics

**Decision**: Save/restore carry `baseVersion` (the `active_version` the client loaded). In one transaction: `SELECT … FOR UPDATE` the parent row (matching 017's `load_live_in_tx` pattern), reject `409 version_conflict` if `active_version != baseVersion`, detect no-op (`content` byte-equal to active version's content → `200 {created: false}`, no version, no audit — FR-013), else insert version `baseVersion + 1` and set `active_version`. The DB unique on `(prompt_id, version_number)` backstops any race. First-ever save uses `baseVersion = 0`.

**Rationale**: Identical developer/UX contract to 017's agent 409 flow (frontend already has the conflict pattern); the unique index turns the residual race into a constraint error rather than silent loss (spec US1 scenario 5, SC-006).

**Alternatives considered**: last-write-wins — rejected by spec; content-hash comparison for no-op — unnecessary, byte-compare of ≤ 8 KB strings is free.

## R7 — History pagination

**Decision**: `GET …/prompt/versions?limit=<1..100, default 25>&before=<version_number>` returning newest-first `items` + `hasMore`, cursoring on `version_number` (strictly `< before`). List items carry metadata + a server-computed `contentPreview` (first 160 chars, single-line); full content comes from the detail endpoint.

**Rationale**: `version_number` is a dense, unique, monotonic cursor — simpler and more stable than timestamp cursors used where no such key exists; matches the platform's cursor-pagination convention (conversations/escalations). Preview-in-list keeps the drawer light for long histories (spec US2 scenario 5).

**Alternatives considered**: offset pagination — rejected: platform uses cursors and offsets skew under concurrent inserts; full content in list — rejected: 8 KB × N payloads for a drawer that shows one line each.

## R8 — Author attribution and audit

**Decision**: Version rows store `created_by_user_id` (nullable FK) **and** a `created_by_display` text snapshot captured at save time; history renders the snapshot (spec US5 scenario 2 — author visible after deactivation; also covers platform users acting in tenant context, and `Migration backfill` for 0045's version 1). Additionally each save/restore writes `tenancy::audit::record_in_tx` in the same transaction: actions `agent_prompt.version_created` / `agent_prompt.version_restored`, resource `agent_prompt`, details = version numbers, content length, `restored_from`, change-note presence — **never the prompt content itself** (015 invariant: prompt content stays out of logs/audit payloads; the versions table is its one home).

**Rationale**: The snapshot decouples display from the users table's future lifecycle; the audit trail stays the platform's single append-only who/what/when ledger (FR-014) exactly as 017 did for config changes.

**Alternatives considered**: joining `users` at read time — rejected: display then depends on user-row survival and platform-actor names aren't tenant-membership rows; full content in audit details — rejected: duplicates the versions table into a table that feeds admin UIs and exports.

## R9 — Preview is client-side substitution; catalog rides the GET

**Decision**: No preview endpoint. `GET …/prompt` returns the variables catalog (name, description, sample) alongside the active prompt and limits; the preview panel substitutes samples into the *current editor content* with a pure TS renderer that mirrors R5's scanner, live on each edit, marking unknown/malformed placeholders visually instead of rendering them as valid (FR-009). Historical-version preview reuses the same renderer.

**Rationale**: Substitution is a deterministic string operation over a catalog the client already holds — a server round-trip per keystroke buys nothing (spec: preview uses sample values only, never live AI calls or real customer data). Single GET keeps the editor to one load.

**Alternatives considered**: server-side preview endpoint — rejected: latency for identical output; embedding samples in a separate `/variables` endpoint — rejected: second round-trip for data that never changes within a session.

## R10 — 017 supersession seam

**Decision**: (a) `AgentConfigPayload` and agent DTOs drop `systemPrompt`; agent GET gains a read-only `activePrompt` summary `{version, updatedAt, updatedBy, excerpt}` (or `null`) so the settings page can render a summary card. (b) The settings page's inline `prompt-editor.component` is replaced by that card, which navigates to the new prompt page (`ai-agent/prompt` child route). (c) `agent_config::create_in_tx` no longer receives prompt content — the agent row and the prompt object are fully decoupled; first agent save and first prompt save can happen in either order. (d) The responder's configured branch loads active content by tenant (one query); the platform-persona branch passes empty content exactly as today.

**Rationale**: This is clarification #1 made mechanical: after this change there is *no* code path that writes prompt content outside `prompt_store::save_version_in_tx`. Decoupling create-order satisfies the spec edge case (prompt page usable before the agent is configured) without a provisional-agent hack.

**Alternatives considered**: agent PUT delegating internally to the prompt save (keeping `systemPrompt` in its payload) — rejected: two contracts for one action, ambiguous audit attribution, and the spec says the settings surface navigates into prompt management, not that it wraps it.

## R11 — Restore semantics

**Decision**: `POST …/prompt/versions/{number}/restore` with `baseVersion`; re-runs R5 validation on the source content (blocked with the standard 422 if the catalog has since shrunk), applies R6 conflict/no-op rules (restoring content identical to active → `created: false`), and on success inserts a new version with `restored_from = {number}` and audit action `agent_prompt.version_restored`. History is never rewritten.

**Rationale**: Roll-forward restore is the spec's explicit model (US2 scenario 3, assumption "restore is roll-forward"); `restored_from` gives the history drawer its "Restored from v3" badge for free.

**Alternatives considered**: restore-as-pointer-move (set `active_version` to the old number) — rejected by spec: history must remain a linear record and the restore itself must be a visible, audited event.

## R12 — Routes, permissions, page

**Decision**: Five endpoints under the existing prefix — `GET/PUT /tenant/ai/agent/prompt`, `GET /tenant/ai/agent/prompt/versions`, `GET /tenant/ai/agent/prompt/versions/{number}`, `POST /tenant/ai/agent/prompt/versions/{number}/restore` — mounted in `router.rs` via the same `routes!().map(require_permission)` pattern: GETs behind `ai_agent.view`, PUT/restore behind `ai_agent.manage`. No new permission codes and no matrix change: 017's R11 narrowing already reduced `ai_agent.*` to Owner/Admin, which is exactly FR-015. Frontend: child route `ai-agent/prompt` (new `page-title` entry; permission-map entry reusing `ai_agent.view`), feature files under `features/tenant/ai-agent/prompt/`.

**Rationale**: The prompt is agent configuration — same resource family, same permission vocabulary, same Owner/Admin audience; a new permission code would fragment the "AI settings" access story 017 deliberately unified. Nesting the route keeps nav (one "AI Agent" item) and guard wiring untouched.

**Alternatives considered**: new `prompt.view/manage` codes — rejected: no role in any spec distinguishes prompt access from agent access; top-level `/tenant/prompts` path — rejected: implies a prompt library that v1 (one system prompt) is not.
