# Data Model: AI Agent Configuration

**Feature**: 017-ai-agent-config | **Date**: 2026-07-16

Two new tables, one vocabulary extension, and one conversations column. Migrations `0041`–`0043`; the agent tables belong to `modules/ai`, the message/conversation changes to `modules/conversations`. Conventions per 005 (UUID v7 PKs, `created_at`/`updated_at` TIMESTAMPTZ with `set_updated_at` trigger, soft delete via `deleted_at`, partial unique indexes).

## Catalogs (code, mirrored as CHECKs)

- **Tone**: `professional | friendly | casual | formal | empathetic` (R10). Default `professional`.
- **Channels**: the conversations vocabulary — `email | phone | web_chat | whatsapp | telegram` (`conversations_channel_check`, 0026). `enabled_channels` may only contain these values (validated in code; stored as JSONB array).
- **Providers**: 015's fixed catalog `openai | anthropic | gemini`; agent-level override columns reuse the same CHECK.
- **Curated models**: per-provider code constant in `ai-providers` registry (R5); not stored.
- **Human-request phrases**: code constant in `modules/ai` (R7); not stored.

## `agent_configurations` (0041)

The tenant's AI agent. Multi-agent-shaped from day one; v1 adds one droppable index to cap it at one live row per tenant (R2). Row existence = agent configured/active (clarification: inactive until first save).

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` | Constitution II — never NULL here (no platform-scope agent) |
| `name` | TEXT | NOT NULL, non-empty, ≤ 80 chars (CHECK) | customer-visible agent name |
| `is_default` | BOOLEAN | NOT NULL DEFAULT true | always true in v1; the multi-agent future selects among rows by this flag |
| `avatar_kind` | TEXT | NOT NULL DEFAULT `'preset'`, CHECK in (`'preset'`,`'upload'`) | |
| `avatar_preset` | TEXT NULL | | preset key; NULL when kind = upload |
| `tone` | TEXT | NOT NULL DEFAULT `'professional'`, CHECK in tone catalog | R10 |
| `system_prompt` | TEXT | NOT NULL, CHECK `char_length ≤ 8000` | may be empty string (defaults shown client-side are explicit content once saved) |
| `business_rules` | JSONB | NOT NULL DEFAULT `'[]'` | ordered array of strings; code-validated: ≤ 20 entries, each non-empty ≤ 500 chars (R3) |
| `escalation_rules` | JSONB | NOT NULL DEFAULT `'[]'` | ordered array of rule objects (shape below); code-validated (R3) |
| `enabled_channels` | JSONB | NOT NULL DEFAULT `'[]'` | array of channel-catalog values, no duplicates; empty = agent responds nowhere (US4 scenario 3) |
| `provider` | TEXT NULL | CHECK in provider catalog | agent-level override; NULL = follow tenant AI-layer configuration (R5) |
| `model` | TEXT NULL | non-empty when set; CHECK (`provider IS NULL`) = (`model IS NULL`) | override is all-or-nothing pair |
| `version` | INTEGER | NOT NULL DEFAULT 1 | optimistic concurrency (R8); incremented by the update statement, not a trigger |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | `set_updated_at` trigger |
| `deleted_at` | TIMESTAMPTZ NULL | | soft delete (future multi-agent removal path; no v1 delete route) |

**Escalation rule JSONB shape** (validated in code, not by CHECK):

```json
{
  "id": "uuid — generated server-side on save if absent",
  "name": "string, non-empty, ≤ 80 chars, unique within the array",
  "trigger": "human_request | topic_keywords",
  "keywords": ["non-empty strings, ≤ 40 chars each; required non-empty iff trigger = topic_keywords, must be [] for human_request"],
  "required_skill_ids": ["uuid — each must exist live in this tenant's skills at save time"]
}
```

**Indexes**

- `UNIQUE (tenant_id) WHERE deleted_at IS NULL` — **the v1 single-agent cap**; dropping this index is the entire multi-agent unlock (R2, SC-006).
- `UNIQUE (tenant_id) WHERE is_default AND deleted_at IS NULL` — exactly one live default per tenant; survives multi-agent.
- `UNIQUE (tenant_id, lower(name)) WHERE deleted_at IS NULL` — per-tenant unique agent names (FR-003).

**State transitions**: absent (agent inactive, conversations flow to humans) → created on first successful PUT (agent active on enabled channels) → updated in place (version increments, audited) → soft-deleted (future; not reachable in v1). The responder reads the live row once per customer-message event, so config changes bind at the next response (FR-016).

**Derived-on-read (not stored)**:
- `broken_skill_refs` — per escalation rule, `required_skill_ids` no longer live in `skills` (R3; US3 scenario 4).
- `stale_selection` — the override `provider`/`model` no longer usable (provider credential unresolvable); live traffic falls back to the tenant AI-layer default (FR-008).

## `agent_avatar_uploads` (0041)

Uploaded avatar bytes, kept off the hot config row (R4). One live upload per agent.

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` | isolation on the serving path |
| `agent_id` | UUID | NOT NULL, FK → `agent_configurations(id)` | |
| `content_type` | TEXT | NOT NULL, CHECK in (`'image/png'`,`'image/jpeg'`,`'image/webp'`) | |
| `bytes` | BYTEA | NOT NULL, CHECK `octet_length(bytes) <= 262144` | ≤ 256 KB (R4) |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | trigger; replacement = UPDATE in place |
| `deleted_at` | TIMESTAMPTZ NULL | | cleared when tenant reverts to preset |

