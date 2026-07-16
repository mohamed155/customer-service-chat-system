# Tasks: AI Agent Configuration

**Input**: Design documents from `/specs/017-ai-agent-config/` (spec.md, plan.md, research.md, data-model.md, contracts/rest-api.md, contracts/agent-runtime.md, quickstart.md)

**Tests**: Included — the plan and constitution (Principle VII) require unit + integration coverage; every story phase below has a Tests subsection. Write each test, run it, watch it fail (for new behavior) before implementing.

**Organization**: Tasks are grouped by user story (spec.md priorities P1/P2/P3). **Read this before starting**: this feature has one shared, order-sensitive execution pipeline (`agent_responder.rs`) that every story's acceptance scenarios run through. That pipeline is built ONCE, completely, in Phase 2 (Foundational) — with every gate/branch it needs from day one (channel gate, rule evaluation including the always-on baseline, the unconfigured-tenant fallback, prompt composition, provider resolution, reply insertion). Each User Story phase after that does NOT re-open `agent_responder.rs` to add new behavior; instead it adds the **config surface** (validation rules, endpoints, UI) that feeds a specific part of the already-built pipeline, plus the **tests that prove that part works**. This is why some files appear in multiple story phases — read the "Depends on" note at the top of each phase before starting it out of order.

## Path Conventions (this repo)

