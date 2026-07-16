# Research: AI Agent Configuration

**Feature**: 017-ai-agent-config | **Date**: 2026-07-16

Decisions resolving every open design question for the plan. No NEEDS CLARIFICATION markers remained in the Technical Context; these entries record the choices and the alternatives weighed.

## R1 — Where the agent configuration lives: `modules/ai`

**Decision**: The agent configuration model, routes, prompt composition, and the customer-message responder all go into the existing `backend/crates/modules/ai` crate. No new module crate.

**Rationale**: `modules/ai` already owns the AI admin surface (015: `ai_configurations`, `ai_credentials`, usage), the `ai_agent.view/manage` permission pair guards it, and the responder must call `AiService`, which lives there — putting the agent config elsewhere would force a second crate to depend on `modules/ai` for no boundary gain. The agent is "how the tenant's AI behaves", squarely this module's Purpose. The placeholder `prompts` module stays empty: prompt *versioning* is its future concern; deterministic prompt *composition* from a config row is agent behavior.

**Alternatives considered**: New `modules/agents` crate — rejected: it would own one table and immediately need `modules/ai` types both ways (config → service, responder → config), inviting a cycle or an anemic crate. `modules/prompts` — rejected: this feature has no prompt versioning; adopting the crate now would misname its contents.

## R2 — v1 "exactly one agent" without blocking multi-agent later

**Decision**: The `agent_configurations` table is multi-agent-shaped from day one: `(tenant_id, lower(name))` partial-unique for named agents, `is_default` with a partial-unique `(tenant_id) WHERE is_default AND deleted_at IS NULL` guaranteeing exactly one default. The v1 single-agent rule is enforced by **one additional partial unique index** `(tenant_id) WHERE deleted_at IS NULL` plus upsert-only route semantics (PUT, no POST-create of additional agents). Multi-agent later = drop that one index and add list/create routes; zero data migration (SC-006).

**Rationale**: An index is the cheapest possible enforcement that is also trivially removable. Rows created in v1 are already valid multi-agent rows (named, default-flagged).

**Alternatives considered**: Code-only enforcement — rejected: a race on first-configure could create two rows; the constitution treats isolation/integrity invariants as DB-enforced. Separate `tenant_default_agent` pointer table — rejected: more moving parts for the same guarantee, and 015 set the precedent of partial unique indexes for one-live-row-per-scope.

## R3 — Business rules and escalation rules as JSONB on the agent row

**Decision**: `business_rules JSONB` — ordered array of strings (≤ 20 rules, each ≤ 500 chars). `escalation_rules JSONB` — ordered array of objects `{id (uuid), name, trigger: "human_request" | "topic_keywords", keywords: [string], required_skill_ids: [uuid]}` (≤ 20 rules; `keywords` required non-empty iff trigger = `topic_keywords`). Both validated in code on save; skill references validated against the tenant's `skills` table on save and re-checked on read so deleted skills surface as a `broken_skill_refs` flag (spec US3 scenario 4).

**Rationale**: Both lists are small, ordered, always read whole-row (settings page and responder each load the full config), and never queried relationally — the same profile as 015's `fallbacks JSONB`, which passed the constitution gate with that justification. Child tables would add two joins to the responder's hot path and CRUD fan-out for no query we ever run.

**Alternatives considered**: Child tables `agent_business_rules` / `agent_escalation_rules` — rejected per above; revisit only if rules ever need independent audit trails or cross-agent reuse. FK-enforced skill refs — impossible inside JSONB; on-read staleness flagging matches how the spec wants broken refs handled (surfaced, not blocking).

## R4 — Avatar storage: presets by key, uploads as bounded rows in PostgreSQL

**Decision**: `avatar_kind ∈ {preset, upload}`. Presets are a fixed frontend asset catalog referenced by key (`avatar_preset TEXT`). Uploads go in a separate `agent_avatar_uploads` table (BYTEA, ≤ 256 KB, content-type CHECK in `image/png|image/jpeg|image/webp`), one live row per agent, served by a dedicated authenticated GET. The agent row stores only the reference.

**Rationale**: The repo has **no object-storage integration yet** (no S3 client, no storage config fields) — introducing S3 infrastructure for one cosmetic ≤256 KB image is disproportionate to the feature. A separate table keeps blob bytes off the config row that the responder reads per customer message. Moving to object storage later is a backfill + reference swap, invisible to the API. Recorded as a justified deviation from the constitution's "S3-compatible object storage" stack line in plan.md Complexity Tracking.

**Alternatives considered**: Presets only — overruled by clarification (upload is in v1). Stand up S3/MinIO now — rejected: new infra, new config surface, new failure modes, all for a thumbnail; the widget/branding features that genuinely need object storage should drive that adoption. Data-URL stored inline on the agent row — rejected: bloats the hot row and every config GET.

## R5 — Provider/model options: credential-gated providers + curated model catalog