**Indexes**: `UNIQUE (agent_id) WHERE deleted_at IS NULL`.

**Invariant (code-enforced)**: `agent_configurations.avatar_kind = 'upload'` requires a live `agent_avatar_uploads` row; the avatar upload endpoint writes both sides in one transaction, so a failed upload never flips `avatar_kind` (spec edge case: failed upload keeps the previous avatar and loses nothing else).

## Messages vocabulary extension (0042)

`messages_kind_check` gains `'ai'` **and** `'system'`; `messages_kind_consistency` gains the arms `kind IN ('ai','system') AND sender_membership_id IS NULL AND logged_by_membership_id IS NULL` (R9, R13). No new columns.

- `ai` — LLM-generated agent reply. Participant projection `{type: "ai_agent", display_name: <agent name at send time — the tenant agent's name, or the platform default persona name under `ai_handling='platform_ai'`>, id: null}`.
- `system` — platform-authored automatic message; v1's only use is the unconfigured auto-acknowledgment (fixed code-constant text). Participant projection `{type: "system", display_name: "Automated reply", id: null}`. Never LLM output (R13).

Preview/timeline queries treat both like `reply` for last-activity purposes.

## `conversations.ai_handling` (0043)

`ALTER TABLE conversations ADD COLUMN ai_handling TEXT NULL CHECK (ai_handling IN ('platform_ai','human'))`.

Per-conversation fallback decision while the tenant's agent is unconfigured (R13, FR-004b): `NULL` = undecided (awaiting decision iff the tenant has no live agent and a `system` auto-ack exists), `platform_ai` = platform default persona answers via the AI layer, `human` = escalated at decision time; responder skips. Consulted **only** when no live `agent_configurations` row exists (FR-004c). No index — always read via the conversation's PK on already-loaded rows.

**Auto-ack idempotency**: no sent-flag column; "acknowledged" = a `system`-kind message exists in the conversation (the responder checks before inserting, and replays hit the same guard).

## Outbox event (no migration — `outbox_events` exists)

`event_type = 'conversation.customer_message'`, `aggregate_type = 'conversation'`, `aggregate_id = conversation id`, `tenant_id` set, payload `{conversation_id, message_id, channel}`. Emitted transactionally with every `customer`-kind message insert (both `add_message` and `create_conversation`'s initial message). Consumed by the agent responder worker in `modules/ai` (R6) using the same claim pattern as the escalation outbox worker.

## Audit actions (existing `audit_logs`, via `tenancy::audit::record_in_tx`)

| Action | Resource | Payload (FR-014) |
|---|---|---|
| `agent_config.created` | `agent_configuration` | agent id, name, full non-sensitive field snapshot |
| `agent_config.updated` | `agent_configuration` | agent id, changed-field names, old/new for scalar fields, counts for rule arrays |
| `agent_config.avatar_updated` | `agent_configuration` | agent id, kind, preset key or content-type+size |
| `conversation.ai_handling_set` | `conversation` | conversation id, chosen mode (`platform_ai`/`human`); written by the `ai` crate's `agent_routes::set_conversation_ai_handling` handler (see tasks.md T046) — it is the only crate that reaches both `conversations` and `escalations` without a dependency cycle (FR-004b "audited") |

Actor attribution (tenant member vs platform user in tenant context) rides the existing audit helper — no changes needed for US5 scenario 3.

## Relationships

```text
tenants 1 ─── * agent_configurations (v1: ≤ 1 live)      [modules/ai]
agent_configurations 1 ─── 0..1 agent_avatar_uploads      [modules/ai]
agent_configurations.escalation_rules[].required_skill_ids ──> skills (soft refs, staleness-flagged on read)
agent_configurations.provider/model ──(fallback)──> ai_configurations resolution (015)
conversations ── outbox_events(customer_message) ──> agent responder ──> messages(kind='ai')
unconfigured tenant: responder ──> messages(kind='system' auto-ack) ──> conversations.ai_handling
                     ├─ 'platform_ai' ──> platform default persona via 015 resolution ──> messages(kind='ai')
                     └─ 'human' ──> escalations routing (reason: no AI agent configured)
```