- Backend: `backend/crates/modules/ai/src/` (this feature's new code lives here — see research.md R1), `backend/crates/modules/conversations/src/` (small, targeted changes), `backend/crates/modules/authz/src/` (matrix only), `backend/crates/server/` (wiring + tests), `backend/migrations/`.
- Frontend: `frontend/apps/dashboard/src/app/features/tenant/ai-agent/` (rebuilt) and `frontend/apps/dashboard/src/app/features/tenant/conversations/` (banner added).
- Every backend integration test file uses the existing `REQUIRE_DB_TESTS` gate pattern already used by `backend/crates/server/tests/ai.rs` and `escalations.rs` — copy that pattern's test harness setup (tenant/user fixtures, pool acquisition) rather than inventing a new one.

---

## Phase 1: Setup

**Purpose**: Confirm the baseline builds before touching anything, on both sides of the stack. No new dependencies are needed anywhere in this feature (plan.md Technical Context).

- [ ] T001 Baseline check: from `backend/`, run `cargo build --workspace` and `cargo test --workspace` and confirm both succeed on the current `017-ai-agent-config` branch before any edits, so any later failure is attributable to this feature's changes.
- [ ] T002 [P] Baseline check: from `frontend/`, run `pnpm ng build dashboard` and `pnpm ng test dashboard` and confirm both succeed before any edits.

**Checkpoint**: Clean baseline confirmed on both stacks.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema, shared model/validation types, the deterministic prompt composer, the rule-matching engine, the full responder pipeline (all gates and branches from `contracts/agent-runtime.md`), the RBAC matrix narrowing, and the audit helpers. **No user story can be implemented, let alone tested, until this phase is complete** — every story's "independent test" ends with "and the AI actually replies/escalates," which only works once the responder exists end-to-end.

**⚠️ CRITICAL**: Do not skip ahead. `agent_responder.rs` (T017) is the single most important file in this feature — it is written completely here, not incrementally per story.

### Migrations & schema

- [ ] T003 Create `backend/migrations/0041_agent_configurations.sql` with exactly this content (mirrors the 0038 pattern in `backend/migrations/0038_ai_configurations.sql`: `gen_random_uuid()` default PK, `set_updated_at`-style trigger function per table, partial unique indexes):

  ```sql
  -- Migration 0041: AI agent configurations — the tenant's configurable AI
  -- agent. Multi-agent-shaped (named, one designated default) with a single
  -- extra partial-unique index enforcing the v1 "exactly one agent" rule;
  -- dropping that one index is the entire multi-agent unlock (see research.md R2).

  CREATE TABLE agent_configurations (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
      name TEXT NOT NULL CHECK (char_length(trim(name)) BETWEEN 1 AND 80),
      is_default BOOLEAN NOT NULL DEFAULT true,
      avatar_kind TEXT NOT NULL DEFAULT 'preset' CHECK (avatar_kind IN ('preset', 'upload')),
      avatar_preset TEXT NULL,
      tone TEXT NOT NULL DEFAULT 'professional'
          CHECK (tone IN ('professional', 'friendly', 'casual', 'formal', 'empathetic')),
      system_prompt TEXT NOT NULL DEFAULT '' CHECK (char_length(system_prompt) <= 8000),
      business_rules JSONB NOT NULL DEFAULT '[]',
      escalation_rules JSONB NOT NULL DEFAULT '[]',
      enabled_channels JSONB NOT NULL DEFAULT '["web_chat"]',
      provider TEXT NULL CHECK (provider IN ('openai', 'anthropic', 'gemini')),
      model TEXT NULL CHECK (model IS NULL OR char_length(trim(model)) >= 1),
      version INTEGER NOT NULL DEFAULT 1,
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      deleted_at TIMESTAMPTZ NULL,

      CONSTRAINT agent_configurations_provider_model_pair CHECK (
          (provider IS NULL AND model IS NULL) OR (provider IS NOT NULL AND model IS NOT NULL)
      )
  );

  CREATE UNIQUE INDEX agent_configurations_tenant_single_live_uq
      ON agent_configurations (tenant_id)
      WHERE deleted_at IS NULL;

  CREATE UNIQUE INDEX agent_configurations_tenant_default_uq
      ON agent_configurations (tenant_id)
      WHERE is_default AND deleted_at IS NULL;

  CREATE UNIQUE INDEX agent_configurations_tenant_name_uq
      ON agent_configurations (tenant_id, lower(name))
      WHERE deleted_at IS NULL;

  CREATE OR REPLACE FUNCTION set_agent_configurations_updated_at()
  RETURNS TRIGGER AS $$
  BEGIN
      NEW.updated_at = now();
      RETURN NEW;
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER set_agent_configurations_updated_at
      BEFORE UPDATE ON agent_configurations
      FOR EACH ROW
      EXECUTE FUNCTION set_agent_configurations_updated_at();

  CREATE TABLE agent_avatar_uploads (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
      agent_id UUID NOT NULL REFERENCES agent_configurations(id) ON DELETE CASCADE,
      content_type TEXT NOT NULL CHECK (content_type IN ('image/png', 'image/jpeg', 'image/webp')),
      bytes BYTEA NOT NULL CHECK (octet_length(bytes) <= 262144),
      created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      deleted_at TIMESTAMPTZ NULL
  );

  CREATE UNIQUE INDEX agent_avatar_uploads_agent_live_uq
      ON agent_avatar_uploads (agent_id)
      WHERE deleted_at IS NULL;

  CREATE OR REPLACE FUNCTION set_agent_avatar_uploads_updated_at()
  RETURNS TRIGGER AS $$
  BEGIN
      NEW.updated_at = now();
      RETURN NEW;
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER set_agent_avatar_uploads_updated_at
      BEFORE UPDATE ON agent_avatar_uploads
      FOR EACH ROW
      EXECUTE FUNCTION set_agent_avatar_uploads_updated_at();
  ```

  Note `enabled_channels` defaults to `'["web_chat"]'` (not empty) so that the moment a row is created via first save with no explicit channel selection, the agent is reachable on the platform's only v1 channel — this matches the default template in `contracts/rest-api.md`.

- [ ] T004 Create `backend/migrations/0042_message_kinds_ai_system.sql`. Postgres CHECK constraints cannot be altered in place — drop and recreate both, exactly reproducing every existing arm from `backend/migrations/0034_messages.sql` plus the two new kinds:

  ```sql
  -- Migration 0042: extend the messages kind vocabulary with 'ai' (LLM-generated
  -- agent reply) and 'system' (platform-authored automatic message, e.g. the
  -- unconfigured-tenant auto-acknowledgment). Both carry NULL membership ids,
  -- like 'customer'.

  ALTER TABLE messages DROP CONSTRAINT messages_kind_check;
  ALTER TABLE messages ADD CONSTRAINT messages_kind_check CHECK (
      kind IN ('customer', 'reply', 'note', 'ai', 'system')
  );

  ALTER TABLE messages DROP CONSTRAINT messages_kind_consistency;
  ALTER TABLE messages ADD CONSTRAINT messages_kind_consistency CHECK (
      (kind = 'customer' AND sender_membership_id IS NULL)
      OR (kind = 'reply' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
      OR (kind = 'note' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
      OR (kind IN ('ai', 'system') AND sender_membership_id IS NULL AND logged_by_membership_id IS NULL)
  );
  ```

- [ ] T005 Create `backend/migrations/0043_conversation_ai_handling.sql`:

  ```sql
  -- Migration 0043: per-conversation fallback decision while a tenant has no
  -- live agent_configurations row. NULL = undecided; ignored entirely once
  -- the tenant configures its own agent (see contracts/agent-runtime.md step 1).

  ALTER TABLE conversations
      ADD COLUMN ai_handling TEXT NULL CHECK (ai_handling IN ('platform_ai', 'human'));
  ```

- [ ] T006 [P] Extend `backend/crates/shared/db/tests/schema.rs` with new test functions (follow the file's existing behavioral style — attempt an insert/update and assert `Ok`/`Err`, do not use `information_schema` introspection): (a) name empty or 81 chars → `agent_configurations` insert fails; (b) tone outside the 5-value catalog → fails; (c) `system_prompt` over 8000 chars → fails; (d) `provider` set with `model` NULL (or vice versa) → fails the pair CHECK; (e) inserting a second live `agent_configurations` row for the same `tenant_id` → fails on `agent_configurations_tenant_single_live_uq`; (f) inserting a second `is_default = true` live row for the same tenant (different name) → fails on `agent_configurations_tenant_default_uq`; (g) two live rows with the same `lower(name)` for one tenant → fails on `agent_configurations_tenant_name_uq`; (h) `agent_avatar_uploads` with `content_type = 'application/pdf'` → fails; (i) `agent_avatar_uploads.bytes` over 262144 bytes → fails; (j) two live `agent_avatar_uploads` rows for the same `agent_id` → fails; (k) `messages` insert with `kind = 'ai'` and non-NULL `sender_membership_id` → fails; (l) `messages` insert with `kind = 'system'` and NULL membership ids → succeeds; (m) `conversations.ai_handling` accepts `'platform_ai'`/`'human'`/`NULL`, rejects any other string.

### Shared model, catalogs, and validation (`modules/ai`)

- [ ] T007 Add `escalations` and `conversations` as path dependencies in `backend/crates/modules/ai/Cargo.toml` (mirror the existing `tenancy = { path = "../tenancy" }` line style). This is required because `agent_responder.rs` (T017) calls `conversations` query helpers to read/insert messages and `escalations::routing::route_new_escalation_in_tx` plus `escalations::presence::Runtime` to fire escalations, and because `agent_config.rs` implements `conversations`' `AiAgentStatus` port (T048). Also add `async-trait.workspace = true` to `backend/crates/modules/conversations/Cargo.toml` for that port's trait definition. **No cycle**: the arrows are `ai → conversations`, `ai → escalations`, `escalations → conversations`; nothing depends on `ai`. This direction is load-bearing for the whole feature — whenever `conversations` appears to need something from `ai`, the answer is a trait port defined in `conversations` and implemented in `ai` (T048), never a reverse dependency and never a raw cross-module table read.

- [ ] T008 Create `backend/crates/modules/ai/src/agent_config.rs` with the row/DTO types and pure validation functions (no I/O in this file except the query functions listed below). Required contents:
  - `pub const TONES: [&str; 5] = ["professional", "friendly", "casual", "formal", "empathetic"];`
  - `pub const AVATAR_PRESETS: &[&str] = &[...]` — pick 6-8 short preset keys (e.g. `"spark"`, `"orbit"`, `"beacon"`, `"nova"`, `"pulse"`, `"atlas"`); these are referenced by key only, no image data lives in the backend.
  - `pub struct AgentConfigurationRow { id: Uuid, tenant_id: Uuid, name: String, is_default: bool, avatar_kind: String, avatar_preset: Option<String>, tone: String, system_prompt: String, business_rules: serde_json::Value, escalation_rules: serde_json::Value, enabled_channels: serde_json::Value, provider: Option<String>, model: Option<String>, version: i32, created_at: DateTime<Utc>, updated_at: DateTime<Utc> }` deriving `sqlx::FromRow`.
  - `pub struct EscalationRule { id: Uuid, name: String, trigger: EscalationTrigger, keywords: Vec<String>, required_skill_ids: Vec<Uuid> }` with `EscalationTrigger` a `#[serde(rename_all = "snake_case")]` enum `HumanRequest | TopicKeywords`, matching the JSON shape in `data-model.md`.
  - `pub struct AgentConfigPayload { name: String, avatar: AvatarPayload, tone: String, system_prompt: String, business_rules: Vec<String>, escalation_rules: Vec<EscalationRulePayload>, enabled_channels: Vec<String>, provider_selection: Option<ProviderSelectionPayload>, version: Option<i32> }` — the exact `PUT` request body from `contracts/rest-api.md`.
  - `pub fn validate_payload(payload: &AgentConfigPayload) -> Result<(), Vec<ValidationIssue>>` returning **all** field errors at once (not fail-fast) as `{field, code, message}` triples matching the `422` `details` shape used elsewhere in this codebase (see `backend/crates/modules/conversations/src/routes.rs` `add_message`'s `with_details(vec![json!({...})])` pattern): name 1-80 trimmed chars; tone ∈ `TONES`; system_prompt ≤ 8000 chars; business_rules ≤ 20 entries each 1-500 chars; escalation_rules ≤ 20 entries, each `name` 1-80 chars and unique (case-insensitive) within the array, `keywords` non-empty only when `trigger = topic_keywords` (and empty required when `trigger = human_request`), each keyword 1-40 chars; enabled_channels ⊆ `{email, phone, web_chat, whatsapp, telegram}` no duplicates; `provider_selection` provider ∈ `{openai, anthropic, gemini}` when present.
  - `pub const CATALOG_CHANNELS: [&str; 5] = ["email", "phone", "web_chat", "whatsapp", "telegram"];` (mirrors `conversations_channel_check` from `backend/migrations/0026_conversations.sql`).
  - `pub const PROVIDER_CATALOG: [&str; 3] = ["openai", "anthropic", "gemini"];` (must match `ai_configurations` CHECK from 0038 — do not diverge).
  - Escalation reason strings and the human-request phrase catalog do **not** belong in this file — they live in `agent_rules.rs` (T012). Keep `agent_config.rs` free of responder concerns: it is the config model, validation, and queries only.

- [ ] T009 In the same `backend/crates/modules/ai/src/agent_config.rs`, add the query functions (I/O, using `sqlx::Transaction<'_, Postgres>` where the caller is inside a transaction, plain `&PgPool` where it's a read-only GET):
  - `pub async fn load_live(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Option<AgentConfigurationRow>>` — `SELECT * FROM agent_configurations WHERE tenant_id = $1 AND deleted_at IS NULL`.
  - `pub async fn load_live_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid) -> sqlx::Result<Option<AgentConfigurationRow>>` — same query, `FOR UPDATE` locked (needed by the version-checked update path and by the responder to avoid racing a concurrent PUT).
  - `pub async fn create_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, payload: &AgentConfigPayload) -> sqlx::Result<AgentConfigurationRow>` — INSERT with `is_default = true`, `version = 1`; on unique-violation of `agent_configurations_tenant_single_live_uq` the caller (route handler) must translate that into the `409` "configuration changed since it was loaded" response (two concurrent first-saves race).
  - `pub async fn update_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, agent_id: Uuid, expected_version: i32, payload: &AgentConfigPayload) -> sqlx::Result<Option<AgentConfigurationRow>>` — `UPDATE ... SET ..., version = version + 1 WHERE tenant_id = $1 AND id = $2 AND version = $3 AND deleted_at IS NULL RETURNING *`; `Ok(None)` means version mismatch → caller returns `409`.
  - `pub async fn live_skill_ids(pool: &PgPool, tenant_id: Uuid, ids: &[Uuid]) -> sqlx::Result<Vec<Uuid>>` — `SELECT id FROM skills WHERE tenant_id = $1 AND id = ANY($2) AND deleted_at IS NULL` (mirror `escalations::routing::skill_ids_exist_in_tenant_in_tx` query style from `backend/crates/modules/escalations/src/model.rs`); used both to validate on save (reject unknown ids) and to compute `broken_skill_refs` on read (ids present in the stored rule but absent from this result).
  - `pub async fn credential_resolves(pool: &PgPool, tenant_id: Uuid, provider: &str) -> bool` — thin wrapper around `crate::resolution::resolve_credential_view(pool, Scope::Tenant(tenant_id), provider)` (this is the existing metadata-only lookup already used by `routes.rs` — it needs no master key, unlike `resolve_credential` which decrypts): `Some(_) => true`, `None => false`. Used both for the `options` endpoint's `credential_available` and for computing `provider_selection.stale`. Do not use `resolve_credential` here — it requires a `MasterKey` that is private inside `AiService` and unreachable from route handlers or the responder; this function only needs to know a credential *exists*, not decrypt it.

### Deterministic prompt composition

- [ ] T010 Create `backend/crates/modules/ai/src/agent_prompt.rs` implementing exactly the template from `contracts/agent-runtime.md` step 5:
  - `const TONE_DIRECTIVES: [(&str, &str); 5]` mapping each of the 5 tones to one fixed directive sentence (e.g. `("friendly", "Respond in a warm, approachable, conversational tone.")`) — write reasonable directive text for all 5.
  - `pub fn compose_system_message(agent_name: &str, system_prompt: &str, tone: &str, business_rules: &[String]) -> String` building, in fixed order: (1) `system_prompt` verbatim (skip the line entirely if empty); (2) the tone directive; (3) if `business_rules` is non-empty, a header line ("You must always follow these rules:") followed by `1. rule\n2. rule\n...` in stored order; (4) the fixed guardrail line identifying as `agent_name`, an AI assistant, that never claims to be human. Join with `"\n\n"` between present sections.
  - This function MUST be pure (no I/O, no randomness, no current-time) so it is byte-deterministic for identical inputs — this is what the unit test in T011 checks.

- [ ] T011 [P] Add `#[cfg(test)] mod tests` in `agent_prompt.rs`: call `compose_system_message` twice with identical arguments and `assert_eq!` the two outputs (byte-equality); call it with empty `system_prompt` and confirm that section is omitted; call it with empty `business_rules` and confirm the rules header is omitted entirely (not printed with zero items); call it once per tone and assert each produces a distinct, non-panicking string.

### Escalation rule matching (baseline + tenant rules)

- [ ] T012 Create `backend/crates/modules/ai/src/agent_rules.rs`:
  - `pub const HUMAN_REQUEST_PHRASES: &[&str] = &["talk to a human", "speak to a human", "speak to an agent", "talk to an agent", "real person", "human agent", "speak to someone", "customer service representative"];` (case-insensitive substring catalog — research.md R7; feel free to add a couple more common phrasings, keep the list a `const`).
  - The two fixed escalation reason strings, as named constants so the audit trail, the queue UI, and the tests all reference one source (never re-type these literals at call sites — T017 and T046 both import them):
    - `pub const BASELINE_ESCALATION_REASON: &str = "customer requested a human";` — used when the built-in phrase catalog fires with no tenant rule matching (T017 step 5).
    - `pub const UNCONFIGURED_ESCALATION_REASON: &str = "no AI agent configured";` — used by the ai-handling `human` decision (T046); matches the string documented in `contracts/rest-api.md`.
  - `pub fn matches_human_request(message_body: &str) -> bool` — lowercase the body once, then `HUMAN_REQUEST_PHRASES.iter().any(|p| lowered.contains(p))`.
  - `pub enum RuleMatch { None, Baseline, Tenant { rule_id: Uuid, rule_name: String, required_skill_ids: Vec<Uuid> } }`
  - `pub fn evaluate(message_body: &str, rules: &[crate::agent_config::EscalationRule]) -> RuleMatch` implementing the fixed order from `contracts/agent-runtime.md` step 4: (a) baseline `matches_human_request` first — if it matches, return `RuleMatch::Baseline` immediately, before looking at any tenant rule; (b) then walk `rules` in array order — a `HumanRequest`-trigger tenant rule matches by the same `matches_human_request` check, a `TopicKeywords`-trigger tenant rule matches if any of its (lowercased) `keywords` is a substring of the lowercased body; first match wins, return `RuleMatch::Tenant{..}`; (c) no match → `RuleMatch::None`. This function takes only borrowed data and does no I/O — deterministic and unit-testable in isolation from the database.

- [ ] T013 [P] Add `#[cfg(test)] mod tests` in `agent_rules.rs`: (a) message "I want to talk to a human" with zero rules → `RuleMatch::Baseline` (proves FR-011: baseline works even with no tenant rules); (b) message containing a configured keyword → `RuleMatch::Tenant`; (c) message that both mentions a human-request phrase AND matches a later tenant keyword rule → still `RuleMatch::Baseline` (baseline is checked first, unconditionally); (d) two tenant rules where an earlier one and a later one would both match → the earlier one wins (first-match-wins over `rules` in array order); (e) keyword matching is case-insensitive (`"REFUND"` matches a `"refund"` keyword rule).

### The full responder pipeline

- [ ] T014 In `backend/crates/modules/conversations/src/outbox.rs`, add `pub async fn emit_customer_message_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, conversation_id: Uuid, message_id: Uuid, channel: &str) -> sqlx::Result<()>`, copying the exact `INSERT INTO outbox_events (...) VALUES (..., 'conversation.customer_message', ...)` shape used by `emit_status_changed_in_tx` in the same file, with payload `json!({"conversation_id": conversation_id, "message_id": message_id, "channel": channel})` (per `contracts/agent-runtime.md`'s event schema — note these payload keys are snake_case here, unlike the existing camelCase escalation events, to match the contract doc; keep it exactly as written in the contract).

- [ ] T015 Wire `emit_customer_message_in_tx` into `backend/crates/modules/conversations/src/routes.rs`: in `add_message`, call it (inside the existing transaction, after `add_message_in_tx` succeeds) **only when** `payload.kind == MessageKind::Customer`, passing the newly created message's `id` and the conversation's `channel` (available from `queries::conversation_row_in_tx`'s earlier result — thread the channel value through). In `create_conversation`, call it identically for the conversation's initial message when that message is customer-authored (check how `create_conversation` builds its first message in `backend/crates/modules/conversations/src/routes.rs` around line 852 and mirror the same "only if kind = customer" guard).

- [ ] T016 In `backend/crates/modules/conversations/src/model.rs`, extend `MessageKind` with `Ai` and `System` variants (`#[serde(rename = "ai")]` / `#[serde(rename = "system")]`, snake_case matching the DB values). Update the `Participant` construction logic wherever `MessageKind` is matched to build the participant projection (search `queries.rs` and `model.rs` for existing `match kind` / `MessageKind::Customer =>` arms and add): `Ai => Participant { participant_type: "ai_agent".into(), id: None, membership_id: None, display_name: <agent name passed in>, active: None }`; `System => Participant { participant_type: "system".into(), id: None, membership_id: None, display_name: "Automated reply".into(), active: None }`. In `backend/crates/modules/conversations/src/queries.rs`, ensure timeline and last-message-preview queries already select `kind IN (...)` without an explicit whitelist (confirm they select `*`/all kinds and are NOT filtered to `('customer','reply','note')` anywhere — if such a filter exists, widen it to include `'ai'` and `'system'`).

- [ ] T017 Create `backend/crates/modules/ai/src/agent_responder.rs` — the complete pipeline from `contracts/agent-runtime.md`. This is the single largest file in this feature.

  **Cross-module access rule (Constitution I)**: `agent_responder.rs` must never write raw SQL against `conversations`, `messages`, or `escalations` tables directly. Every read/write against those tables goes through a small `pub` helper function added to the *owning* module (`conversations::queries` or `escalations::routing`) in this same task — list them below. This keeps the "modules touch their own tables only" boundary real, not just asserted in the plan.

  **Add these owning-module helper functions** (do this first, they're small):
  - `backend/crates/modules/conversations/src/queries.rs`: `pub async fn conversation_ai_state(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid) -> sqlx::Result<Option<(String, Option<String>)>>` returning `(status, ai_handling)`; `pub async fn has_system_message(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid) -> sqlx::Result<bool>`; `pub async fn insert_auto_ack_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, conversation_id: Uuid, body: &str) -> sqlx::Result<()>` (inserts `kind='system'`, bumps `last_activity_at`); `pub async fn insert_ai_reply_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, conversation_id: Uuid, body: &str) -> sqlx::Result<()>` (inserts `kind='ai'`, bumps `last_activity_at`); `pub async fn has_ai_reply_since(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid, since_message_id: Uuid) -> sqlx::Result<bool>` (idempotency guard — compares `seq`/`created_at` against the triggering message's row); `pub async fn message_body(pool: &PgPool, tenant_id: Uuid, message_id: Uuid) -> sqlx::Result<Option<String>>`; `pub async fn recent_history(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid, limit: i64) -> sqlx::Result<Vec<(String, String)>>` returning `(kind, body)` pairs for `kind IN ('customer','reply','ai')`, chronological, capped at `limit`.
  - `backend/crates/modules/escalations/src/routing.rs` (or a thin new `pub` wrapper next to it if `route_new_escalation_in_tx`'s exact signature doesn't fit): `pub async fn has_open_escalation(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid) -> sqlx::Result<bool>` (`status IN ('queued','assigned')`).

  Implement `pub async fn process_agent_responder_once(pool: &PgPool, ai: &AiService, presence: &Arc<escalations::presence::Runtime>) -> sqlx::Result<bool>` (returns `Ok(true)` if it processed an event, `Ok(false)` if the queue was empty) in three phases so **no DB transaction is ever held across the vendor HTTP call** — the plan's Constitution X / Performance Goals require the vendor round-trip (up to ~10s worst-case with 015's retries/failover) off any lock, and `agent_configurations` must never be `FOR UPDATE`-locked for that long (it would serialize every concurrent AI reply for the tenant):

  **Phase A — claim + read (short, no config lock)**:
  1. Claim one unprocessed `outbox_events` row exactly like `process_escalation_outbox_once` (`UPDATE ... SET claimed_at = now(), claim_token = $1 WHERE id = (SELECT ... FOR UPDATE SKIP LOCKED AND event_type = 'conversation.customer_message') RETURNING ...`); parse `tenant_id`, `conversation_id`, `message_id`, `channel`. If none claimed, return `Ok(false)`.
  2. `agent_config::load_live(pool, tenant_id)` — plain read, **not** `_in_tx`, **no `FOR UPDATE`** (a PUT racing this read is fine: the plan already says config binds at read-time, in-flight completes under the prior config).
     - **`None` (unconfigured)** → `conversations::queries::conversation_ai_state(pool, tenant_id, conversation_id)`:
       - `status` `resolved`/`closed`, or `ai_handling = 'human'` → done: `DELETE FROM outbox_events WHERE id = $event_id`, return `Ok(true)`.
       - `ai_handling IS NULL` → `conversations::queries::has_system_message(...)`; if false, `pool.begin()` → `insert_auto_ack_in_tx` (fixed body constant, e.g. `"Thanks for reaching out! A team member will review your message shortly."`) → commit. Either way, delete the outbox row, return `Ok(true)`. **No AI reply, no escalation.**
       - `ai_handling = 'platform_ai'` → do not return; continue to step 3 using an in-memory **platform default persona** (`AgentConfigurationRow`-shaped value: fixed name e.g. `"Assistant"`, `tone = "professional"`, empty `system_prompt`, empty `business_rules`, empty `escalation_rules`) and treat the channel gate (step 3) as always-open for this branch. If provider resolution (step 4 below) fails, fall through to the same "auto-ack if not already sent" behavior as the `ai_handling IS NULL` case above instead of erroring.
     - **`Some(row)`** → `ai_handling` is never consulted again; continue with the loaded row.
  3. Gate on channel (real-agent branch only): `channel` not in `row.enabled_channels` → done (delete event, return `Ok(true)`).
  4. Gate on state: re-check via `conversation_ai_state` (`resolved`/`closed`) and `escalations::routing::has_open_escalation` — either → done.
  5. `conversations::queries::message_body(pool, tenant_id, message_id)`; parse `row.escalation_rules` (JSONB) into `Vec<EscalationRule>` (empty for the platform-persona branch); `agent_rules::evaluate(&body, &rules)`.
     - `RuleMatch::Baseline` or `RuleMatch::Tenant{..}` → fetch `present_ids` via `presence.present_membership_ids_async(tenant_id).await`; open a short transaction; for `Baseline` call `escalations::routing::route_new_escalation_in_tx(&mut tx, pool, tenant_id, conversation_id, agent_rules::BASELINE_ESCALATION_REASON, &[], &[], &present_ids, Uuid::nil())`; for `Tenant{rule_name, required_skill_ids, ..}` resolve live skills via `agent_config::live_skill_ids` first (drop stale ones), then call the same function with `reason = &rule_name` and those skill ids/names; commit; delete the outbox row; return `Ok(true)`. **No AI reply either way.**
     - `RuleMatch::None` → continue to Phase B.

  **Phase B — vendor call (no transaction, no lock held)**:
  6. `agent_prompt::compose_system_message(&row.name, &row.system_prompt, &row.tone, &business_rules_vec)`. `conversations::queries::recent_history(pool, tenant_id, conversation_id, 20)`, map `customer` → user role, `reply`/`ai` → assistant role (check `ai_providers::Message`/`Role` shape already imported in `modules/ai/src/routes.rs`).
  7. Resolve provider/model: if `row.provider`/`row.model` both `Some` and `agent_config::credential_resolves(pool, tenant_id, provider)` is true, call `ai.complete_with_override(AiCallContext{tenant_id, request_id: None}, AiInput{system: Some(composed), messages}, provider, model)` (T018); otherwise call the existing `ai.complete(...)`. Either call happens with **no open transaction and no row lock**. If the result is `Err(AiCallError::NotConfigured)` (including the override path falling through, and the platform-persona branch with nothing resolvable): delete the outbox row, return `Ok(true)`, **no reply, no error surfaced to the customer** (platform-persona sub-case: also run the "auto-ack if not sent" step from step 2 before returning).

  **Phase C — insert reply (short, fresh transaction)**:
  8. `conversations::queries::has_ai_reply_since(pool, tenant_id, conversation_id, message_id)` — if true (a concurrent run already replied to this same trigger, e.g. after a crash-and-retry), skip insertion (idempotency). Otherwise `pool.begin()` → `conversations::queries::insert_ai_reply_in_tx(&mut tx, tenant_id, conversation_id, &result.content)` → commit.
  9. Delete the outbox row, return `Ok(true)`.

  Also add `pub async fn run_agent_responder_worker(pool: PgPool, ai: AiService, presence: Arc<escalations::presence::Runtime>) -> ! { loop { match process_agent_responder_once(&pool, &ai, &presence).await { Ok(true) => {}, Ok(false) => tokio::time::sleep(Duration::from_secs(1)).await, Err(e) => { tracing::error!(%e, "agent responder consumer error"); tokio::time::sleep(Duration::from_secs(5)).await } } } }` — copy `run_escalation_outbox_worker`'s body shape exactly.

- [ ] T018 In `backend/crates/modules/ai/src/service.rs`, add a new public method on `AiService` (alongside the existing `complete`) that accepts an explicit provider/model override for the agent-level selection (research.md R5/R12 depend on this — it does not exist in the code inherited from feature 015, whose `complete()` always resolves the tenant's `ai_configurations` row internally): `pub async fn complete_with_override(&self, ctx: AiCallContext, input: AiInput, provider: &str, model: &str) -> Result<AiCallResult, AiCallError>`. Model it closely on `complete()`'s own body (same file, right above):
  1. `let scope = Scope::Tenant(ctx.tenant_id);`
  2. Resolve `capture_content` the same way `complete()` does: `let capture_content = resolve_config(&self.0.pool, scope).await.map_err(...)?.map(|r| r.capture_content).unwrap_or(false);` (an override with no `ai_configurations` row at all still works — capture just defaults to off, matching the "off by default" invariant from feature 015).
  3. `let master_key = self.0.master_key.as_ref().ok_or(AiCallError::NotConfigured)?;` then `resolve_credential(&self.0.pool, master_key, scope, provider).await.map_err(AiCallError::Internal)?` — if `None`, return `Err(AiCallError::NotConfigured)` (the caller in `agent_responder.rs` T017 step 7 already treats this as "fall back to plain `complete()`"). If `Some((key, _))`, build exactly one `Attempt { provider: provider.to_string(), model: model.to_string(), key, max_output_tokens: None, temperature: None }` — **no fallback chain**: the agent-level override is a single provider/model pair by design (data-model.md: one nullable column pair, not an array), unlike `complete()`'s tenant-config-driven fallback list.
  4. Call `run_attempts_traced(&self.0.registry, &attempts, ctx.request_id.as_deref(), input.system, input.messages).await` — the exact same private helper `complete()` calls, imported already in this file.
  5. Time it and write the usage record with the exact same `usage::UsageWrite { .. }` construction and `usage::insert(&self.0.pool, w).await` call that `complete()` uses after `run_attempts_traced` returns (copy that block verbatim, adjusting only which local variables feed it — `tenant_id`, `request_id`, `capture_content`, and the `Ok`/`Err` match arms are unchanged). Return the same `Result<AiCallResult, AiCallError>` `complete()` returns.

- [ ] T019 [P] In `backend/crates/server/src/main.rs`, wire the responder worker exactly like the escalation worker is wired (see the block around `let escalation_worker = tokio::spawn(escalations::events::run_escalation_outbox_worker(...))`): add `let agent_responder_worker = tokio::spawn(ai::agent_responder::run_agent_responder_worker(state.db.clone(), state.ai.clone(), state.escalations.clone()));` and add a matching `result = agent_responder_worker => { panic!("agent responder worker stopped unexpectedly: {result:?}"); }` arm to the `tokio::select!` block. Confirm `escalations::presence::Runtime` is `Clone` (it already is — `state.escalations.clone()` is used for the existing worker) and that `ai::agent_responder` is declared `pub mod agent_responder;` in `backend/crates/modules/ai/src/lib.rs`.

### RBAC narrowing and audit helpers

- [ ] T020 In `backend/crates/modules/authz/src/matrix.rs`, apply research.md R11 exactly: remove `Permission::AiAgentView` and `Permission::AiAgentManage` from the `TENANT_MANAGER` array; remove `Permission::AiAgentView` from the `TENANT_VIEWER` array; remove `Permission::AiAgentView` from `STAFF_PRODUCTION_DEVELOPER`. Leave `TENANT_ADMIN` (Owner uses `Permission::TENANT` which is unaffected, Admin uses `TENANT_ADMIN`) untouched — it already has both. Leave `TENANT_AGENT`, `PLATFORM_*`, and `STAFF_PRODUCTION_SUPPORT`/`SALES`/`FINANCE` untouched (none currently grant `AiAgent*`).

- [ ] T021 [P] Update `backend/crates/server/tests/rbac.rs`: find the existing matrix-expectation tests/tables that assert which roles have `ai_agent.view`/`ai_agent.manage` (they exist today for Manager/Viewer since T020 changes their permissions) and flip those specific expectations to reflect the narrowing; add explicit assertions that `TENANT_MANAGER` and `TENANT_VIEWER` no longer contain either `AiAgentView`/`AiAgentManage` (Viewer) or both (Manager), while `TENANT_ADMIN` still contains both.

- [ ] T022 [P] Create `backend/crates/modules/ai/src/agent_audit.rs` (separate from the existing `audit.rs` which covers 015's `ai_config.*`/`ai_credential.*` actions — keep this feature's actions in their own file for clarity, both re-exported from `lib.rs`) with helper functions calling `tenancy::audit::record_in_tx` (signature: `record_in_tx(tx, action: &str, actor_user_id: Option<Uuid>, tenant_id: Option<Uuid>, resource_type: &str, resource_id: Option<&str>, details: &serde_json::Value)`):
  - `pub async fn record_agent_config_created(tx, actor_user_id, tenant_id, agent_id, payload) -> sqlx::Result<()>` → action `"agent_config.created"`, resource_type `"agent_configuration"`, `details` = a non-sensitive snapshot (name, tone, channel list, provider/model, rule counts — not full rule text is fine to include, nothing secret exists here anyway).
  - `pub async fn record_agent_config_updated(tx, actor_user_id, tenant_id, agent_id, changed_fields: &[&str]) -> sqlx::Result<()>` → action `"agent_config.updated"`, `details = json!({"changed_fields": changed_fields})`.
  - `pub async fn record_agent_config_avatar_updated(tx, actor_user_id, tenant_id, agent_id, kind: &str, detail: &str) -> sqlx::Result<()>` → action `"agent_config.avatar_updated"`.
  - Note `conversation.ai_handling_set` audit (used by US6/T0xx below) belongs in `conversations`' own audit helper file, not here — conversations must not depend on `modules/ai` for this.

**Checkpoint**: Foundation complete. Migrations applied, schema tested, prompt composer and rule matcher unit-tested in isolation, the full responder pipeline exists and compiles (it will not yet be exercised by any endpoint — that starts in US1), RBAC matrix narrowed, audit helpers ready. `cargo build --workspace` must succeed before proceeding to any user story.

---

## Phase 3: User Story 1 - A Tenant Admin Defines the Agent's Identity and Behavior (Priority: P1) 🎯 MVP

**Goal**: A tenant admin can open the settings page, see editable defaults, save a name/avatar/tone/system-prompt, and see that saved identity actually drive AI replies.

**Depends on**: Phase 2 complete. This phase creates the first two HTTP endpoints (`GET`/`PUT /tenant/ai/agent`) and the avatar endpoints — every later story phase (US2-US6) extends the same `agent_routes.rs` file and the same `AgentConfigPayload`/response DTO rather than adding new endpoints, so this phase's tasks are NOT safely parallel with later phases' route tasks (they touch the same file).

**Independent Test**: Sign in as Owner/Admin, `GET /tenant/ai/agent` shows `configured: false` with editable defaults (never a 404 or empty state), fill and `PUT` a name/avatar/tone/prompt, reload and see it persisted, post a customer web-chat message and confirm the `ai`-kind reply in the timeline reflects the saved name/tone/prompt; a second tenant's `GET` still shows `configured: false` (isolation).

### Tests for User Story 1

- [ ] T023 [P] [US1] Unit tests for `agent_config::validate_payload` in `backend/crates/modules/ai/src/agent_config.rs` `#[cfg(test)]`: empty name → error; 81-char name → error; name at exactly 80 chars → passes; tone not in `TONES` → error; system_prompt at 8001 chars → error; a valid minimal payload (name + default tone + empty prompt + no rules + `["web_chat"]` channels + no provider_selection) → passes with zero issues.
- [ ] T024 [US1] Integration test `backend/crates/server/tests/ai_agent.rs` (new file — copy the DB-gated harness setup from `backend/crates/server/tests/ai.rs`'s top of file: tenant/user fixture helpers, `REQUIRE_DB_TESTS` guard): `get_returns_editable_defaults_when_unconfigured` — `GET /tenant/ai/agent` on a fresh tenant returns 200, `configured: false`, and a non-null default template (name, tone `professional`, `enabled_channels: ["web_chat"]`).
- [ ] T025 [US1] In `ai_agent.rs`: `first_save_creates_and_activates_agent` — `PUT` with a valid payload and no `version` → `201`, `version: 1`; a **second** `PUT` with no `version` after that (row now exists) → `409`.
- [ ] T025b [US1] In `ai_agent.rs`: `stale_version_conflicts_without_overwriting` — the core FR-017 case (two admins editing concurrently, which T025 does **not** cover — it only tests the omitted-version path): `PUT` a valid payload (→ `version: 1`), `PUT` again with `version: 1` and a distinct name (→ `200`, `version: 2`), then `PUT` a third payload carrying the now-stale `version: 1` → `409`; follow with a `GET` and assert the **second** save's content survived intact (no silent merge, no partial overwrite from the rejected third save).
- [ ] T026 [US1] In `ai_agent.rs`: `save_round_trips_on_reload` — `PUT` a full payload, then `GET` and assert every field matches what was saved (name/avatar/tone/system_prompt/enabled_channels).
- [ ] T027 [US1] In `ai_agent.rs`: `save_rejects_invalid_payload_atomically` — `PUT` with empty name and a valid other field → `422` with `details` naming the `name` field; follow with a `GET` and confirm nothing was persisted (still `configured: false` or unchanged from before the bad attempt).
- [ ] T028 [US1] In `ai_agent.rs`: `cross_tenant_isolation` — tenant A saves an agent; tenant B's `GET` still shows `configured: false`; tenant B cannot reach tenant A's `id` through any parameter (there is no by-id route, but assert the response never leaks tenant A's name/prompt).
- [ ] T029 [US1] In `ai_agent.rs`: `configured_agent_drives_ai_reply` (wiremock-backed, following the wiremock pattern in `backend/crates/server/tests/ai.rs`) — `PUT` a config with a distinctive name/tone/system_prompt and a resolvable provider (via a tenant `ai_configurations`/`ai_credentials` row from feature 015's fixtures) and `enabled_channels: ["web_chat"]`; post a `web_chat` customer message; run `ai::agent_responder::process_agent_responder_once` directly (not through the worker loop — deterministic single-shot call per research.md R6); assert an `ai`-kind message was inserted and that the wiremock-captured outbound request's system message contains the configured name, tone directive, and prompt text (byte-level substring checks tying back to T010's composer).
- [ ] T030 [US1] In `ai_agent.rs`: `avatar_preset_select_and_upload` — `PUT` with `avatar: {kind: "preset", preset: "spark"}` succeeds; `PUT /tenant/ai/agent/avatar` with a small valid PNG succeeds, bumps `version`, and `GET /tenant/ai/agent/avatar` serves it back with the right `Content-Type`; a follow-up `PUT /tenant/ai/agent` with `avatar: {kind: "preset", preset: "orbit"}` succeeds and a subsequent `GET /tenant/ai/agent/avatar` now `404`s (upload soft-deleted); an oversized (>256KB) or wrong-content-type upload → `422`/`413` and the previously-saved avatar is unchanged.

### Implementation for User Story 1

- [ ] T031 [US1] Create `backend/crates/modules/ai/src/agent_routes.rs` with `GET /tenant/ai/agent` (`get_agent_config`): loads via `agent_config::load_live`; if `None`, build and return the default template (`configured: false`, generic name e.g. `"AI Assistant"`, default preset, `professional`, empty prompt, empty rule arrays, `["web_chat"]`, no provider override, `version: null`); if `Some(row)`, build the full response shape from `contracts/rest-api.md`'s `GET` example, including derived `broken_skill_refs` per escalation rule (via `agent_config::live_skill_ids`) and derived `provider_selection.stale` (via `agent_config::credential_resolves`, only when `row.provider` is `Some`). Follow the utoipa `#[utoipa::path(...)]` annotation style used throughout `backend/crates/modules/ai/src/routes.rs`.
- [ ] T032 [US1] In `agent_routes.rs`, `PUT /tenant/ai/agent` (`put_agent_config`): parse `ApiJson<AgentConfigPayload>`; run `agent_config::validate_payload`, return `422` with all issues on failure; begin a transaction; `load_live_in_tx`; if `None` and `payload.version` is `Some(_)`, return `409`; if `None`, `create_in_tx` (catch the unique-violation race → `409`) then `agent_audit::record_agent_config_created`, commit, return `201`; if `Some(existing)`, require `payload.version == Some(existing.version)` else `409`, then validate `escalation_rules[].required_skill_ids` against `agent_config::live_skill_ids` (any unknown id → `422` naming the offending rule), validate `provider_selection` (if `Some`, provider must be in `agent_config::PROVIDER_CATALOG` and `agent_config::credential_resolves` must be true, else `422`), `update_in_tx`, `agent_audit::record_agent_config_updated` with the diffed field names, commit, return `200` with the fresh row (same shape as `GET`).
- [ ] T033 [US1] In `agent_routes.rs`, `PUT /tenant/ai/agent/avatar` (`put_agent_avatar`): requires a live agent (`404` otherwise); read raw bytes + `Content-Type` header, validate against `agent_config::AVATAR_PRESETS`'s sibling constants for allowed content types and the 256KB cap (`422`/`413` on failure, changing nothing); in one transaction, upsert `agent_avatar_uploads` by soft-deleting any prior live row for this `agent_id` (`UPDATE agent_avatar_uploads SET deleted_at = now() WHERE agent_id = $1 AND deleted_at IS NULL`) and then `INSERT`ing the new row — do this unconditionally, not as a conditional fallback; it always works against the partial unique index and avoids `ON CONFLICT`'s partial-index-targeting restrictions entirely. Set `agent_configurations.avatar_kind = 'upload'`, bump `version`, `agent_audit::record_agent_config_avatar_updated`, commit; return the new `avatar` object + `version`.
- [ ] T034 [US1] In `agent_routes.rs`, `GET /tenant/ai/agent/avatar` (`get_agent_avatar`): look up the live `agent_avatar_uploads` row for the tenant's agent, `404` if none, else return the bytes with the stored `content_type` and `Cache-Control: private, max-age=300`.
- [ ] T035 [US1] Mount the four routes in `backend/crates/server/src/router.rs` under `mount_tenant`, following the exact `.routes(routes!(...).map(|_| {...}).route_layer(require_permission(...)))` pattern used for `ai::routes::get_tenant_config`/`put_tenant_config` a few lines above: `GET /tenant/ai/agent` and `GET /tenant/ai/agent/avatar` behind `Permission::AiAgentView`; `PUT /tenant/ai/agent` and `PUT /tenant/ai/agent/avatar` behind `Permission::AiAgentManage`.
- [ ] T036 [US1] Add `pub mod agent_config; pub mod agent_prompt; pub mod agent_rules; pub mod agent_responder; pub mod agent_routes; pub mod agent_audit;` to `backend/crates/modules/ai/src/lib.rs` (module doc comment: extend the existing Purpose/Responsibilities list per the file's doc-comment style to mention agent configuration and the responder).
- [ ] T037 [US1] Extend `backend/crates/server/tests/openapi_contract.rs` with the four new paths/operation ids and confirm the OpenAPI document builds without panicking (run the existing test that asserts the doc generates).

**Checkpoint**: US1 fully functional — an admin can configure identity/behavior and see it reflected in real AI replies. This is the MVP; stop here and validate against `quickstart.md` steps 1, 3, 4, 6, 15 (partially — full avatar coverage) before continuing.

---

## Phase 4: User Story 6 - Conversations Arriving Before the Agent Is Configured (Priority: P2)

**Goal**: Customers get an automatic acknowledgment while a tenant is unconfigured, and staff can choose platform AI or human handling per conversation.

**Depends on**: Phase 2 (the unconfigured branch of `agent_responder.rs` already exists — T017 step 2). This phase is placed before US2-US5 deliberately: it is P2 like US2/US3, is completely independent of the settings page, and its behavior is directly observable the moment Phase 2 is done (a tenant with zero agent configuration already exists in every other story's "before you configure" starting state) — validating it early derisks the shared pipeline before layering more settings on top.

**Independent Test**: With no agent configured, post a customer message → exactly one `system` auto-ack; choose `platform_ai` on that conversation → next customer message gets an `ai` reply from the platform persona; on a different conversation choose `human` → it lands in the escalation queue.

### Tests for User Story 6

- [ ] T038 [P] [US6] Integration test file `backend/crates/server/tests/ai_agent.rs` (same file as US1, new test functions): `unconfigured_tenant_sends_single_auto_ack` — no agent configured, post a customer message, run `process_agent_responder_once`, assert one `system`-kind message exists and `awaiting_ai_decision` (via the conversation detail response) is `true`; post a second customer message and run the responder again, assert still exactly one `system` message (idempotent, no duplicate acks).
- [ ] T039 [US6] `ai_handling_platform_ai_requires_resolvable_layer` — `POST /tenant/conversations/{id}/ai-handling {"mode":"platform_ai"}` on a tenant with no resolvable `ai_configurations`/credential → `422`; with a resolvable one (015 fixtures) → `200`, `ai_handling: "platform_ai"`.
- [ ] T040 [US6] `ai_handling_platform_ai_then_ai_reply` — after setting `platform_ai`, post a customer message, run the responder, assert an `ai`-kind reply was inserted and the audit log has `conversation.ai_handling_set`.
- [ ] T041 [US6] `ai_handling_human_escalates_immediately` — `POST .../ai-handling {"mode":"human"}` → `200`, response includes an escalation reference; assert the escalation appears in `escalations` with reason `"no AI agent configured"`; run the responder on a subsequent customer message for that conversation and assert no new message/escalation is created (already handled).
- [ ] T042 [US6] `ai_handling_rejects_once_agent_configured` — configure the tenant's real agent (US1's `PUT`), then `POST .../ai-handling` on any conversation → `409` (FR-004c: configured agent supersedes, the decision is no longer meaningful).
- [ ] T043 [US6] `ai_handling_human_to_platform_ai_blocked_once_escalated` — set `human` (creates an escalation), then attempt to set `platform_ai` on the same conversation → `409`.
- [ ] T043b [US6] `unresolvable_platform_ai_returns_to_awaiting_decision` (spec.md edge case: "if it becomes unresolvable later, affected conversations fall back to awaiting a decision and human escalation remains available") — set `platform_ai` on a conversation while the AI layer resolves; then remove the tenant's AI-layer credential/config (015 fixture teardown); `GET` the conversation detail and assert `awaiting_ai_decision` is `true` again even though `ai_handling` is still `"platform_ai"` (T048's derivation); assert `POST .../ai-handling {"mode":"human"}` from that state still succeeds and escalates (the `IS DISTINCT FROM 'human'` guard in T044 permits it); assert the responder, run against a new customer message in that conversation, adds no second auto-ack and no AI reply.

### Implementation for User Story 6

**Ownership note**: `conversations` cannot depend on `ai` (T007 made `ai` depend on `conversations`; the reverse would cycle) and cannot depend on `escalations` either (`escalations` already depends on `conversations`; the reverse would cycle). The `ai-handling` decision touches all three concerns — conversation state, agent-configured check, and escalation creation — so its handler lives in `modules/ai`, the only crate that already reaches both `conversations` and `escalations` (T007). The route path stays exactly `POST /tenant/conversations/{id}/ai-handling` and the guard stays `conversations.manage` — only the Rust module hosting the handler is `ai`. `contracts/rest-api.md` and `data-model.md` already state this; no doc edit is needed here.

- [ ] T044 [US6] Add `pub async fn set_ai_handling_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, conversation_id: Uuid, mode: &str) -> sqlx::Result<bool>` to `backend/crates/modules/conversations/src/queries.rs` (this table write stays owned by `conversations`, called from `ai` via the cross-module dependency T007 already established): `UPDATE conversations SET ai_handling = $1 WHERE tenant_id = $2 AND id = $3 AND ai_handling IS DISTINCT FROM 'human' RETURNING id` (the `IS DISTINCT FROM 'human'` guard implements T043's "human → platform_ai blocked"); returns whether a row was updated. Also add `pub async fn agent_exists(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<bool>` to `backend/crates/modules/ai/src/agent_config.rs` (not `conversations` — this checks `agent_configurations`, which `ai` already owns): `SELECT EXISTS(SELECT 1 FROM agent_configurations WHERE tenant_id = $1 AND deleted_at IS NULL)`.
- [ ] T045 [US6] Add `pub async fn record_ai_handling_set(tx: &mut Transaction<'_, Postgres>, actor_user_id: Option<Uuid>, tenant_id: Uuid, conversation_id: Uuid, mode: &str) -> sqlx::Result<()>` to `backend/crates/modules/ai/src/agent_audit.rs` (alongside T022's other audit helpers — same file, same `tenancy::audit::record_in_tx` shape) — action `"conversation.ai_handling_set"`, resource_type `"conversation"`, resource_id = `conversation_id`.
- [ ] T046 [US6] Create `pub async fn set_conversation_ai_handling(...)` in `backend/crates/modules/ai/src/agent_routes.rs` (`POST /tenant/conversations/{id}/ai-handling`): parse `{mode: "platform_ai" | "human"}`; `conversations::queries::conversation_ai_state(pool, tenant_id, conversation_id)` (the same helper T017 Phase A uses) → `404` if the conversation doesn't exist, `409` if `status` is `resolved`/`closed`; `agent_config::agent_exists(pool, tenant_id)` → `409` if `true` (FR-004c: a configured agent supersedes the decision entirely); for `mode = "platform_ai"`, `agent_config::credential_resolves` must be true for the tenant's resolvable AI-layer provider (check via `crate::resolution::resolve_config` — if it returns `None`, or its provider's credential doesn't resolve, → `422` with a reason) — do NOT check credentials via `conversations`; this whole handler lives in `ai`, so no crate-boundary problem exists here; begin a transaction, `conversations::queries::set_ai_handling_in_tx(&mut tx, tenant_id, conversation_id, "platform_ai")` (`false` → `409`, someone already set `human`), `agent_audit::record_ai_handling_set(...)`, commit, return the conversation detail; for `mode = "human"`, begin a transaction, `set_ai_handling_in_tx(..., "human")`, then `escalations::routing::route_new_escalation_in_tx(&mut tx, pool, tenant_id, conversation_id, agent_rules::UNCONFIGURED_ESCALATION_REASON, &[], &[], &present_ids, principal.user_id)` (fetch `present_ids` via the injected `Arc<escalations::presence::Runtime>` before opening the transaction, same as T017), `agent_audit::record_ai_handling_set(...)`, commit, return the conversation detail with the escalation reference.
- [ ] T047 [US6] Mount `POST /tenant/conversations/{id}/ai-handling` in `router.rs` under `mount_tenant` behind `Permission::ConversationsManage` (not an `AiAgent*` permission — per research.md R13, this is conversation handling), pointing at `ai::agent_routes::set_conversation_ai_handling`.
- [ ] T048 [US6] Surface the decision state on the conversation DTOs **without `conversations` reading any `ai`-owned table** (Constitution I: modules communicate through interfaces, not direct data access — an inline raw `SELECT ... FROM agent_configurations` inside `conversations::queries` violates this just as much as a Cargo dependency would, and `conversations` cannot depend on `ai` anyway because `ai` already depends on it). Use a trait port owned by the consumer:
  1. In `backend/crates/modules/conversations/src/lib.rs` (or a new `src/ports.rs`), declare the interface `conversations` needs but does not implement:

     ```rust
     #[async_trait::async_trait]
     pub trait AiAgentStatus: Send + Sync {
         /// True when the tenant has a live agent configuration of its own.
         async fn agent_configured(&self, tenant_id: Uuid) -> bool;
         /// True when the platform AI layer resolves for the tenant (config + credential).
         async fn platform_ai_available(&self, tenant_id: Uuid) -> bool;
     }
     ```

     Add `async-trait` to `backend/crates/modules/conversations/Cargo.toml` (`async-trait.workspace = true` — already a workspace dep, see `modules/ai/Cargo.toml`).
  2. In `backend/crates/modules/ai/src/agent_config.rs`, implement it for a small adapter (`pub struct AiAgentStatusAdapter { pub pool: PgPool }`), backed by `agent_config::agent_exists` (T044) and `credential_resolves` (T009) over the provider from `crate::resolution::resolve_config`. Dependency direction stays `ai → conversations`, no cycle.
  3. In `backend/crates/server/src/main.rs`/`router.rs`, wire `Extension(Arc::new(ai::agent_config::AiAgentStatusAdapter { pool: state.db.clone() }) as Arc<dyn conversations::AiAgentStatus>)` onto the tenant router, same `Extension` pattern the escalations `presence::Runtime` already uses.
  4. In `backend/crates/modules/conversations/src/queries.rs`, expose only conversations-owned facts: `ai_handling` (its own column, already added by T005) and `has_system_message` (T017's helper — reuse it, don't duplicate).
  5. In `backend/crates/modules/conversations/src/model.rs`, add `ai_handling: Option<String>` and `awaiting_ai_decision: bool` to the conversation detail/inbox DTOs; compute the flag in the detail/list handlers from the injected port plus the two owned facts, using this derivation (**not** the narrower `ai_handling IS NULL` one — spec.md's edge case requires a conversation whose `platform_ai` choice became unresolvable to return to awaiting-decision so the banner reappears and human escalation stays reachable):

     ```text
     awaiting_ai_decision =
         !status.agent_configured(tenant)
         && has_system_ack
         && ( ai_handling IS NULL
              || (ai_handling == "platform_ai" && !status.platform_ai_available(tenant)) )
     ```

     Call the port once per request (not per row) and reuse the two booleans across the list projection to avoid an N+1.
- [ ] T049 [US6] Extend `openapi_contract.rs` for the new route and DTO fields.

**Checkpoint**: US1 + US6 together mean every tenant, configured or not, produces sensible AI/human behavior from message one.

---

## Phase 5: User Story 2 - A Tenant Admin Selects the AI Provider and Model for the Agent (Priority: P2)

**Goal**: The settings page offers a credential-gated provider/model picker; the choice actually routes AI replies.

**Depends on**: Phase 3 (US1) — extends `agent_routes.rs` and the shared `PUT` payload built there; not parallel with US1's route tasks. The provider-override branch of `agent_responder.rs` (T017 step 7, T018) was already built in Phase 2 — this story is mostly the `options` endpoint plus already-covered validation, so it is comparatively small.

**Independent Test**: `GET /tenant/ai/agent/options` shows only credential-backed providers; select and save one; trigger a reply and confirm (via the usage record) it was served by that provider/model; remove the credential and confirm the settings page flags the selection `stale` and traffic falls back.

### Tests for User Story 2

- [ ] T050 [P] [US2] Integration test in `ai_agent.rs`: `options_lists_only_credential_backed_providers` — with credentials configured for only `anthropic` (015 fixtures), `GET /tenant/ai/agent/options` returns `anthropic.credential_available: true` and the others `false`, and `tones`/`channels`/`avatar_presets`/`prompt_max_length`/`limits` all present and matching the constants from T008/T010.
- [ ] T051 [US2] `provider_override_serves_ai_reply` — `PUT` the agent with `provider_selection: {provider: "anthropic", model: "<some curated model>"}` (credential resolvable), post a customer message, run the responder, assert (via wiremock request capture or the usage record inserted by `complete_with_override`) that the call went to Anthropic with that model — not whatever the tenant's plain `ai_configurations` row says.
- [ ] T052 [US2] `stale_override_falls_back` — save an override for a provider, then delete that tenant's credential (015's `DELETE /tenant/ai/credentials/{provider}` or direct fixture teardown), `GET /tenant/ai/agent` and assert `provider_selection.stale: true`; post a message and run the responder, assert the reply (if any) came through the plain tenant/platform default resolution path, not the stale override (i.e. `complete()` was used, not `complete_with_override`).
- [ ] T053 [US2] `save_rejects_unresolvable_provider` — `PUT` with a `provider_selection` for a provider with no credential at all → `422`.

### Implementation for User Story 2

- [ ] T054 [US2] In `agent_routes.rs`, add `GET /tenant/ai/agent/options` (`get_agent_options`) returning the shape from `contracts/rest-api.md`: `tones` (T008 `TONES`), `channels` (T008 `CATALOG_CHANNELS`), `avatar_presets` (T008 `AVATAR_PRESETS`), `providers` (iterate `PROVIDER_CATALOG`, `credential_available` via `agent_config::credential_resolves`, `models` from a new `pub const CURATED_MODELS: &[(&str, &[&str])]` you add to `agent_config.rs` — a handful of current chat model identifiers per provider, e.g. `("openai", &["gpt-4.1", "gpt-4.1-mini"])`, `("anthropic", &["claude-sonnet-5", "claude-haiku-4-5"])`, `("gemini", &["gemini-2.5-pro", "gemini-2.5-flash"])`), `ai_layer_default` (via `crate::resolution::resolve_config` for the tenant, `null`/`null` if unresolvable), `prompt_max_length: 8000`, `limits: {business_rules_max: 20, escalation_rules_max: 20}`.
- [ ] T055 [US2] Mount `GET /tenant/ai/agent/options` in `router.rs` behind `Permission::AiAgentView`, same file/pattern as T035.
- [ ] T056 [US2] Confirm (should already be true from T032/T009) that `put_agent_config` validates `provider_selection` against `PROVIDER_CATALOG` + `credential_resolves` and that `get_agent_config` computes `provider_selection.stale`; if either was stubbed in Phase 3, complete it now.
- [ ] T057 [US2] Extend `openapi_contract.rs` for the new `options` path.

**Checkpoint**: US1 + US2 — identity, behavior, and provider/model choice are all live.

---

## Phase 6: User Story 3 - A Tenant Admin Sets Business Rules and Escalation Rules (Priority: P2)

**Goal**: Business rules shape every reply; escalation rules (human-request + keyword) route to humans with the firing rule as reason; broken skill refs are surfaced.

**Depends on**: Phase 3 (US1) — extends the same `PUT` payload/validation in `agent_routes.rs`/`agent_config.rs`. The rule *matching engine* (T012) and its wiring into the responder (T017 step 5) were already built in Phase 2; this story's tests exercise that existing machinery through real tenant-authored rules for the first time.

**Independent Test**: Add a business rule and a keyword escalation rule tied to a skill; a matching customer message escalates with that rule's name as reason; a non-matching one still gets an AI reply reflecting the business rule; with zero rules configured, an explicit human-request message still escalates (baseline).

### Tests for User Story 3

- [ ] T058 [P] [US3] Integration test in `ai_agent.rs`: `business_rule_appears_in_composed_prompt` — save a distinctive business rule, run the responder (wiremock), assert the captured outbound request's system message contains the rule text in the numbered rules block (ties to T010).
- [ ] T059 [US3] `keyword_rule_escalates_with_rule_name_reason` — save an escalation rule `{name: "Refund requests", trigger: "topic_keywords", keywords: ["refund"], required_skill_ids: [<a real skill>]}`; post a customer message containing "refund"; run the responder; assert no `ai` reply was inserted, an escalation exists with `reason = "Refund requests"` and the skill routed.
- [ ] T060 [US3] `baseline_escalation_survives_zero_tenant_rules` — agent configured with empty `escalation_rules`; message "I want to talk to a human" → escalates anyway (reason = the fixed baseline reason string from T012/T017).
- [ ] T061 [US3] `broken_skill_ref_surfaced_on_read` — save a rule referencing a skill, then soft-delete that skill directly (or via its existing delete endpoint), `GET /tenant/ai/agent` and assert that rule's `broken_skill_refs` contains the deleted skill's id.
- [ ] T062 [US3] `save_rejects_unknown_skill_reference` — `PUT` an escalation rule with a `required_skill_ids` entry that doesn't exist in the tenant → `422` naming the rule.
- [ ] T063 [US3] `save_rejects_malformed_rules` — a `topic_keywords` rule with empty `keywords`, and a `human_request` rule with non-empty `keywords` → both `422` (T008's validation).

### Implementation for User Story 3

- [ ] T064 [US3] Confirm (from T008/T009/T032) that `validate_payload` fully covers business/escalation rule shapes and that `put_agent_config` validates `required_skill_ids` via `live_skill_ids`; this story is mostly test-driven confirmation of Phase 2/3 work — if any gap surfaces from T058-T063, fix it in `agent_config.rs`/`agent_routes.rs` here.
- [ ] T065 [US3] Confirm `get_agent_config`'s `broken_skill_refs` derivation (T031) is exercised correctly by T061; fix if needed.

**Checkpoint**: US1 + US2 + US3 — the agent is now "safe to put in front of customers" per the spec's own framing.

---

## Phase 7: User Story 4 - A Tenant Admin Controls Which Channels the Agent Serves (Priority: P3)

**Goal**: Per-channel enable/disable actually gates AI participation; an all-disabled agent is clearly flagged inactive.

**Depends on**: Phase 3 (US1). The channel gate itself (`agent_responder.rs` T017 step 3) was already built in Phase 2 using whatever `enabled_channels` the row holds; this story is about letting admins change that array and observe the effect, plus the "all disabled → inactive" UI state.

**Independent Test**: Disable `web_chat`, save, post a customer message on that channel, confirm no `ai` reply and the conversation is left for humans; re-enable, confirm replies resume; disable everything and confirm the save still succeeds with a visible "agent inactive" state.

### Tests for User Story 4

- [ ] T066 [P] [US4] Integration test in `ai_agent.rs`: `disabled_channel_blocks_ai_reply` — save with `enabled_channels: []`, post a `web_chat` customer message, run the responder, assert no `ai`/`system` message was inserted and no escalation was created (the conversation is simply left alone — still not the unconfigured branch, since a live row exists).
- [ ] T067 [US4] `re_enabled_channel_resumes_replies` — flip `enabled_channels` back to `["web_chat"]`, post another message, confirm an `ai` reply appears.
- [ ] T068 [US4] `all_channels_disabled_save_succeeds` — `PUT` with `enabled_channels: []` → `200`/`201` (not rejected); `GET` reflects the empty array.

### Implementation for User Story 4

- [ ] T069 [US4] Confirm (from T009/T032) the `enabled_channels` validation (subset of `CATALOG_CHANNELS`, no dupes, empty allowed) is already correct; this story is primarily test-driven confirmation of the Phase 2 gate plus the Phase 3 validation — fix any gap surfaced by T066-T068 here.

**Checkpoint**: All backend behavior for US1-US4 + US6 is done and tested.

---

## Phase 8: User Story 5 - Configuration Changes Are Audited and Access-Controlled (Priority: P3)

**Goal**: Prove every write is audited with correct actor attribution (including platform users in tenant context) and that RBAC narrowing actually blocks Manager/Agent/Viewer.

**Depends on**: Phases 3-7 (exercises the routes/audit calls they built). This phase adds no new production code beyond what T020-T022 and each story's audit calls already wrote — it is the dedicated verification pass the spec calls out as its own story.

### Tests for User Story 5

- [ ] T070 [P] [US5] Integration test in `ai_agent.rs`: `every_write_is_audited` — perform a create, an update, and an avatar change; query `audit_logs` and assert one row per action with the correct `action` string (`agent_config.created`/`agent_config.updated`/`agent_config.avatar_updated`), correct `actor_user_id`, and `updated` rows list the changed field names.
- [ ] T071 [US5] `unauthorized_roles_get_403` — as a Manager, Agent, and Viewer membership in turn, attempt `GET`/`PUT /tenant/ai/agent` → `403` for all three (Manager and Viewer previously had `view`; confirm they no longer do post-T020); as Owner/Admin → succeeds.
- [ ] T072 [US5] `platform_actor_attribution_via_tenant_switch` — using the platform-user-in-tenant-context path (mirror however `backend/crates/server/tests/ai.rs` or `platform_tenants.rs` already tests platform-actor audit attribution for 015's routes), modify the agent config and assert the audit row's actor identifies the platform user, not a tenant membership.

### Implementation for User Story 5

- [ ] T073 [US5] Fix any gap surfaced by T070-T072 in `agent_audit.rs`/`agent_routes.rs` — expected to be none if T020-T022, T032-T034 were done correctly; this phase is verification-first by design.

**Checkpoint**: All backend stories complete and independently verified. Run the full `quickstart.md` automated gates (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `REQUIRE_DB_TESTS=1 cargo test -p server`) before moving to frontend.

---

## Phase 9: Frontend — AI Agent Settings Page (serves US1, US2, US3, US4)

**Goal**: Replace the fixture-only `features/tenant/ai-agent` page (currently a tab-selector stub over static data, per `frontend/CLAUDE.md`'s spec-003-era "fixtures only, no HTTP" rule) with a real settings surface backed by the six endpoints above. This is the first tenant settings page in this codebase backed by live HTTP rather than fixtures — treat `ApiService`/`ApiResponse<T>` usage here as the reference pattern any later settings page will copy.

**Depends on**: Phases 3, 5, 6, 7 (all backend endpoints must exist). Not story-scoped individually because the settings page is one cohesive form — the tasks below are grouped by concern (service → store → components) rather than by user story, but each component references which acceptance scenarios it satisfies.

- [ ] T074 [P] Create `frontend/apps/dashboard/src/app/core/api/ai-agent.models.ts` (or extend `tenant-api.models.ts` if that's this repo's convention — check where `QueueEntry` from T-referenced `escalations-api.service.ts` lives and follow the same file) with TypeScript interfaces mirroring `contracts/rest-api.md` exactly: `AgentConfigResponse`, `AgentConfigPayload`, `AgentOptionsResponse`, `EscalationRule`, `ProviderSelection`, etc.
- [ ] T075 Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent-api.service.ts`, following `escalations-api.service.ts`'s exact shape (`@Injectable({providedIn: 'root'})`, inject `ApiService`, one method per endpoint returning `Observable<ApiResponse<T>>`): `getAgent()`, `saveAgent(payload)`, `getOptions()`, `uploadAvatar(blob, contentType)`, `getAvatarUrl()` (returns the URL string for an `<img src>`, no fetch needed), `setConversationAiHandling(conversationId, mode)` (for the banner in Phase 10 — colocate here since it's the same DTO family, or place in the conversations feature's own API service if this repo keeps API services strictly per-route-owner; check `escalations-api.service.ts` vs where conversation endpoints live and match that convention).
- [ ] T076 [P] Add `ai-agent-api.service.spec.ts` covering each method against a mocked `ApiService` (follow `escalations-api.service.spec.ts`'s existing test style).
- [ ] T077 Rebuild `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.store.ts` as an NgRx SignalStore (replacing the current tab-only stub, keeping the `activeTab` state) with: loaded `config`/`options` signals, a dirty in-progress edit signal, `load()` (calls `getAgent()` + `getOptions()` in parallel), `save()` (calls `saveAgent`, on `409` sets a `conflict` signal the component reads to show a "reload" prompt, on `422` sets a `fieldErrors` signal), `uploadAvatar()`, computed `brokenSkillRefs`/`staleProviderSelection` passthroughs from the loaded config.
- [ ] T078 [P] Update `ai-agent.store.spec.ts` for the new behavior (load, save success, save 409, save 422, avatar upload success/failure).
- [ ] T079 Update `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.component.ts` to render real sections (Identity: name + avatar picker + tone; Prompt; Rules; Channels; Provider/Model) driven by the store instead of fixtures, with a visible "not yet configured — showing defaults" notice when `configured === false`, and a visible "agent inactive (no channels enabled)" notice when `enabled_channels` is empty (US4 scenario 3).
- [ ] T080 [P] Update `ai-agent.component.spec.ts` accordingly.
- [ ] T081 [P] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt-editor.component.ts` — bounded `<textarea>` with a live character counter against `options.prompt_max_length`.
- [ ] T082 [P] Create `tone-selector.component.ts` — renders `options.tones` as a Taiga-wrapped select/radio group (per `frontend/CLAUDE.md`: Taiga components wrapped in project components, no raw Taiga in feature pages — check `shared/components/` for an existing wrapped select to reuse before building a new one).
- [ ] T083 [P] Create `avatar-picker.component.ts` — grid of `options.avatar_presets` plus an upload control with client-side size/type pre-checks (mirroring the 256KB/png-jpeg-webp limits) before calling the store's `uploadAvatar()`.
- [ ] T084 [P] Create `rules-editor.component.ts` — two sub-lists (business rules: reorderable text list; escalation rules: name + trigger radio + conditional keywords input + skill multi-select) with per-rule "broken skill reference" inline warnings driven by `brokenSkillRefs`.
- [ ] T085 [P] Create `provider-model-selector.component.ts` — provider select limited to `options.providers` where `credential_available`, dependent model select from that provider's `models`, a "follow platform default" null option showing `options.ai_layer_default`, and a stale-selection warning banner driven by `staleProviderSelection`.
- [ ] T086 Wire T081-T085 into `ai-agent.component.ts`'s template.

**Checkpoint**: `pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check` all pass; manually drive `quickstart.md` steps 1, 3-15 in a browser.

---

## Phase 10: Frontend — AI-Handling Decision Banner (serves US6)

**Depends on**: Phase 4 (backend `ai-handling` endpoint) and the conversations feature's existing detail view.

- [ ] T087 [P] Extend the conversations feature's existing API service/store (wherever conversation detail is fetched — check `features/tenant/conversations/`) to surface `ai_handling`/`awaiting_ai_decision` from the detail response and expose a `setAiHandling(mode)` call.
- [ ] T088 Create `frontend/apps/dashboard/src/app/features/tenant/conversations/ai-handling-banner.component.ts` — visible only when `awaiting_ai_decision` is true, with two actions ("Use platform AI", "Assign to a human"); the platform-AI action is disabled with a shown reason when `options.ai_layer_default` (fetched once, cached) indicates nothing resolves; on success, hides itself (banner disappears once a decision is made, or once the tenant configures its own agent).
- [ ] T089 [P] Add `ai-handling-banner.component.spec.ts` covering both actions, the disabled-with-reason state, and the banner disappearing after a decision.
- [ ] T090 Wire the banner into the conversation detail view template.

**Checkpoint**: Full feature complete end-to-end, frontend and backend.

---

## Phase 11: Polish & Cross-Cutting

- [x] T091 Run the complete `quickstart.md` automated gate list (backend `fmt`/`clippy`/`cargo test --workspace`/`REQUIRE_DB_TESTS=1 cargo test -p server`; frontend `ng build`/`ng test`/`lint`/`format:check`) and fix anything red.
- [ ] T092 Manually walk all 15 steps of `quickstart.md`'s scenario walkthrough in a running instance (backend + dashboard), confirming each success-criteria spot check (SC-001 through SC-006).
- [ ] T093 [P] Re-read `data-model.md`, `contracts/rest-api.md`, and `contracts/agent-runtime.md` against the final code and fix any drift (e.g. if a field name or status code ended up different during implementation, update the docs to match reality — the contract documents must describe what was actually built).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies.
- **Foundational (Phase 2)**: Depends on Setup. **Blocks every other phase** — it contains the migrations, the shared model/validation, the prompt composer, the rule matcher, and the complete responder pipeline that every story's independent test relies on.
- **US1 (Phase 3, P1, MVP)**: Depends on Foundational only.
- **US6 (Phase 4, P2)**: Depends on Foundational only (does not touch `agent_routes.rs`'s settings-page endpoints at all — could in principle run in parallel with US1 by a second developer, since it lives in different route handlers; sequenced here right after US1 for narrative clarity, not because of a file conflict).
- **US2 (Phase 5, P2)**: Depends on Foundational + US1 (extends the same `agent_routes.rs`/`agent_config.rs` files US1 created).
- **US3 (Phase 6, P2)**: Depends on Foundational + US1 (same file-extension reason).
- **US4 (Phase 7, P3)**: Depends on Foundational + US1 (same file-extension reason).
- **US5 (Phase 8, P3)**: Depends on US1-US4, US6 (verifies behavior they implemented; adds no new production code of its own).
- **Frontend Settings Page (Phase 9)**: Depends on US1, US2, US3, US4 (needs all six endpoints' final shape).
- **Frontend Banner (Phase 10)**: Depends on US6.
- **Polish (Phase 11)**: Depends on everything.

### Parallel Opportunities

- T001/T002 (Setup) — different stacks, fully parallel.
- Within Foundational: T006 (schema tests) can run parallel to T008-T013 (model/prompt/rules — different files) once T003-T005 (migrations) land; T011 and T013 (unit tests) are parallel to each other; T019 (server wiring) can start once T017/T018 exist but is otherwise independent of T020-T022 (RBAC/audit — different files).
- US1 tests T023 and (once the harness file exists) T024-T030 are sequential within `ai_agent.rs` (same file, avoid merge conflicts) but T023 (pure unit test in `agent_config.rs`) is parallel to all of them.
- US6, once Foundational is done, can be staffed in parallel with US1 by a second developer (different route handlers, different test focus) even though it's sequenced after US1 in this document.
- Frontend component tasks T081-T085 are mutually parallel (different files) once T074-T078 (models/service/store) exist.

## Implementation Strategy

### MVP First

1. Phase 1 (Setup) → Phase 2 (Foundational — the big one) → Phase 3 (US1).
2. **STOP and VALIDATE**: run `quickstart.md` steps 1, 3, 4, 6 by hand. An admin can configure identity/behavior and see real AI replies reflect it. This alone is a demonstrable, deployable increment.

### Incremental Delivery After MVP

3. Add Phase 4 (US6) — the platform now behaves sensibly for every tenant from day one, not just configured ones.
4. Add Phase 5 (US2) — provider/model becomes a tenant choice, not just an inherited default.
5. Add Phase 6 (US3) — the agent becomes safe to expose to real customers (rules + escalation).
6. Add Phase 7 (US4) — staged channel rollout.
7. Add Phase 8 (US5) — dedicated audit/RBAC verification pass.
8. Add Phase 9 + 10 (frontend) — the settings page and the decision banner become usable by actual tenant admins and staff, not just via `curl`.
9. Phase 11 (Polish) — final gate run and doc reconciliation.

### Notes for whoever implements this

- [P] tasks touch different files and have no unmet dependency — safe to hand to parallel workers/agents.
- Every `[US#]`-labeled task's file lives in a location shared with other stories in the same phase group (US1/US2/US3/US4 all extend `agent_routes.rs`/`agent_config.rs`); do not attempt those phases out of order or in parallel with each other.
- If a test in a later phase fails and traces back to a Foundational file (`agent_responder.rs`, `agent_prompt.rs`, `agent_rules.rs`), fix it there — do not duplicate logic into the story's own files.
- Commit after each task or logical group, per the repo's normal workflow.

---

## Phase 12: Convergence

**Source**: `/speckit-converge` assessment of the codebase against spec.md, plan.md, and tasks.md. The feature is otherwise substantially built — `cargo build`, `cargo clippy -D warnings`, all backend unit tests, and every frontend gate (`ng build`/`ng test`/`lint`/`format:check`) pass, and all 40 integration tests named in Phases 3-8 exist.

**⚠️ Verification caveat — read this first**: the convergence assessment ran with **no database and no Docker available**, so every `REQUIRE_DB_TESTS`-gated test in `backend/crates/server/tests/ai_agent.rs` **compiled and silently skipped rather than executed**. All 40 integration tests named in Phases 3-8 exist and look correct, but there is no evidence any of them has ever run. Every task below was found by reading code, not by a failing test — and T094 in particular means the whole responder is dead at runtime, which is exactly the class of bug those un-run tests were written to catch. **Before doing anything else in this phase, stand up a Postgres and run `REQUIRE_DB_TESTS=1 DATABASE_URL=... cargo test -p server`.** That single command is likely to surface more than this static pass could, and it is the only thing that actually verifies the responder pipeline, escalation routing, concurrency, avatar limits, isolation, and audit behavior.

- [x] T094 CRITICAL: Fix the responder's outbox claim, which violates a CHECK constraint and makes `process_agent_responder_once` fail on **every** invocation per plan T017 step 1 ("Claim one unprocessed `outbox_events` row exactly like `process_escalation_outbox_once`") (contradicts). Migration `0022_outbox_delivery_claims.sql` adds `CONSTRAINT outbox_claim_shape CHECK ((claimed_at IS NULL) = (claim_token IS NULL))`, and no later migration drops it. The claim query in `backend/crates/modules/ai/src/agent_responder.rs:18-32` runs `UPDATE outbox_events SET claimed_at = now() ...` **without setting `claim_token`**, so `claimed_at IS NOT NULL` while `claim_token IS NULL` → the CHECK fails → the `UPDATE` errors → `process_agent_responder_once` returns `Err` before any work, and `run_agent_responder_worker` logs and sleeps forever. **Nothing works today against a real database**: no AI replies (US1/AC3), no auto-acknowledgments (US6/AC1), no platform-AI replies (US6/AC2), no rule escalations (US3/AC2), no baseline escalation (FR-011). Compare `escalations::events::process_escalation_outbox_once` (`backend/crates/modules/escalations/src/events.rs:203-223`), the function T017 said to copy exactly, and mirror it: generate `let claim_token = Uuid::new_v4();`, `SET claimed_at = now(), claim_token = $1`, and `.bind(claim_token)`. While there, reconcile two further divergences from that same pattern: (a) the escalation claim orders its inner `SELECT` by `created_at ASC` and the responder does not, so customer messages are claimed in arbitrary rather than FIFO order; (b) the escalation claim decodes `RETURNING id` as `i64` while the responder decodes it as `Uuid` — one of the two contradicts the live schema, so confirm which against a real database and fix the loser. This task is the reason to bring a DB up before starting.

- [x] T095 CRITICAL: Eliminate the N+1 in the conversations inbox list per Constitution X ("N+1 queries are treated as defects, not style issues") and Constitution VIII ("N+1 query patterns MUST be avoided") (contradicts). `backend/crates/modules/conversations/src/routes.rs:349` calls `compute_awaiting_ai_decision` once per row inside the list loop; each invocation issues `ai_status.agent_configured(tenant_id)` (a DB round-trip via `agent_exists`), `queries::has_system_message(...)`, and conditionally `ai_status.platform_ai_available(tenant_id)` (a DB round-trip via `resolve_config`) — up to 3 queries × N rows per page, though both port methods take only `tenant_id` and so return the same answer for every row. plan.md T048 step 5 is explicit: "Call the port once per request (not per row) and reuse the two booleans across the list projection to avoid an N+1." Hoist `agent_configured` and `platform_ai_available` to a single call each before the loop and pass the resulting booleans into the per-row derivation; batch `has_system_message` as well (e.g. one `conversation_id = ANY($1)` query over the page's ids, added as a new helper on `conversations::queries` since that table is conversations-owned). Keep the derivation logic itself byte-identical — it is currently correct, including the `platform_ai && !available` branch that spec.md's edge case requires. The conversation *detail* handler is single-row and needs no change.

- [x] T096 HIGH: Add the missing credential check to platform-AI availability per FR-004b ("available only when that layer resolves") and US6/AC4 (partial). Both `agent_config::platform_ai_available` (`backend/crates/modules/ai/src/agent_config.rs:372`) and the `mode = "platform_ai"` 422 gate in `agent_routes::set_conversation_ai_handling` resolve availability with `crate::resolution::resolve_config` alone, which reads only `ai_configurations` (`resolution.rs:26-32`); a config row can exist with no `ai_credentials` row, and because `resolve_config` matches `tenant_id = $1 OR tenant_id IS NULL`, a single platform-scoped config row makes platform AI look available to **every** tenant. plan.md T048 step 2 requires the adapter be "backed by `agent_exists` (T044) and `credential_resolves` (T009) over the provider from `crate::resolution::resolve_config`", and T046 requires the 422 gate to reject when "its provider's credential doesn't resolve" — the port's own doc comment already promises "config + credential". Chain `agent_config::credential_resolves(pool, tenant_id, &resolved.row.provider)` after the `resolve_config` lookup in both places. Consequence today: staff are offered platform AI, the responder hits `AiCallError::NotConfigured` and silently auto-acks, and `awaiting_ai_decision` stays `false` — so the banner never reappears and human escalation is unreachable, which is precisely the spec.md edge case ("if it becomes unresolvable later, affected conversations fall back to awaiting a decision"). **No existing test catches this**: `unresolvable_platform_ai_returns_to_awaiting_decision` (T043b, `ai_agent.rs:1574`) deletes *both* the credential and the `ai_configurations` row, so `resolve_config` already returns `None` and the test passes against the current buggy code. Per Constitution VII ("every bug fix MUST introduce a regression test that fails before the fix and passes after"), add a case that deletes **only** the credential and leaves the config row, and assert `awaiting_ai_decision` returns to `true`.

- [x] T097 [P] MEDIUM: Add a claimable partial index for the responder's outbox event type per Constitution X ("MUST optimize for ... efficient queries") and plan.md Performance Goals ("responder query paths ride existing indexes") (missing). The responder claims on `WHERE event_type = 'conversation.customer_message' AND claimed_at IS NULL`, but no index covers it: `outbox_escalations_claimable_idx` (0037) is partial on the two escalation event types, and `outbox_invitation_delivery_pending_idx`/`_claimable_idx` (0021/0022) are partial on `invitation.email_delivery`. Every claim therefore sequentially scans `outbox_events`, a table that grows with total platform message volume, on every responder iteration (once per second per the worker loop). Add a migration `0044_outbox_customer_message_idx.sql` mirroring 0037's shape: `CREATE INDEX outbox_customer_message_claimable_idx ON outbox_events (event_type, created_at ASC) WHERE event_type = 'conversation.customer_message' AND claimed_at IS NULL;` — pairs with T094's `ORDER BY created_at ASC`. Add the matching assertion to `backend/crates/shared/db/tests/schema.rs`.

- [x] T098 [P] MEDIUM: Restore the `cargo fmt --all --check` gate required by T091 and `quickstart.md`'s automated gate list (contradicts). The check currently exits 1 with 74 diffs across 11 files: `modules/ai/src/{agent_config,agent_responder,agent_routes,agent_rules}.rs`, `modules/conversations/src/{queries,routes}.rs`, `server/src/{handlers,main,router}.rs`, `server/tests/ai_agent.rs`, and `shared/db/tests/schema.rs`. Run `cargo fmt --all` from `backend/`, then confirm `cargo fmt --all --check` exits 0 and `cargo clippy --workspace --all-targets -- -D warnings` still passes. Do this **last** — it will conflict with the edits in T094-T097.

---

## Phase 13: Convergence

**Source**: second `/speckit-converge` assessment. Phase 12's five tasks were re-verified against the code and are **genuinely implemented**, not merely checked off: the claim query now sets `claim_token` and orders by `created_at ASC` (T094); the inbox list hoists both `AiAgentStatus` port calls above the loop and batches acks via `has_system_message_batch` (T095); `platform_ai_available` chains `credential_resolves` (T096); `0044_outbox_customer_message_idx.sql` exists in 0037's shape (T097); and `cargo fmt --all --check` exits 0 (T098). Backend unit tests, `pnpm ng build/test dashboard`, `pnpm lint`, and `pnpm format:check` all pass. Only one gap remains.

- [x] T099 MEDIUM: Drive `cargo clippy --workspace --all-targets -- -D warnings` fully green, as required by T091 and `quickstart.md`'s automated gate list (contradicts). The gate currently exits **101**. T091 and T098 are both checked off asserting this gate passes, but it does not — the earlier convergence run misread it as green because the command was piped (`cargo clippy ... | tail`), which reports `tail`'s exit status rather than clippy's; run it unpiped (`cargo clippy --workspace --all-targets -- -D warnings; echo $?`) to see the true result. Two known failures, both `clippy::needless_borrows_for_generic_args` (implied by `-D warnings`), and both in **pre-existing test code outside this feature's modules** — they were not introduced by Phase 12:
  - `backend/crates/shared/db/tests/schema.rs:1303` in `explain_routing_candidate_selection_uses_index()` — `.bind(&[mid])` → `.bind([mid])`
  - `backend/crates/modules/tenancy/src/invitations.rs:1668` in `concurrent_expired_replacement_only_one_succeeds()` — `.bind(&Uuid::new_v4().to_string())` → bind the owned `String`

  Fix the gate, not just these two lines: a crate that fails to compile under `-D warnings` stops before reporting its own remaining lints, so expect further lints to surface once `db` and `tenancy` compile clean. Re-run until the whole workspace is green, then confirm `cargo fmt --all --check` still exits 0.

**Note for T092 (manual walkthrough) — one unresolved question that only a database can answer.** The responder's claim decodes `RETURNING id` as `i64` and `tenant_id` as `Uuid`, while migration `0002_outbox.sql` declares `id UUID PRIMARY KEY` and `tenant_id TEXT NULL`; the emit functions bind `Uuid::new_v4()` into `id`. This is **not** filed as a task because the responder matches `escalations::events::process_escalation_outbox_once` exactly — the function T017 instructed it to copy — and `conversations::outbox::emit_status_changed_in_tx` binds identically, so this is a pre-existing property of the shared outbox rather than an 017 gap. `sqlx::query_as` is runtime-checked, so a green build proves nothing here and the contradiction cannot be settled by reading files: either the migrations are authoritative (and the escalation worker has the same defect), or the deployed schema differs from `0002`. **If, when you run against a real instance, the responder never fires or errors on claim, investigate the outbox decode types across `escalations` and `ai` together** — and fix them as one platform-wide reconciliation, not by diverging `ai` from the pattern it was built to mirror.