**Decision**: New endpoint `GET /tenant/ai/agent/options` returns, per catalog provider (`openai`, `anthropic`, `gemini`): whether a credential resolves for this tenant (BYOK or platform key — reusing 015's resolution), a curated list of known model identifiers (code constant in `ai-providers`' registry, e.g. current GPT/Claude/Gemini chat models), and the tenant's AI-layer default provider/model. The selector offers only credential-backed providers (FR-007). The agent row stores an optional `provider`/`model` **override**; empty = follow the tenant's AI-layer configuration.

**Rationale**: 015 deliberately made `model` free text at the AI-layer admin surface; the tenant-facing agent page needs a friendlier, bounded picker, and a code catalog is the only source of model names that exists (no vendor list-models calls in the adapters, and adding them would drag vendor I/O into a settings GET). "Available to the tenant" is exactly "a credential resolves", which 015's resolution already computes.

**Alternatives considered**: Live vendor model-list APIs — rejected: three new adapter surfaces, latency and failure modes on a settings page, and Gemini/OpenAI lists include non-chat models needing filtering anyway. Reusing the tenant `ai_configurations` row as the only choice (no agent-level override) — rejected: FR-007 makes provider/model an agent-level setting; the override + fallback design also gives FR-008 its "fall back to the AI-layer default" semantics for free.

## R6 — Responder integration: outbox event + worker, not in-request

**Decision**: When a `customer`-kind message is inserted (via `add_message` or `create_conversation`'s initial message), the conversations module emits an `outbox_events` row (`event_type = "conversation.customer_message"`) in the same transaction — extending its existing `outbox.rs`. A new agent-responder worker in `modules/ai` (mirroring `escalations::events::run_escalation_outbox_worker` / `process_..._once` test pattern) claims these events and runs the pipeline: load live agent config → if none, run the unconfigured-fallback branch (R13: one-time auto-acknowledgment, honor the per-conversation `ai_handling` decision) → otherwise gate on channel enabled → evaluate escalation rules → either escalate or compose the prompt, call `AiService::complete`, and insert the AI reply message. Integration tests drive `process_agent_responder_once` deterministically.

**Rationale**: Keeps AI vendor latency entirely off the staff/customer request path (Constitution X), reuses the outbox pattern and worker scaffolding this repo already runs for escalations, survives crashes (event stays unprocessed), and honors module boundaries: conversations knows nothing about AI — it emits a domain event; `modules/ai` consumes it (Constitution I).

**Alternatives considered**: Synchronous reply inside `add_message` — rejected: couples a staff API's latency to vendor latency and violates the module boundary (conversations would call AiService). Tokio `spawn` fire-and-forget — rejected: lost on crash, untestable deterministically, no retry story. Redis queue — rejected: outbox already exists and is transactional with the message insert.

## R13 — Unconfigured-tenant fallback: auto-acknowledgment + per-conversation staff decision

**Decision** (supersedes the "unconfigured = pure silence" consequence originally drawn from clarification #2; spec FR-004a–c): while a tenant has no live agent configuration, the responder answers the **first** customer message of a conversation with a one-time automatic acknowledgment — fixed platform text, stored as a `system`-kind message — and the conversation enters an awaiting-decision state. A new conversations column `ai_handling` (`NULL` = undecided, `'platform_ai'`, `'human'`) records the per-conversation staff choice, set through a new conversations endpoint guarded by `conversations.manage`:

- `platform_ai` → subsequent customer messages run the normal responder pipeline using the **platform default persona** (the same code-constant template the settings GET shows) with the 015 AI-layer resolution (platform default config + platform keys, or tenant BYOK if present). Baseline human-request escalation still applies. Selectable only when the AI layer resolves for the tenant.
- `human` → an escalation is created immediately through the 014 routing entry (reason: fixed "no AI agent configured" string); the responder ignores the conversation thereafter.

Once the tenant saves its own agent, the live agent config supersedes `ai_handling` entirely (checked before it — FR-004c); the column simply stops mattering.

**Rationale**: Customers get an immediate acknowledgment instead of silence; tenants get AI value before configuring anything, but a human explicitly opts each conversation in — the "never reviewed by a human" liability that motivated clarification #2 is answered by the staff decision instead of by inactivity. Per-conversation state on the conversation row (not a tenant-level mode) matches the user's description ("the manager chooses… the conversation") and needs no new table. `conversations.manage` is the right guard: this is conversation handling, not AI settings, so Managers/Agents can act on it while AI settings stay Owner/Admin (R11 untouched).

**Alternatives considered**: Tenant-level one-time choice — rejected: coarser than the stated flow and forces an all-or-nothing call before the tenant has seen the AI perform. Auto-reply as `ai` kind — rejected: it is not LLM output and must not masquerade as the agent; a `system` kind keeps timeline semantics honest (R9 extended: 0042 adds both `ai` and `system`). Auto-reply on every message while undecided — rejected: spammy; once per conversation acknowledges receipt without pretending to converse.

## R7 — "Explicit human request" detection: built-in phrase catalog

**Decision**: The `human_request` trigger matches a fixed, code-owned catalog of case-insensitive phrases ("talk to a human", "speak to an agent", "real person", …) against the customer message. Tenants cannot edit the catalog in v1 but can add their own `topic_keywords` rules for anything beyond it. The catalog is a code constant, documented in the runtime contract, and the baseline rule is always evaluated even when the tenant has defined no rules (FR-011: rules extend, never disable, the baseline).

**Rationale**: Deterministic, testable, zero-cost, language-expandable later. Using the LLM to detect "wants a human" would make the escalation safety valve depend on the very component it guards against (and on vendor availability).

**Alternatives considered**: LLM-classified intent — rejected per above (also non-deterministic, violating the spirit of Constitution IV's determinism demand). Tenant-editable phrase list for the baseline — rejected: FR-011 requires the baseline to be non-disableable; merging tenant edits into it blurs that guarantee.

## R8 — Concurrent-edit protection: integer `version` column

**Decision**: `agent_configurations.version INTEGER NOT NULL DEFAULT 1`, incremented on every UPDATE. `GET` returns it; `PUT` requires it; mismatch → `409 conflict` with the standard error envelope (FR-017). First-ever save sends `version: null`/omitted and creates the row.

**Rationale**: Explicit and self-documenting versus comparing `updated_at` timestamps (which collide at trigger resolution and are awkward to echo through forms). Matches the spec's "second saver is told the configuration changed underneath them".

**Alternatives considered**: `updated_at`-as-ETag — rejected: fragile equality on timestamps, and the value already changes via trigger on any column. Last-write-wins — forbidden by FR-017.

## R9 — AI reply representation: new `ai` message kind

**Decision**: Extend the messages vocabulary with `kind = 'ai'` (migration updates `messages_kind_check` and `messages_kind_consistency`: `ai` rows carry NULL `sender_membership_id`/`logged_by_membership_id`, like `customer`). Timeline/preview projections render the sender participant as `{"type": "ai_agent", display_name: <agent name>, id: null}`. The addition is additive for API consumers; the dashboard timeline gains an AI style variant.

**Rationale**: AI replies are a first-class sender role, not a staff reply — attributing them to a synthetic membership would corrupt assignee semantics and RBAC assumptions. An enum extension plus CHECK migration is the established pattern (0034 defined the vocabulary; 0042 extends it).

**Alternatives considered**: Reuse `reply` with a marker column — rejected: `reply` requires a real `sender_membership_id` by CHECK, and consumers key styling/attribution off `kind`. Separate `ai_messages` table — rejected: destroys the single-timeline query model.

## R10 — Tone: fixed five-value catalog mapped to deterministic prompt directives

**Decision**: `tone TEXT CHECK IN ('professional','friendly','casual','formal','empathetic')`, default `professional`. Each tone maps to a fixed directive sentence in the prompt composer (code constant table). The tone list ships in `GET /tenant/ai/agent/options` so the frontend never hardcodes it.

**Rationale**: The spec assumes a curated set; a DB CHECK mirrors how 015 pinned the provider catalog. Directive text in code keeps composition deterministic (Constitution IV) and reviewable.

**Alternatives considered**: Free-text tone — rejected by spec assumption (it's just a second system prompt). Tones table — rejected: no tenant-specific tones in v1, catalog changes are code changes like providers.

## R11 — RBAC narrowing to Owner/Admin

**Decision**: Per the spec clarification, remove `AiAgentView`/`AiAgentManage` from `TENANT_MANAGER`, and `AiAgentView` from `TENANT_VIEWER` and `STAFF_PRODUCTION_DEVELOPER` in `authz::matrix`. `TENANT_ADMIN` (used by Owner and Admin) keeps both. Platform users acting in tenant context retain access through the existing platform-context path used by 015's routes. `rbac.rs` matrix expectations and any dashboard nav visibility driven by `ai_agent.view` update accordingly.

**Rationale**: FR-013 names Owner/Admin exclusively. This also tightens 015's AI provider/usage endpoints (same permission pair) — a deliberate, spec-driven consequence: AI configuration including keys and usage is sensitive enough that the narrowing is coherent, and no spec ever promised Manager/Viewer access (the old matrix predates the clarification).

**Alternatives considered**: New `ai_agent_config.*` permission codes leaving 015's untouched — rejected: two permission pairs guarding one conceptual surface ("the AI agent") invites drift, and the clarification's intent plainly covers the whole AI settings area.

## R12 — Prompt composition template

**Decision**: Deterministic, fixed-order composition (documented in `contracts/agent-runtime.md`): (1) agent system prompt verbatim; (2) tone directive; (3) business rules as a numbered "You must always follow these rules" block, in stored order; (4) fixed platform guardrail line (identity: agent name, honesty about being an AI). Same config in → byte-identical system message out. Customer-visible content (the conversation transcript) is passed only as user/assistant messages, never merged into the system prompt.

**Rationale**: Constitution IV requires deterministic prompt construction; a fixed template with ordered inputs satisfies it and makes FR-009 ("deterministically incorporated") concrete and unit-testable.

**Alternatives considered**: Template stored in DB — rejected: nothing edits it in v1; it would be config without a consumer. Injecting rules as a separate system message per rule — rejected: message-count varies by config, harder to reason about with vendor system-message quirks (Gemini single-system-instruction, etc. — 015 adapters normalize one system message).
