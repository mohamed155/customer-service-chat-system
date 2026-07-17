---

description: "Task list for feature 018-prompt-management"

---

# Tasks: Prompt Management

**Input**: Design documents from `/specs/018-prompt-management/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/rest-api.md](./contracts/rest-api.md), [contracts/prompt-runtime.md](./contracts/prompt-runtime.md), [quickstart.md](./quickstart.md)

**Audience note**: These tasks assume no prior context beyond what's written in each task. Every task names its exact file(s) and, where the codebase already has an equivalent pattern, points at the exact file/line to copy the style from. When a task says "mirror X", open X first and match its structure — do not invent a different structure. **No new Cargo or npm dependencies are needed anywhere in this feature.**

**Tests**: Backend integration/unit tests and frontend spec files ARE requested (see plan.md Testing section) — this tasks.md includes them inline with each story's implementation tasks, not as a separate optional block.

**Organization**: Tasks are grouped by user story (from spec.md) so each story is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel with other [P] tasks in the same phase (different files, no ordering dependency)
- **[Story]**: US1–US5, matching spec.md's priorities (US1/US2 = P1, US3/US4 = P2, US5 = P3)
- Tasks with no [Story] label are Setup or Foundational (must complete first, in order, before any user story)

---

## Phase 1: Setup

- [X] T001 Confirm a clean baseline before touching anything: `cd backend && cargo check --workspace` and `cd frontend && pnpm nx run dashboard:build`. Both must succeed with the *current* (pre-018) code. If either fails, stop and fix the pre-existing issue first — do not build 018 on top of a broken baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema + pure validation/rendering logic + data-access layer + audit helpers that every user story (US1–US5) depends on. No user-facing endpoint exists yet after this phase — that's built story-by-story starting in Phase 3.

**⚠️ CRITICAL**: Nothing in Phase 3+ compiles until T002–T010 are done, because migration 0045 drops a column that existing code (`agent_config.rs`, `ai_agent.rs` tests) currently reads.

- [X] T002 Create `backend/migrations/0045_agent_prompts.sql`. Follow the exact conventions of `backend/migrations/0041_agent_configurations.sql` (UUID PK `DEFAULT gen_random_uuid()`, `tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT`, `updated_at` trigger function per table) and `backend/migrations/0006_audit_logs.sql` (append-only `forbid_mutation()` trigger function — it already exists in the DB from migration 0006 and is generic, so **reuse it**, do not redefine it). The migration must contain, in this order:
  1. `CREATE TABLE agent_prompts` — columns: `id UUID PK`, `tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT`, `prompt_kind TEXT NOT NULL DEFAULT 'system' CHECK (prompt_kind IN ('system'))`, `active_version INTEGER NOT NULL CHECK (active_version > 0)`, `created_at`/`updated_at TIMESTAMPTZ NOT NULL DEFAULT now()`, `deleted_at TIMESTAMPTZ NULL`.
  2. `CREATE UNIQUE INDEX agent_prompts_tenant_kind_uq ON agent_prompts (tenant_id, prompt_kind) WHERE deleted_at IS NULL`.
  3. A `set_agent_prompts_updated_at()` trigger function + `BEFORE UPDATE` trigger, copying the pattern at `backend/migrations/0041_agent_configurations.sql` lines 43-54.
  4. `CREATE TABLE agent_prompt_versions` — columns: `id UUID PK`, `tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT`, `prompt_id UUID NOT NULL REFERENCES agent_prompts(id) ON DELETE RESTRICT`, `version_number INTEGER NOT NULL CHECK (version_number > 0)`, `content TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 8000)`, `change_note TEXT NULL CHECK (change_note IS NULL OR char_length(change_note) <= 500)`, `restored_from INTEGER NULL`, `created_by_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL`, `created_by_display TEXT NOT NULL`, `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`. No `updated_at`, no `deleted_at`, no update trigger — this table is append-only.
  5. `CREATE UNIQUE INDEX agent_prompt_versions_prompt_version_uq ON agent_prompt_versions (prompt_id, version_number)`.
  6. `CREATE INDEX agent_prompt_versions_tenant_prompt_created_idx ON agent_prompt_versions (tenant_id, prompt_id, version_number DESC)` — the history cursor's access path.
  7. `CREATE TRIGGER agent_prompt_versions_append_only BEFORE UPDATE OR DELETE ON agent_prompt_versions FOR EACH ROW EXECUTE FUNCTION forbid_mutation();` (reusing the function from migration 0006).
  8. Backfill, in two statements, **before** the column drop: `INSERT INTO agent_prompts (tenant_id, prompt_kind, active_version) SELECT tenant_id, 'system', 1 FROM agent_configurations WHERE deleted_at IS NULL AND trim(system_prompt) <> ''`; then `INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_user_id, created_by_display) SELECT ac.tenant_id, ap.id, 1, ac.system_prompt, NULL, 'Migration backfill' FROM agent_configurations ac JOIN agent_prompts ap ON ap.tenant_id = ac.tenant_id AND ap.prompt_kind = 'system' WHERE ac.deleted_at IS NULL AND trim(ac.system_prompt) <> ''`.
  9. `ALTER TABLE agent_configurations DROP COLUMN system_prompt;`
  See [data-model.md](./data-model.md) for the full rationale of every column.

- [X] T003 [P] In `backend/crates/shared/db/tests/schema.rs`, add a `migration_0045_*` test block (mirror the style of the `migration_0041_*` tests starting at line 4802 — same file, `#[tokio::test]` per assertion, same `db::run_migrations` + raw-SQL-insert-expect-error pattern). Cover: (a) `agent_prompts` CHECK on `prompt_kind` rejects a value other than `'system'`; (b) `agent_prompt_versions` CHECK rejects empty content, content > 8000 chars, and `change_note` > 500 chars; (c) the partial unique `(tenant_id, prompt_kind)` on `agent_prompts` rejects a second live row for the same tenant; (d) the unique `(prompt_id, version_number)` on `agent_prompt_versions` rejects a duplicate version number for the same prompt; (e) a raw `UPDATE agent_prompt_versions SET content = '...' WHERE id = ...` and a raw `DELETE FROM agent_prompt_versions WHERE id = ...` both fail with a Postgres error (append-only trigger); (f) `agent_configurations` no longer has a `system_prompt` column (assert a raw `SELECT system_prompt FROM agent_configurations` fails to prepare). Depends on T002.

- [X] T004 Edit `backend/crates/modules/ai/src/agent_config.rs`: remove `system_prompt` from `AgentConfigurationRow` (line 32), remove `system_prompt` from `AgentConfigPayload` (line 91), delete the `system_prompt` length check inside `validate_payload` (lines 127-133), remove the `.bind(&payload.system_prompt)` call and the `system_prompt` column/placeholder from the `INSERT` in `create_in_tx` (lines 279-304) and from the `UPDATE` in `update_in_tx` (lines 306-336). Update the `make_payload` test helper (lines 404-419) and every call site that passes a `prompt` argument (lines 421-472) to drop that parameter. This is a compile-fix for T002's column drop — do it in the same commit conceptually, before anything else touches `agent_config.rs`. Depends on T002.

- [X] T005 [P] Edit `backend/crates/server/tests/ai_agent.rs`: remove/update every assertion that reads or sets `systemPrompt` in the JSON request/response bodies used across the existing agent-config tests, so the file compiles and passes against T004's payload shape. Depends on T004.

- [X] T006 Create `backend/crates/modules/ai/src/prompt_validate.rs` (NEW). This is the authoritative, pure (no DB) validation/rendering module. Grammar reference: [contracts/prompt-runtime.md](./contracts/prompt-runtime.md) — re-read it before writing this file, it fully specifies the scanner semantics. Contents:
  - `pub struct PromptVariable { pub name: &'static str, pub description: &'static str, pub sample: &'static str }`.
  - `pub const VARIABLES: &[PromptVariable]` — exactly 4 entries per [research.md](./research.md) R4: `agent_name` ("The AI agent's customer-facing name", sample `"Aria"`), `tenant_name` ("The tenant's business name", sample `"Acme Support"`), `customer_name` ("The customer's display name", sample `"Jamie Lee"`), `channel` ("The conversation's channel", sample `"web_chat"`).
  - `pub const MAX_CONTENT_LENGTH: usize = 8000;` and `pub const MAX_CHANGE_NOTE_LENGTH: usize = 500;`.
  - `pub const STARTER_PROMPT: &str` — the editable first-run baseline shown when a tenant has no prompt row yet (spec.md's unconfigured-tenant edge case). 017 has no starter constant of its own (`default_agent_config()` at `agent_routes.rs:137` uses `String::new()`), so this feature introduces it. Exact copy to use:
    ```rust
    /// Editable baseline shown when a tenant has no prompt row yet.
    /// Must itself pass `validate_prompt` — asserted in this module's tests.
    pub const STARTER_PROMPT: &str = "You are {{agent_name}}, the customer \
    support assistant for {{tenant_name}}.\n\nHelp {{customer_name}} clearly \
    and concisely. If you don't know an answer, say so and offer to connect \
    them with a member of the team.";
    ```
  - `pub fn validate_prompt(content: &str) -> Result<(), Vec<crate::agent_config::ValidationIssue>>` — reuse the existing `ValidationIssue { field, code, message }` struct from `agent_config.rs` (do not redefine it). Rules, all of which may fire together (return every issue found, not just the first):
    - `required`: `content.trim().is_empty()`.
    - `too_long`: `content.chars().count() > MAX_CONTENT_LENGTH` (8000).
    - Placeholder scan (single left-to-right pass over the string, byte-offset tracked): only the two-char sequences `{{` and `}}` are significant; a lone `{` or `}` is literal prose and produces no issue. On `{{`: start collecting a name (`[a-z][a-z0-9_]*` only) until `}}`. If `}}` is reached and the collected name is empty, contains an invalid character, or doesn't start with a lowercase letter → `malformed_placeholder`, message names the offending fragment and its starting offset. If `{{` is opened and the string ends (or another `{{` starts) before a matching `}}` → `malformed_placeholder` for the unclosed opener. A `}}` encountered with no matching open `{{` → `malformed_placeholder`. A well-formed `{{name}}` whose `name` is not one of `VARIABLES`' names → `unknown_variable`, message names the variable and its offset. No escape syntax, no nesting, no tolerated whitespace inside braces (`{{ agent_name }}` is `malformed_placeholder`) — per contract, this keeps the grammar unambiguous.
  - `pub fn render_prompt(content: &str, vars: &std::collections::HashMap<&str, String>) -> String` — single left-to-right pass, same lexer as `validate_prompt`. Replace every well-formed `{{name}}` whose `name` is a key in `vars` with that value verbatim (no re-scanning of the inserted value — this is the injection-safety property). Any other span — malformed placeholders, or well-formed placeholders whose name isn't in `vars` — is copied through byte-for-byte unchanged (this matters for the backfilled-legacy-content edge case in the responder — see prompt-runtime.md's "Edge" section). Same `(content, vars)` must always produce identical output bytes (deterministic — no randomness, no locale-dependent formatting).
  - `#[cfg(test)] mod tests` covering at minimum: each of the 4 codes firing in isolation with correct offsets, multiple issues in one string, a fully valid string returning `Ok(())`, `render_prompt` determinism (call twice, assert equal), `render_prompt` leaving a customer-supplied value containing `{{agent_name}}`-like text unexpanded when inserted as a *value* (proves injection-safety), and `render_prompt` passing through non-catalog `{{`-sequences byte-for-byte when not in `vars` (legacy edge case), and `validate_prompt(STARTER_PROMPT).is_ok()` (the shipped baseline must never be invalid — this test is what stops a future catalog change from silently breaking every new tenant's first-run experience).

- [X] T007 Edit `backend/crates/modules/ai/src/lib.rs`: add `pub mod prompt_validate;` to the `pub mod` block (after line 61, alongside the other `pub mod` declarations). Depends on T006.

- [X] T008 Create `backend/crates/modules/ai/src/prompt_store.rs` (NEW). This is the data-access layer — all SQL for both tables lives here, nowhere else. Mirror the style of `agent_config.rs`'s `load_live`/`load_live_in_tx`/`create_in_tx`/`update_in_tx` (plain `sqlx::query_as` calls, `#[derive(sqlx::FromRow)]` row structs, `Transaction<'_, Postgres>` for the write path). Contents:
  - `#[derive(Debug, Clone, sqlx::FromRow)] pub struct AgentPromptRow { pub id: Uuid, pub tenant_id: Uuid, pub prompt_kind: String, pub active_version: i32, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc> }`.
  - `#[derive(Debug, Clone, sqlx::FromRow)] pub struct PromptVersionRow { pub id: Uuid, pub tenant_id: Uuid, pub prompt_id: Uuid, pub version_number: i32, pub content: String, pub change_note: Option<String>, pub restored_from: Option<i32>, pub created_by_user_id: Option<Uuid>, pub created_by_display: String, pub created_at: DateTime<Utc> }`.
  - `pub async fn active_content(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Option<String>>` — the responder hot-path read from [data-model.md](./data-model.md)'s "Active-content read" section: `SELECT v.content FROM agent_prompts p JOIN agent_prompt_versions v ON v.prompt_id = p.id AND v.version_number = p.active_version WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL`, `fetch_optional`.
  - `pub async fn load_bootstrap(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<Option<(AgentPromptRow, PromptVersionRow)>>` — load the live `agent_prompts` row for the tenant plus its active version row (join or two queries, your call), for the GET editor-bootstrap endpoint. Return `None` when no prompt row exists yet.
  - `pub async fn load_for_update_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid) -> sqlx::Result<Option<AgentPromptRow>>` — `SELECT ... FROM agent_prompts WHERE tenant_id = $1 AND prompt_kind = 'system' AND deleted_at IS NULL FOR UPDATE`, mirroring `agent_config::load_live_in_tx`.
  - `pub enum SaveOutcome { Created { version: i32 }, NoOp { version: i32 } }` and `pub enum SaveError { Conflict { active_version: i32 }, Db(sqlx::Error) }` (`impl From<sqlx::Error> for SaveError`).
  - `pub async fn save_version_in_tx(tx: &mut Transaction<'_, Postgres>, tenant_id: Uuid, base_version: i32, content: &str, change_note: Option<&str>, actor_user_id: Option<Uuid>, actor_display: &str, restored_from: Option<i32>) -> Result<SaveOutcome, SaveError>` implementing research R6 exactly:
    1. `load_for_update_in_tx` for `tenant_id`.
    2. If no row exists: only proceed if `base_version == 0` (else `Conflict { active_version: 0 }`); `INSERT INTO agent_prompts (tenant_id, prompt_kind, active_version) VALUES ($1, 'system', 1) RETURNING *` to lazily create the parent row, then insert version 1 (skip to step 5 with `new_version_number = 1`, no no-op check possible on first save).
    3. If a row exists and `base_version != row.active_version`: return `Conflict { active_version: row.active_version }`.
    4. If a row exists: fetch the active version's `content` (`SELECT content FROM agent_prompt_versions WHERE prompt_id = $1 AND version_number = $2`) and compare byte-for-byte to the `content` parameter. If equal, return `NoOp { version: row.active_version }` — insert nothing, the caller must not write an audit row either.
    5. `new_version_number = base_version + 1`. `INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, change_note, restored_from, created_by_user_id, created_by_display) VALUES (...)`. On a unique-violation `sqlx::Error` (the `(prompt_id, version_number)` index catching a race the `FOR UPDATE` lock should already have prevented), map to `SaveError::Conflict` by re-reading the current `active_version`.
    6. `UPDATE agent_prompts SET active_version = $1 WHERE id = $2` (skip if the row was just created in step 2 with `active_version` already `1`).
    7. Return `Created { version: new_version_number }`.
  - `pub async fn list_versions(pool: &PgPool, tenant_id: Uuid, limit: i64, before: Option<i32>) -> sqlx::Result<(Vec<PromptVersionRow>, bool)>` — join `agent_prompt_versions` to `agent_prompts` on `prompt_id` filtering `tenant_id` + `prompt_kind = 'system'`, `AND ($3::int IS NULL OR version_number < $3)`, `ORDER BY version_number DESC LIMIT $2 + 1`; fetch `limit + 1` rows, truncate to `limit`, `has_more = fetched.len() > limit as usize`.
  - `pub async fn get_version(pool: &PgPool, tenant_id: Uuid, version_number: i32) -> sqlx::Result<Option<(PromptVersionRow, bool)>>` — fetch the version row (tenant-scoped join as above) plus whether `version_number == agent_prompts.active_version` (the `is_active` flag).
  Depends on T002.

- [X] T009 Edit `backend/crates/modules/ai/src/lib.rs`: add `pub mod prompt_store;`. Depends on T008.

- [X] T010 [P] Edit `backend/crates/modules/ai/src/agent_audit.rs`: add two functions, mirroring `record_agent_config_updated` (lines 45-63) exactly in shape: `pub async fn record_agent_prompt_version_created(tx: &mut Transaction<'_, Postgres>, actor_user_id: Option<Uuid>, tenant_id: Uuid, prompt_id: Uuid, version_number: i32, content_len: usize, has_change_note: bool) -> sqlx::Result<()>` calling `tenancy::audit::record_in_tx(tx, "agent_prompt.version_created", actor_user_id, Some(tenant_id), "agent_prompt", Some(&prompt_id.to_string()), &json!({"version": version_number, "content_length": content_len, "has_change_note": has_change_note}))`; and `pub async fn record_agent_prompt_version_restored(tx: &mut Transaction<'_, Postgres>, actor_user_id: Option<Uuid>, tenant_id: Uuid, prompt_id: Uuid, version_number: i32, restored_from: i32, content_len: usize) -> sqlx::Result<()>` with action `"agent_prompt.version_restored"` and details `{"version": version_number, "restored_from": restored_from, "content_length": content_len}`. **Never include the prompt `content` string itself in either details payload** — this is a hard platform invariant (015), only lengths/numbers/booleans.

**Checkpoint**: `cd backend && cargo check --workspace && cargo test -p ai` passes. No routes exist yet — that starts in Phase 3.

---

## Phase 3: User Story 1 - Every Prompt Change Becomes a Recoverable Version (Priority: P1) 🎯 MVP

**Goal**: Saving the prompt creates an immutable, attributed version that becomes active immediately; stale/no-op saves are handled correctly; the AI agent's next reply uses the newest version.

**Independent Test**: Save three successive edits to the system prompt, then verify three distinct versions exist, each retrievable with its exact saved content, and that the newest one is the prompt the AI agent actually uses.

- [X] T011 [US1] In `backend/crates/modules/ai/src/prompt_routes.rs` (NEW), add the response/request DTOs for [contracts/rest-api.md](./contracts/rest-api.md) endpoints #1 and #2 — mirror the `#[derive(Debug, Clone, Serialize, ToSchema)] #[serde(rename_all = "camelCase")]` style used throughout `agent_routes.rs` (e.g. lines 64-120):
  - `PromptSummaryDto { exists: bool, active_version: i32, content: String, updated_at: Option<DateTime<Utc>>, updated_by: Option<String> }`
  - `VariableDto { name: String, description: String, sample: String }`
  - `LimitsDto { max_content_length: u32, max_change_note_length: u32 }`
  - `PromptBootstrapResponse { prompt: PromptSummaryDto, variables: Vec<VariableDto>, limits: LimitsDto }`
  - `PromptSavePayload { content: String, change_note: Option<String>, base_version: i32 }` (`Deserialize`, not `Serialize`)
  - `PromptSaveResponse { version: i32, created: bool, restored_from: Option<i32>, updated_at: Option<DateTime<Utc>>, updated_by: Option<String> }`
  Match the exact JSON shapes shown in the contract's endpoint #1 and #2 examples.

- [X] T012 [US1] In `prompt_routes.rs`, add `pub async fn get_prompt_bootstrap(State(pool): State<PgPool>, ctx: TenantContext, Extension(_principal): Extension<Principal>) -> Response` — mirror `agent_routes::get_agent_config` (lines 259-280) for the overall shape (match/Ok(Some)/Ok(None)/Err(e) with `ApiError::internal_error(...).with_request_id(&ctx.request_id)`). Call `prompt_store::load_bootstrap`. When `Some((prompt_row, version_row))`: `PromptSummaryDto { exists: true, active_version: prompt_row.active_version, content: version_row.content, updated_at: Some(version_row.created_at), updated_by: Some(version_row.created_by_display) }`. When `None`: `exists: false`, `active_version: 0`, `content: prompt_validate::STARTER_PROMPT.to_string()` (the editable first-run baseline from T006 — the client sends it straight back on the first save, with `baseVersion: 0`, so an unedited first save stores the starter text as version 1), `updated_at: None`, `updated_by: None`. Always include `variables: prompt_validate::VARIABLES.iter().map(...).collect()` and `limits: LimitsDto { max_content_length: prompt_validate::MAX_CONTENT_LENGTH as u32, max_change_note_length: prompt_validate::MAX_CHANGE_NOTE_LENGTH as u32 }`. Add a `#[utoipa::path(get, path = "/tenant/ai/agent/prompt", ...)]` annotation mirroring `get_agent_config`'s neighbor `get_agent_options` (lines 284-300) for the doc-comment/response-table style.

- [X] T013 [US1] In `prompt_routes.rs`, add `pub async fn put_prompt(State(pool): State<PgPool>, ctx: TenantContext, Extension(principal): Extension<Principal>, ApiJson(payload): ApiJson<PromptSavePayload>) -> Response` — mirror `agent_routes::put_agent_config`'s overall transaction shape (lines 374-610: validate → begin tx → mutate → audit → commit, with `let _ = tx.rollback().await;` before every early-return error). Steps:
  1. `if let Err(issues) = prompt_validate::validate_prompt(&payload.content) { return ApiError::unprocessable_entity("Validation failed").with_details(issues.iter().map(|i| serde_json::to_value(i).unwrap())).with_request_id(&ctx.request_id).into_response(); }`
  2. Begin tx (internal_error on failure, same pattern as `put_agent_config` lines 391-399).
  3. Call `prompt_store::save_version_in_tx(&mut tx, ctx.tenant_id, payload.base_version, &payload.content, payload.change_note.as_deref(), Some(principal.user_id), &principal.display_name, None)`.
  4. On `Err(SaveError::Conflict { active_version })`: rollback, `return ApiError::conflict("Prompt changed since it was loaded").with_details(vec![serde_json::json!({"activeVersion": active_version})]).with_request_id(&ctx.request_id).into_response();` — this is the `409 version_conflict` from the contract (the `ApiError::conflict` constructor always emits `code: "conflict"`; the contract's `version_conflict` label is documentation for the *situation*, not a literal required JSON `code` value — follow the same convention `escalations/src/routes.rs` uses for its structured 409s, i.e. `ApiError::conflict(message).with_details(vec![json!({...})])`).
  5. On `Err(SaveError::Db(e))`: log + rollback + `internal_error`.
  6. On `Ok(SaveOutcome::NoOp { version })`: rollback (nothing was written) or commit (no-op, harmless either way since nothing changed) — return `200 { version, created: false, restoredFrom: None, updatedAt: <active version's timestamp — re-fetch or thread it through>, updatedBy: <same> }`.
  7. On `Ok(SaveOutcome::Created { version })`: call `agent_audit::record_agent_prompt_version_created(&mut tx, Some(principal.user_id), ctx.tenant_id, <prompt_id>, version, payload.content.len(), payload.change_note.is_some())` (you'll need `save_version_in_tx` to also return or let you look up the `prompt_id` — either have it return the id, or re-query the parent row before committing), then commit, return `200 { version, created: true, restoredFrom: None, updatedAt: now, updatedBy: principal.display_name }`.
  Add the `#[utoipa::path(put, path = "/tenant/ai/agent/prompt", ...)]` annotation mirroring `put_agent_config`'s (lines 349-373), listing 200/401/403/409/422/500.

- [X] T014 [US1] Edit `backend/crates/modules/ai/src/lib.rs`: add `pub mod prompt_routes;`.

- [X] T015 [US1] Edit `backend/crates/server/src/router.rs`: mount the two new routes. Copy the exact `.routes(routes!(...).map(|_| { let get = ...; let put = ...; get.merge(put) }))` pattern at lines 536-548 (the `get_agent_config`/`put_agent_config` block), producing:
  ```rust
  .routes(
      routes!(
          ai::prompt_routes::get_prompt_bootstrap,
          ai::prompt_routes::put_prompt
      )
      .map(|_| {
          let get = routing::get(ai::prompt_routes::get_prompt_bootstrap)
              .route_layer(require_permission(Permission::AiAgentView));
          let put = routing::put(ai::prompt_routes::put_prompt)
              .route_layer(require_permission(Permission::AiAgentManage));
          get.merge(put)
      }),
  )
  ```
  Place it immediately after the existing `get_agent_config`/`put_agent_config` block (after line 548) since `/tenant/ai/agent/prompt` nests under the same prefix.

- [X] T016 [US1] Edit `backend/crates/modules/ai/src/agent_routes.rs`: add a read-only `active_prompt: Option<ActivePromptSummary>` field to `AgentDetail` (line 73-86), where `ActivePromptSummary { version: i32, updated_at: DateTime<Utc>, updated_by: String, excerpt: String }` (`excerpt` = first ~120 chars of the active version's content, single-line). Populate it in `get_agent_config` (and wherever else `AgentDetail` is constructed, e.g. `build_agent_response`) via `prompt_store::load_bootstrap`; `None` when no prompt row exists. `default_agent_config()` (line 124) gets `active_prompt: None`. This is the R10 summary card data source — do not add a way to *write* prompt content through this DTO, only read.

- [X] T017 [US1] Edit `backend/crates/modules/ai/src/agent_responder.rs`: replace the `row.system_prompt` read used to build `system_message` (lines 239-246) with:
  1. `let content = prompt_store::active_content(pool, tenant_id).await?.unwrap_or_default();` (empty string when no prompt row — matches 017's existing empty-prompt behavior, per data-model.md's active-content-read note).
  2. Skip rendering entirely on the `is_platform_persona` branch (pass `content = String::new()`, no `render_prompt` call — the persona has no tenant-authored prompt, per prompt-runtime.md).
  3. On the configured branch, build the runtime vars map: `agent_name` = `row.name`; `tenant_name` = `tenancy::authorize::fetch_tenant(pool, tenant_id).await.map(|t| t.name).unwrap_or_default()`; `customer_name` = `conversations::queries::customer_display_name(pool, tenant_id, conversation_id).await?.unwrap_or_else(|| "the customer".to_string())` (added in T018); `channel` = the already-parsed `channel` local variable (line 51).
  4. `let rendered = ai::prompt_validate::render_prompt(&content, &vars);` then pass `&rendered` into `agent_prompt::compose_system_message` where `&row.system_prompt` used to go.
  5. Add `prompt_version` to whatever structured tracing/log call already fires around a successful reply in this function (search for `tracing::` calls in this file) — bind the `active_version` you already have from `prompt_store` (or re-derive it), `0` when there's no prompt row or on the persona branch.
  Depends on T008, T018.

- [X] T018 [US1] Edit `backend/crates/modules/conversations/src/queries.rs`: add `pub async fn customer_display_name(pool: &PgPool, tenant_id: Uuid, conversation_id: Uuid) -> sqlx::Result<Option<String>>` right next to `message_body`/`recent_history` (lines 1225-1273) — same style, one query: `SELECT c.display_name FROM conversations conv JOIN customers c ON c.id = conv.customer_id WHERE conv.tenant_id = $1 AND conv.id = $2 AND conv.deleted_at IS NULL`, `fetch_optional` mapped through `query_scalar`.

- [X] T019 [US1] Create `backend/crates/server/tests/ai_agent_prompt.rs` (NEW). Copy the test-harness boilerplate verbatim from `backend/crates/server/tests/ai_agent.rs` lines 1-90 (`test_config`, `plain_state`, `wiremock_state`, `require_db_tests`, `get_pool`, plus whatever `seed_user`/`seed_membership`/`seed_tenant`/`authenticated_request`/`send`/`body_json` helpers that file defines — copy or `include!` them, matching however `ai_agent.rs` itself structures reuse). All tests `#[tokio::test]`, gated on `get_pool().await` returning `Some` (skip with an eprintln when `DATABASE_URL` unset, `panic!` when `REQUIRE_DB_TESTS=1`). Cover, for US1 scope only:
  - First save on a tenant with no prompt row: `PUT` with `baseVersion: 0` → `200 { version: 1, created: true }`; a follow-up `GET` bootstrap shows `exists: true, activeVersion: 1`.
  - Second save with `baseVersion: 1` → `200 { version: 2, created: true }`.
  - Save with a `baseVersion` that doesn't match the current active version → `409`, body's `error.details[0].activeVersion` equals the real current version.
  - Save with content byte-identical to the active version's content → `200 { created: false }`, and a follow-up history check (once T038-T039 land you can extend this — for now just assert `GET` bootstrap's `activeVersion` did not change).
  - Responder end-to-end: seed a configured tenant + conversation + customer, save a prompt containing `{{agent_name}}`, `{{tenant_name}}`, `{{customer_name}}`, `{{channel}}`, spin up a `wiremock` OpenAI mock (mirror `ai_agent.rs`'s wiremock setup), call `process_agent_responder_once`, and assert the request body wiremock captured has all four placeholders replaced with the real values (not the samples).
  - Assert the reply after a *second* save reflects the newer version's content (proves FR-017 bind-at-next-run).

- [X] T020 [US1] Add real-route RBAC coverage for the prompt endpoints in `backend/crates/server/tests/ai_agent_prompt.rs` (T019's file), mirroring `ai_agent.rs`'s `unauthorized_roles_get_403` (lines 2895-2945 — the same file you copied harness code from in T019) exactly. **Do NOT add entries to `rbac.rs`'s `TENANT_OPERATIONS` array.** Feature 017 deliberately did not register its agent routes there (`grep "ai/agent" backend/crates/server/tests/rbac.rs` returns nothing): the `ai_agent.view` / `ai_agent.manage` permission *codes* are already covered by that array's synthetic `/test/tenant/ai/view` and `/test/tenant/ai/manage` entries, and the array is positionally `.zip()`ed against fixed-length `expected: [bool; 18]` arrays (`rbac.rs:1377-1398`) — appending to it silently truncates rather than failing, so a new entry would look covered while testing nothing. Write two tests: (a) `for role in ["manager", "agent", "viewer"]` — seed that role, assert `GET /tenant/ai/agent/prompt` → `403` and `PUT /tenant/ai/agent/prompt` → `403`; (b) `for role in ["owner", "admin"]` — assert both succeed. Depends on T015, T019.

- [X] T021 [US1] Edit `backend/crates/server/tests/openapi_contract.rs`: add `"/tenant/ai/agent/prompt"` to the expected-paths list (near line 245-247), and add GET/PUT-exist assertions for it mirroring the `get_agent_config` pattern at lines 265-278. Also update whatever assertion currently checks the `AgentDetail`/agent DTO shape to account for `systemPrompt` no longer being present and `activePrompt` now being present (search this file for `systemPrompt` — it's referenced around the areas the grep at the start of this task found).

- [X] T022 [US1] Edit `frontend/apps/dashboard/src/app/core/api/ai-agent.models.ts`: remove `systemPrompt` from `AgentDetail` and `AgentConfigPayload`; add `activePrompt: { version: number; updatedAt: string; updatedBy: string; excerpt: string } | null` to `AgentDetail`. In the same file (or a new sibling `prompt.models.ts` if you prefer — pick one and be consistent with T023+), add: `PromptSummary { exists: boolean; activeVersion: number; content: string; updatedAt: string | null; updatedBy: string | null }`, `PromptVariable { name: string; description: string; sample: string }`, `PromptLimits { maxContentLength: number; maxChangeNoteLength: number }`, `PromptBootstrapResponse { prompt: PromptSummary; variables: PromptVariable[]; limits: PromptLimits }`, `PromptSavePayload { content: string; changeNote: string | null; baseVersion: number }`, `PromptSaveResponse { version: number; created: boolean; restoredFrom: number | null; updatedAt: string | null; updatedBy: string | null }` — field names camelCase, matching the backend's `#[serde(rename_all = "camelCase")]`.

- [X] T023 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-api.service.ts` (NEW). Mirror `ai-agent-api.service.ts` (lines 1-27) exactly — `@Injectable({ providedIn: 'root' })`, `inject(ApiService)`. Methods: `getPrompt(): Observable<ApiResponse<PromptBootstrapResponse>>` → `this.api.get('tenant/ai/agent/prompt')`; `savePrompt(payload: PromptSavePayload): Observable<ApiResponse<PromptSaveResponse>>` → `this.api.put('tenant/ai/agent/prompt', payload)`.

- [X] T024 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt.store.ts` (NEW). Mirror `ai-agent.store.ts`'s (full file) SignalStore structure: `withState`/`withComputed`/`withMethods(rxMethod for load, plain method for save)`/`withHooks(onInit → load())`. State: `{ bootstrap: PromptBootstrapResponse | null; editorContent: string; changeNote: string; dirty: boolean; loading: boolean; saving: boolean; error: string | null; conflict: { activeVersion: number } | null; fieldErrors: Record<string, string[]> | null; noOpNotice: boolean }`. Methods:
  - `load` (`rxMethod<void>`): calls `prompt-api.getPrompt()`, on success `patchState(store, { bootstrap: res.data, editorContent: res.data.prompt.content, dirty: false, loading: false })`.
  - `setContent(content: string)`: `patchState(store, { editorContent: content, dirty: true, fieldErrors: null })` — never clears `error`/`conflict` implicitly, only an explicit retry does.
  - `save()`: `patchState(store, { saving: true, error: null, conflict: null, fieldErrors: null, noOpNotice: false })`; call `prompt-api.savePrompt({ content: store.editorContent(), changeNote: store.changeNote() || null, baseVersion: store.bootstrap()?.prompt.activeVersion ?? 0 })`. On success with `created: true`: reload `bootstrap` (either re-call `load()` or patch `bootstrap.prompt` fields directly from the response) and `dirty: false`. On success with `created: false`: `patchState(store, { saving: false, noOpNotice: true })`, **do not** touch `editorContent`. On error `409`: `patchState(store, { saving: false, conflict: { activeVersion: err.details[0].activeVersion } })` — **do not** touch `editorContent` (FR-011: content is never discarded). On error `422`: build `fieldErrors` from `err.details` exactly like `ai-agent.store.ts`'s save() does (lines 87-93), **do not** touch `editorContent`. Any other error: set `error: err.message`.
  Mirror `ai-agent.store.ts` lines 80-100 closely for the error-branching shape.

- [X] T025 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-page.component.ts` (NEW, skeleton for this phase — US2/US3 extend it). Standalone component, `changeDetection: ChangeDetectionStrategy.OnPush`, injects `PromptStore` (or provides it locally via `providers: [PromptStore]`). Template: a textarea bound to `store.editorContent()` / `(ngModelChange)="store.setContent($event)"` — reuse the styling from `prompt-editor.component.ts` (border/radius/font tokens, character counter with `nearLimit` warning) rather than inventing new CSS; a change-note text input; a Save button (`[disabled]="!store.dirty() || store.saving()"`) calling `store.save()`; a conflict banner shown when `store.conflict()` is truthy, with a "Review & retry" action that reloads (`store.load()`); a no-op notice/toast shown when `store.noOpNotice()`. Leave clearly-marked template regions/comments for where the variables panel, preview panel, and history-drawer trigger will be inserted by US2/US3 (`<!-- US2: history drawer trigger goes here -->` etc.) so those phases have an obvious insertion point.

- [X] T026 [US1] Edit `frontend/apps/dashboard/src/app/core/router/app-paths.ts`: add `aiAgentPrompt: 'ai-agent/prompt',` next to `aiAgent: 'ai-agent',` (line 24), inside the same tenant paths object.

- [X] T027 [US1] Edit `frontend/apps/dashboard/src/app/core/router/page-title.ts`: add `'aiAgentPrompt'` to the page-title key union (line 13) and an entry `aiAgentPrompt: { title: 'Prompt Management', subtitle: 'Version, preview, and restore your AI agent\'s system prompt' },` next to the `aiAgent` entry (line 63).

- [X] T028 [US1] Edit `frontend/apps/dashboard/src/app/core/authz/permissions.ts`: add `[APP_PATHS.tenant.aiAgentPrompt]: 'ai_agent.view',` next to the `aiAgent` entry (line 37) — the page itself only needs view access to load; the save action is separately gated server-side by `ai_agent.manage` on the PUT route (same pattern the existing `aiAgent` page already uses for its own PUT).

- [X] T029 [US1] Edit `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts`: add a new route object immediately after the `aiAgent` route (after line 76), mirroring its exact shape:
  ```ts
  {
    path: APP_PATHS.tenant.aiAgentPrompt,
    canMatch: [permissionGuard],
    loadComponent: () =>
      import('./ai-agent/prompt/prompt-page.component').then((m) => m.PromptPageComponent),
    data: {
      pageTitle: 'aiAgentPrompt',
      requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.aiAgentPrompt],
    },
    title: PAGE_TITLES.aiAgentPrompt.title,
  },
  ```

- [X] T030 [US1] Edit `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.component.ts`: remove the `'prompt'` tab's `<app-prompt-editor>` usage (around lines 167-176) and replace its content with a read-only summary card: shows `store.config()?.agent.activePrompt` when non-null (`v{{version}} · updated {{updatedAt}} by {{updatedBy}}` + the `excerpt` text), or "No prompt configured yet" when `null`; a button/link `routerLink="APP_PATHS.tenant.aiAgentPrompt"` (via the tenant-scoped router helper this app already uses for tenant-prefixed links — check how other tenant nav links build their `routerLink`, e.g. search this component or a sibling for `routerLink=` usage) labeled "Manage prompt". Remove the `PromptEditorComponent` import (line 17) and its entry in the `imports` array (line 38).

- [X] T031 [US1] Search `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.component.ts` and `ai-agent.store.ts` for every place that assembles an `AgentConfigPayload` object (the save-form submission) and remove the `systemPrompt` field from it, since T022 removed it from the type and the compiler will point you at every remaining call site.

- [X] T032 [US1] Delete `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt-editor.component.ts` and `prompt-editor.component.spec.ts` — fully superseded per FR-018. (T030 already removed the import/usage; this task just deletes the now-orphaned files.)

- [X] T033 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-api.service.spec.ts` (NEW) — `HttpTestingController`-based, mirror `ai-agent-api.service.spec.ts`'s structure. Assert `getPrompt()` issues `GET tenant/ai/agent/prompt` and `savePrompt(payload)` issues `PUT tenant/ai/agent/prompt` with the payload as the body.

- [X] T034 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt.store.spec.ts` (NEW) — mirror `ai-agent.store.spec.ts`'s structure (mock `PromptApiService`, assert `patchState` outcomes via the store's public signals). Cases: `load()` populates `editorContent` from the bootstrap response; `save()` success (`created: true`) updates `bootstrap` and clears `dirty`; `save()` `created: false` sets `noOpNotice` and leaves `editorContent` untouched; `save()` on `409` sets `conflict` and leaves `editorContent` untouched; `save()` on `422` sets `fieldErrors` and leaves `editorContent` untouched.

- [X] T035 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-page.component.spec.ts` (NEW). Assert: editor renders the loaded content; Save button is disabled until the user types (dirty); after a simulated 409 the conflict banner appears and the textarea's value is unchanged from what the user typed.

- [X] T036 [US1] Edit `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.component.spec.ts`: remove assertions about the old prompt textarea tab content; add an assertion that the Prompt tab renders the summary card and that its "Manage prompt" control navigates to `APP_PATHS.tenant.aiAgentPrompt`.

- [X] T037 [US1] Edit `frontend/apps/dashboard/src/app/features/tenant/ai-agent/ai-agent.store.spec.ts`: remove any assertion that depends on `systemPrompt` being part of the saved payload or loaded config (T031's compiler-driven cleanup should have already surfaced these).

**Checkpoint**: Save → version → activate → responder-uses-it works end-to-end via the API and the new prompt page; the settings page's Prompt tab is now a read-only summary that navigates in. `cargo test -p server --test ai_agent_prompt` (with `REQUIRE_DB_TESTS=1 DATABASE_URL=...`) and `pnpm ng test dashboard` both pass for what exists so far.

---

## Phase 4: User Story 2 - Browse Version History and Restore a Previous Version (Priority: P1)

**Goal**: The version history drawer lists every version newest-first with pagination; selecting one shows its full content; restoring it creates a new version (roll-forward) that becomes active, itself recorded as a normal version referencing its source.

**Independent Test**: Create several versions, restore an older one, and verify (a) the agent now uses the restored content, (b) the restore appears in history as a new version referencing its source, and (c) all intermediate versions are still present and viewable.

- [X] T038 [US2] In `prompt_routes.rs`, add the DTOs for [contracts/rest-api.md](./contracts/rest-api.md) endpoints #3, #4, #5, same derive/style as T011: `PromptVersionListItemDto { version_number: i32, content_preview: String, change_note: Option<String>, restored_from: Option<i32>, created_at: DateTime<Utc>, created_by: String, is_active: bool }`; `PromptVersionListResponse { items: Vec<PromptVersionListItemDto>, has_more: bool }`; `PromptVersionDetailResponse { version_number: i32, content: String, change_note: Option<String>, restored_from: Option<i32>, created_at: DateTime<Utc>, created_by: String, is_active: bool }`; `RestorePayload { base_version: i32 }` (`Deserialize`). Note `PromptSaveResponse` from T011 already has a `restored_from` field — reuse it as the restore endpoint's response type (no new response DTO needed there, per the contract's "same shape/vocabulary as PUT").

- [X] T039 [US2] In `prompt_routes.rs`, add `pub async fn list_prompt_versions(State(pool): State<PgPool>, ctx: TenantContext, Extension(_principal): Extension<Principal>, Query(params): Query<...>) -> Response` for `GET /tenant/ai/agent/prompt/versions?limit&before`. Clamp `limit` to `1..=100` (default `25` when absent — use axum's typical `#[derive(Deserialize)] struct ListVersionsQuery { limit: Option<i64>, before: Option<i32> }` extracted with `Query<ListVersionsQuery>`, mirroring however other paginated list endpoints in this codebase parse query params — check `routes.rs` or `usage.rs` in this same crate for the existing convention on optional query params before inventing one). Call `prompt_store::list_versions`. Map each row to `PromptVersionListItemDto`, with `content_preview` = first 160 chars of `content`, single-line (replace `\n`/`\r` with spaces), and `is_active` = `version_number == <the prompt's active_version>` (you'll need that value alongside the rows — extend `list_versions`'s return or issue one extra cheap lookup). No prompt row at all → `{ items: [], hasMore: false }` (not a 404).

- [X] T040 [US2] In `prompt_routes.rs`, add `pub async fn get_prompt_version(State(pool): State<PgPool>, ctx: TenantContext, Extension(_principal): Extension<Principal>, Path(version_number): Path<i32>) -> Response` for `GET /tenant/ai/agent/prompt/versions/{number}`. Call `prompt_store::get_version`; `None` (unknown version or tenant mismatch — these must be indistinguishable per the isolation convention) → `ApiError::not_found(...)`; `Some` → `200` with `PromptVersionDetailResponse`.

- [X] T041 [US2] In `prompt_routes.rs`, add `pub async fn restore_prompt_version(State(pool): State<PgPool>, ctx: TenantContext, Extension(principal): Extension<Principal>, Path(version_number): Path<i32>, ApiJson(payload): ApiJson<RestorePayload>) -> Response` for `POST /tenant/ai/agent/prompt/versions/{number}/restore`:
  1. `prompt_store::get_version(pool, ctx.tenant_id, version_number)` → `None` → `404 not_found`.
  2. `prompt_validate::validate_prompt(&source.content)` → on `Err` return the same `422` shape as `put_prompt` (T013 step 1) — this is the spec US4 scenario 5 "restore blocked because the catalog shrank" path.
  3. Begin tx, call `prompt_store::save_version_in_tx(&mut tx, ctx.tenant_id, payload.base_version, &source.content, None, Some(principal.user_id), &principal.display_name, Some(version_number))` — **reuse the same function T008 built for PUT**, just passing `restored_from: Some(version_number)`. Same `Conflict`/`NoOp`/`Created` handling as T013 steps 4-7, except on `Created` call `agent_audit::record_agent_prompt_version_restored` (not `_created`) and the `200` response includes `restoredFrom: Some(version_number)`.
  Add `#[utoipa::path(post, path = "/tenant/ai/agent/prompt/versions/{number}/restore", ...)]`.

- [X] T042 [US2] Edit `backend/crates/server/src/router.rs`: mount the 3 new routes right after T015's block. `GET /tenant/ai/agent/prompt/versions` and `GET /tenant/ai/agent/prompt/versions/{number}` are distinct paths from each other and from the restore POST, so each needs its own `.routes(routes!(handler).layer(require_permission(...)))` entry — mirror the single-route pattern at lines 520-527 (`get_tenant_usage_detail`) rather than the two-verbs-one-path `.map()` pattern, since none of these three share a path with each other:
  ```rust
  .routes(
      routes!(ai::prompt_routes::list_prompt_versions)
          .layer(require_permission(Permission::AiAgentView)),
  )
  .routes(
      routes!(ai::prompt_routes::get_prompt_version)
          .layer(require_permission(Permission::AiAgentView)),
  )
  .routes(
      routes!(ai::prompt_routes::restore_prompt_version)
          .layer(require_permission(Permission::AiAgentManage)),
  )
  ```

- [X] T043 [US2] Extend `backend/crates/server/tests/ai_agent_prompt.rs` (from T019) with: history pagination (save >25 versions of a tenant's prompt in a loop, then page through with `limit`/`before`, assert `hasMore` flips to `false` on the last page and every version number appears exactly once across all pages); version detail 200 + 404-for-unknown-number + 404-cross-tenant; restore happy path (restore an older version, assert the new version's content equals the source's, `restoredFrom` is set, it becomes `active_version`, and a subsequent responder run — reuse the T019 wiremock helper — reflects the restored content); restore no-op (restoring a version whose content equals the current active content → `created: false`); restore conflict (stale `baseVersion` → `409`); restore-blocked-by-validation (insert a version row **directly via raw SQL** in the test — bypassing `validate_prompt` — whose content references a variable name not in `prompt_validate::VARIABLES` (e.g. `{{business_hours}}`), since v1's fixed catalog can never produce this state through the API itself; then call restore on that version and assert `422`; this exercises spec US4 scenario 5 / prompt-runtime.md's "Edge: backfilled legacy content" note); audit assertions for `agent_prompt.version_restored` (actor, tenant, `restored_from` in details, no content in details); restore 404 for a cross-tenant version number.

- [X] T044 [US2] Extend the RBAC tests in `backend/crates/server/tests/ai_agent_prompt.rs` (from T020) to cover the three US2 routes, with the same structure and the same reasoning (**no `rbac.rs` changes** — see T020 for why): manager/agent/viewer → `403` and owner/admin → success, for `GET /tenant/ai/agent/prompt/versions`, `GET /tenant/ai/agent/prompt/versions/{number}`, and `POST /tenant/ai/agent/prompt/versions/{number}/restore`.

- [X] T045 [US2] Edit `backend/crates/server/tests/openapi_contract.rs`: add the 3 new paths to the expected-paths list and assert their HTTP methods (GET/GET/POST) and response schemas are registered, mirroring T021's additions.

- [X] T046 [US2] Edit `frontend/apps/dashboard/src/app/core/api/ai-agent.models.ts` (or `prompt.models.ts`, wherever T022 put the prompt DTOs): add `PromptVersionListItem { versionNumber: number; contentPreview: string; changeNote: string | null; restoredFrom: number | null; createdAt: string; createdBy: string; isActive: boolean }`, `PromptVersionListResponse { items: PromptVersionListItem[]; hasMore: boolean }`, `PromptVersionDetail { versionNumber: number; content: string; changeNote: string | null; restoredFrom: number | null; createdAt: string; createdBy: string; isActive: boolean }`, `RestorePayload { baseVersion: number }`.

- [X] T047 [US2] Edit `frontend/.../prompt/prompt-api.service.ts` (from T023): add `listVersions(limit?: number, before?: number): Observable<ApiResponse<PromptVersionListResponse>>` (build `HttpParams` conditionally, mirror `ApiService.list`'s param-building style at `api.service.ts` lines 25-28), `getVersion(versionNumber: number): Observable<ApiResponse<PromptVersionDetail>>` → `GET tenant/ai/agent/prompt/versions/${versionNumber}`, `restoreVersion(versionNumber: number, baseVersion: number): Observable<ApiResponse<PromptSaveResponse>>` → `POST tenant/ai/agent/prompt/versions/${versionNumber}/restore` with `{ baseVersion }`.

- [X] T048 [US2] Edit `frontend/.../prompt/prompt.store.ts` (from T024): extend state with `{ historyItems: PromptVersionListItem[]; historyHasMore: boolean; historyLoading: boolean; selectedVersion: PromptVersionDetail | null }`. Add methods: `loadHistory(before?: number)` — appends to `historyItems` when `before` is provided (pagination), replaces when it's the first page; `selectVersion(versionNumber: number)` — calls `getVersion`, sets `selectedVersion`; `restore(versionNumber: number)` — calls `restoreVersion(versionNumber, store.bootstrap()?.prompt.activeVersion ?? 0)`, with the exact same `409`/`422`/no-op branching as `save()` (T024), and on success also refreshes `bootstrap` and re-runs `loadHistory()` from scratch (the restore added a new top-of-history item).

- [X] T048a [US2] Extend `frontend/apps/dashboard/src/app/shared/components/dialog-shell/dialog-shell.component.ts` with a `readonly variant = input<'center' | 'drawer-right'>('center')` that switches **only** the panel's positioning CSS. `center` keeps today's behavior exactly (`position: fixed; top: 50%; left: 50%` + transform, `width: min(440px, calc(100vw - 2rem))`, existing radius) — it must render byte-identically so no existing dialog regresses. `drawer-right` anchors the panel instead: `inset: 0 0 0 auto; width: min(560px, 100vw); max-height: 100dvh; border-radius: 0;`. The backdrop, focus handling, `aria-modal`, `role`, and keydown/dismiss logic are shared and unchanged — this is why we extend `dialog-shell` rather than build a second component (Constitution IX: reusable components exist before pages, and UI logic is never duplicated). `shared/components/` has no drawer today (verified: 22 components, none side-anchored), so this task is what makes plan.md's "composes existing shared/Taiga-wrapped components" true. Add a `variant: 'drawer-right'` rendering case to `dialog-shell.component.spec.ts` and keep the existing `center` cases green.

- [X] T049 [US2] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/version-history-drawer.component.ts` (NEW). Compose `<app-dialog-shell variant="drawer-right">` (from T048a) — do not hand-roll a panel, backdrop, focus trap, or dismiss logic, and do not use raw Taiga styling in this feature component (frontend CLAUDE.md). Inside the shell, list `store.historyItems()` newest-first: version number, `createdBy`, `createdAt` (formatted), `changeNote` if present, a "Restored from vN" badge when `restoredFrom` is set, an "Active" badge when `isActive`. A "Load more" control visible when `store.historyHasMore()`, calling `store.loadHistory(lastItem.versionNumber)`. Clicking a row calls `store.selectVersion(item.versionNumber)` and shows the full content (plus a simple client-side diff against the currently active content — a naive line-by-line or word-by-word comparison is sufficient, no diff library needed unless one is already a project dependency — check `package.json` first). A "Restore" button on the selected-version view that requires an explicit confirm step (a second click / confirm dialog — do not fire `store.restore()` on the first click) before calling `store.restore(selectedVersion.versionNumber)`.

- [X] T050 [US2] Edit `frontend/.../prompt/prompt-page.component.ts` (from T025): add a "Version history" trigger button that opens `<app-version-history-drawer>` (fill in the placeholder comment T025 left for this), calling `store.loadHistory()` when first opened.

- [X] T051 [P] [US2] Create `frontend/.../prompt/version-history-drawer.component.spec.ts` (NEW). Assert: items render newest-first; "Load more" calls `loadHistory` with the correct `before` cursor and appends rather than replaces; clicking "Restore" without confirming does not call the store; confirming calls `store.restore` with the right version number; `isActive`/`restoredFrom` badges render conditionally.

- [X] T052 [P] [US2] Extend `frontend/.../prompt/prompt.store.spec.ts` (from T034): `loadHistory` success/pagination-append cases; `restore` success updates `bootstrap` and reloads history; `restore` on `409` sets `conflict`; `restore` on `422` sets `fieldErrors`; `restore` no-op sets `noOpNotice`.

- [X] T053 [P] [US2] Extend `frontend/.../prompt/prompt-api.service.spec.ts` (from T033): request-shape assertions for `listVersions` (query params), `getVersion` (path param), `restoreVersion` (path param + body).

**Checkpoint**: US1 + US2 together deliver the full "safe prompt management" promise from spec.md (version + browse + restore). This is a reasonable point to demo.

---

## Phase 5: User Story 3 - Compose Prompts with Variables and Preview the Result (Priority: P2)

**Goal**: The variables panel lists the catalog and inserts placeholders at the cursor; the preview panel renders the current editor content with samples substituted, live, marking any unresolved/malformed placeholder distinctly.

**Independent Test**: Insert two supported variables into the prompt, verify the preview renders them with sample values and updates as the text is edited, then save and confirm the stored version preserves the variable placeholders (not the sample values).

- [X] T054 [US3] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-lang.ts` (NEW). A pure TS mirror of `prompt_validate.rs` (T006) — same grammar, same codes, same offset semantics, re-read [contracts/prompt-runtime.md](./contracts/prompt-runtime.md) before writing this. Exports:
  - `export interface PlaceholderSpan { start: number; end: number; name: string; valid: boolean }`
  - `export function scanPlaceholders(content: string): PlaceholderSpan[]` — the single-pass lexer.
  - `export interface ValidationIssue { field: string; code: 'required' | 'too_long' | 'malformed_placeholder' | 'unknown_variable'; message: string }`
  - `export function validatePrompt(content: string, catalogNames: string[]): ValidationIssue[]` — same rules as `validate_prompt` in T006 (required/too_long/malformed_placeholder/unknown_variable, offsets embedded in `message`).
  - `export interface PreviewResult { text: string; errorSpans: { start: number; end: number; reason: string }[] }`
  - `export function renderPreview(content: string, samples: Record<string, string>): PreviewResult` — substitutes catalog samples for valid placeholders in `text`; any malformed or unknown placeholder is left as-is in `text` but also recorded in `errorSpans` so the preview component can highlight it distinctly instead of rendering it as normal text (FR-009).

- [X] T055 [US3] Create `specs/018-prompt-management/contracts/prompt-validation-fixture.json` (NEW) — a JSON array of `{ "content": string, "expected": [{ "code": string, "nameContains": string | null }] }` table-driven test cases, covering: a fully valid prompt with all 4 variables; empty string; whitespace-only string; an 8001-char string; unclosed `{{agent_name`; a stray `}}` with no opener; `{{}}` (empty name); `{{Agent_Name}}` (invalid casing/char); `{{ agent_name }}` (whitespace inside braces — malformed per the no-tolerance rule); `{{business_hours}}` (well-formed but not in the catalog — `unknown_variable`); a string with two separate issues (e.g. one malformed + one unknown); a string with a single literal `{` and `}` that must NOT produce any issue. This file is the single source of truth for scanner/validator behavior — both T056 and T057 read it, so a scanner/validator change only needs one file edited to update both test suites.

- [X] T056 [US3] Edit `backend/crates/modules/ai/src/prompt_validate.rs`'s `#[cfg(test)]` block (from T006): add a test that `include_str!`s `specs/018-prompt-management/contracts/prompt-validation-fixture.json` (use a relative path from this file's location, e.g. `../../../../../specs/018-prompt-management/contracts/prompt-validation-fixture.json` — count the `../` carefully from `backend/crates/modules/ai/src/` to the repo root), parses it (add `serde_json` as a dev-dependency if `modules/ai`'s `Cargo.toml` doesn't already have it available for tests — check first, it likely already does since the crate uses `serde_json::Value` elsewhere), and asserts `validate_prompt`'s output matches every case's `expected` codes. Depends on T006, T055.

- [X] T057 [P] [US3] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-lang.spec.ts` (NEW). First try importing the fixture directly from its `specs/` location with a relative path (Angular's TS config needs `resolveJsonModule: true`, `esModuleInterop`-adjacent settings — check `frontend/tsconfig*.json` for whether JSON imports outside `src/` are already permitted by the build; a path outside the `frontend/` directory may be rejected by the bundler's file-system sandboxing). **If that import fails to resolve or build**, copy the fixture verbatim into `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/prompt-validation-fixture.json` and add a one-line comment at the top of both JSON files cross-referencing the other, noting they must be kept identical (R5's shared-fixture intent, achieved by disciplined duplication rather than build-tool cross-referencing, since Cargo and the Angular/Nx build have no shared file-resolution mechanism). Assert `validatePrompt`'s output matches every fixture case. Depends on T054, T055.

- [X] T058 [US3] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/variables-panel.component.ts` (NEW). Lists `store.bootstrap()?.variables` (name, description, sample). Each entry is clickable/has an "Insert" action that inserts `{{name}}` into the prompt editor's textarea at the **current cursor position** (not always appended at the end) — this requires either (a) a `ViewChild` reference to the textarea shared between this component and `prompt-page.component.ts` (lift the textarea element reference up, or move the textarea into this feature's shared state), or (b) emitting an `(insertVariable)` output event from this component that `prompt-page.component.ts` handles by reading `textareaEl.selectionStart`/`selectionEnd`, splicing the string, and calling `store.setContent(...)` with the cursor repositioned after the inserted placeholder. Pick whichever is simpler given how T025 structured the editor; document the choice with a one-line comment.

- [X] T059 [US3] Create `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/preview-panel.component.ts` (NEW). Takes the live `store.editorContent()` and `store.bootstrap()?.variables` (mapped to a `Record<name, sample>`), calls `prompt-lang.ts`'s `renderPreview()` on every change (use a `computed()` signal, not a manual subscription, so it's automatically live), and renders the substituted text with each `errorSpans` entry shown as a visually distinct chip/highlight (different background/border, not just plain text) rather than rendered as if it were valid content.

- [X] T060 [US3] Edit `frontend/.../prompt/prompt-page.component.ts` (from T025/T050): wire in `<app-variables-panel (insertVariable)="...">` and `<app-preview-panel [content]="store.editorContent()" [variables]="store.bootstrap()?.variables">` in the placeholder region T025 left for this, in a layout roughly matching the spec's description (editor + variables panel + preview panel, side by side or stacked — match whatever layout convention this app's other multi-panel feature pages use, e.g. check the escalations or knowledge-base feature for a precedent 2/3-column layout pattern).

- [X] T061 [P] [US3] Create `frontend/.../prompt/variables-panel.component.spec.ts` (NEW). Asserts all 4 catalog entries render with name/description/sample; clicking one emits/inserts `{{name}}` at a non-trivial cursor position (place the cursor mid-string in the test, not at 0 or the end, to actually prove cursor-position insertion works).

- [X] T062 [P] [US3] Create `frontend/.../prompt/preview-panel.component.spec.ts` (NEW). Asserts valid placeholders are substituted with samples; the rendered preview updates when the input `content` signal changes; an unknown/malformed placeholder is rendered with the error-chip treatment, not as plain substituted-looking text.

- [X] T063 [US3] Extend `backend/crates/server/tests/ai_agent_prompt.rs` (T019/T043) with one test: save a version whose content is `"Hi {{agent_name}} from {{tenant_name}}"`, then `GET` its version detail and assert the returned `content` field is byte-identical to what was submitted (placeholders preserved raw, never sample-substituted at rest) — proves spec US3 acceptance scenario 4's storage half.

**Checkpoint**: The editor is now fully usable — insert variables, see a live accurate preview, save preserves raw placeholders.

---

## Phase 6: User Story 4 - Invalid Prompts Are Blocked Before They Go Live (Priority: P2)

**Goal**: Unknown variables, malformed placeholders, empty content, and over-length content are all rejected — both inline while typing and authoritatively on save/restore — and a rejection never discards the user's in-editor content.

**Independent Test**: Attempt to save a prompt containing a misspelled variable name and verify the save is rejected with a message identifying the offending placeholder; fix it and verify the save succeeds.

- [X] T064 [US4] Edit `frontend/.../prompt/prompt-page.component.ts`: on every `store.editorContent()` change, compute `prompt-lang.ts`'s `validatePrompt(content, catalogNames)` (a `computed()` signal, reusing `prompt-lang.ts` from T054) and render each returned issue as an inline message near the offending fragment (an issue list under the editor is the simplest correct implementation — a list item per issue showing its `message`; pointing a literal on-canvas underline at the exact character offset inside a `<textarea>` is not natively possible without a richer editor widget, so a below-the-textarea issue list satisfying "surfaced inline... before they attempt to save" is acceptable unless this codebase already has a richer text-annotation component — check `shared/components/` first). Disable the Save button while any issue exists (`store.dirty() && clientIssues().length === 0`), in addition to T025's existing `!dirty` check. Gate the `required` issue specifically on the field being dirty or blurred — a first-time visitor must never see a validation error on an untouched form (spec.md's unconfigured-tenant edge case: they see the baseline "rather than an error"). All other codes (`too_long`, `malformed_placeholder`, `unknown_variable`) surface immediately as typed.

- [X] T065 [US4] Verify (and add a regression test if not already covered by T034) that `prompt.store.ts`'s `save()` and `restore()` never clear `editorContent` or `selectedVersion` on a `422` response — re-read T024's save() spec and T048's restore() spec; this task is a verification pass, only add code if you find a gap.

- [X] T066 [US4] Extend `backend/crates/server/tests/ai_agent_prompt.rs` (T019/T043/T063) with an explicit server-side validation-rejection matrix via `PUT`: (a) content containing `{{business_hours}}` → `422`, and the returned issue's `code == "unknown_variable"` with the variable name and an offset in the message; (b) `{{agent_name` (unclosed) → `422` `malformed_placeholder`; (c) a stray `}}` → `422` `malformed_placeholder`; (d) empty string and whitespace-only string → `422` `required`; (e) an 8001-character string → `422` `too_long`. For every case, also assert the tenant's active version is unchanged afterward (`GET` bootstrap still shows the pre-rejection `activeVersion` and `content`) — proves FR-011's "never becomes active" guarantee, not just the status code.

- [X] T067 [P] [US4] Extend `frontend/.../prompt/prompt-page.component.spec.ts` (T035): typing an unknown-variable placeholder shows the inline `unknown_variable` message and disables Save; removing/fixing it re-enables Save; simulating a server `422` response leaves the textarea's current text untouched (no reset, no data loss).

- [X] T068 [US4] Edit `frontend/.../prompt/version-history-drawer.component.ts` (T049): when `store.restore()` results in a `422` (the "catalog shrank since this version was saved" case), surface the same issue-list rendering used in T064 within the drawer's restore-confirmation UI, instead of failing silently or with a generic error toast.

**Checkpoint**: No invalid content can become active through either save or restore, and no rejection ever loses the user's typed content.

---

## Phase 7: User Story 5 - Prompt Changes Are Fully Audited (Priority: P3)

**Goal**: Every save and restore is directly, explicitly proven to produce a correct, content-free, append-only audit record attributing the right actor/tenant/version — not just incidentally covered by other stories' tests.

**Independent Test**: Have two different administrators each save a prompt change, then verify the history attributes each version to the correct person with the correct timestamp, and that the tenant's audit trail contains a corresponding entry for each save and restore.

- [X] T069 [US5] Extend `backend/crates/server/tests/ai_agent_prompt.rs` with dedicated audit assertions (querying `audit_logs` directly via `sqlx::query` against the test pool, mirroring how `rbac.rs`'s `permission_denial_writes_audit_reason_without_exposing_permission` test at lines 1107-1140 queries `audit_logs`): after a save, exactly one row exists with `action = 'agent_prompt.version_created'`, `actor_user_id` matching the acting principal's id, `tenant_id` correct, `resource_type = 'agent_prompt'`, and `details` containing the version number/content length/change-note-presence but **not** a substring of the actual prompt content anywhere in the serialized `details` JSON (assert this negatively — `!details.to_string().contains(&submitted_content)`); the same shape for `agent_prompt.version_restored` including `restored_from` in `details`. Then: two different seeded users each save once (against the same tenant, sequentially, respecting `baseVersion`), and assert the history list's `createdBy` for each of their two versions matches their respective `display_name`.

- [X] T070 [US5] Extend `backend/crates/server/tests/ai_agent_prompt.rs` with a test proving `created_by_display` is a point-in-time snapshot, not a live join: save a version as a given principal, then directly `UPDATE users SET display_name = 'Changed Name' WHERE id = $1` for that principal in the test, then re-`GET` that version's detail (and the history list) and assert `createdBy` still shows the **original** display name, not `'Changed Name'` — this is the executable proof of spec US5 scenario 2 ("author visible even after the account changes/is deactivated").

- [X] T071 [US5] Confirm `backend/crates/shared/db/tests/schema.rs`'s T003 append-only assertions (raw `UPDATE`/`DELETE` against `agent_prompt_versions` both fail) are present and passing — if T003 already covered this, this task is a no-op verification; if it was skipped or missed, add it now. This is the database-level backstop for FR-003/FR-014's "unalterable" audit-trail requirement, independent of any application-code discipline.

**Checkpoint**: Every FR-014/FR-002/SC-003 auditability guarantee in spec.md has a direct, named, executable test — not incidental coverage.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [X] T072 [P] Run `cd backend && cargo fmt --all --check && cargo clippy --workspace -- -D warnings`; fix every warning surfaced (do not `#[allow(...)]` your way past them unless an existing, identical pattern elsewhere in this codebase already does so for the same lint).

- [X] T073 [P] Run `cd backend && cargo test --workspace` (unit tests, no DB needed) and then `REQUIRE_DB_TESTS=1 DATABASE_URL=<local test db> cargo test --workspace` (full integration suite including the new `ai_agent_prompt.rs`, and the extended `ai_agent.rs`/`rbac.rs`/`openapi_contract.rs`/`schema.rs`). Fix any failure — do not mark this task done with red tests.

- [X] T074 [P] Run `cd frontend && pnpm nx run-many -t lint test build --projects=dashboard`. Fix any failure.

- [X] T075 Walk through every numbered step in [quickstart.md](./quickstart.md)'s "Manual walkthrough" section (steps 1-15) against a locally running backend (`cargo run -p server`) and frontend (`pnpm start`). Any deviation from the described behavior maps back to a spec FR/SC — fix it before considering 018 done. This is the final end-to-end sign-off; T072-T074 are necessary but not sufficient.

- [X] T076 Update the module-level `//!` doc comment at the top of `backend/crates/modules/ai/src/lib.rs` (lines 1-46) to mention `prompt_store`, `prompt_validate`, `prompt_routes`, and migration 0045's two tables — follow the file's existing documentation style (Purpose/Responsibilities/Public Interfaces/Dependencies/Data Model/Extension Points sections) rather than just appending a note.

- [X] T077 [P] Grep for stragglers: `grep -rn "system_prompt\|systemPrompt" backend/crates frontend/apps/dashboard/src`. Every remaining hit must be either (a) inside `agent_prompt_versions`/the new prompt feature's own code (expected — e.g. `prompt_store.rs`'s SQL doesn't reference this string, but comments explaining the 018 migration might), or (b) a genuine leftover that needs removing. Confirm there is no code path anywhere that still reads or writes `agent_configurations.system_prompt` (the column no longer exists after T002, so any surviving reference is a compile error you'd have already caught — this task is really about doc comments, test fixture names, and variable names that are now misleading even though they compile).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001)**: No dependencies.
- **Foundational (T002-T010)**: Depends on T001. Strictly blocks every user story — in particular, T002 (migration) must land before T004 (which fixes the resulting compile error), and T004 must land before anything in Phase 3+ touches `agent_config.rs` or the router.
- **User Stories (Phase 3-7)**: All depend on Foundational (T002-T010) being complete.
  - **US1 (Phase 3)** has no dependency on any other user story — it's the true MVP.
  - **US2 (Phase 4)** reuses `prompt_store::save_version_in_tx` (built in Foundational, wired for PUT in US1) for restore — depends on Foundational only, not on US1's *tasks*, though in practice US1's router/lib.rs edits (T014, T015) will already be in the file US2 extends, so implement US1 before US2 even though they're not logically coupled.
  - **US3 (Phase 5)** is frontend-only and depends on US1's `prompt-page.component.ts`/`prompt.store.ts` skeleton (T024, T025) existing to extend. Backend catalog data it needs (`VARIABLES`) already exists from Foundational (T006) and is returned by US1's GET (T012).
  - **US4 (Phase 6)** depends on US1's save/PUT (T013) and US2's restore (T041) already calling `validate_prompt` — it adds *coverage and inline UX* around validation that Foundational/US1/US2 already enforce authoritatively.
  - **US5 (Phase 7)** depends on US1's audit call (T013/T017 → T010's helpers) and US2's audit call (T041) already existing — it adds *dedicated proof*, not new enforcement.
- **Polish (Phase 8)**: Depends on all desired user stories being complete.

### Within Each Phase

- Backend DTOs/handlers before router mounting before integration tests (a test can't exercise a route that isn't mounted).
- `lib.rs` `pub mod` edits (T007, T009, T014) must land in the same or an earlier commit than anything that references that module.
- Frontend: models (T022/T046) before API service (T023/T047) before store (T024/T048) before components (T025/T049) before that component's spec file.

### Parallel Opportunities

- T003, T005, T010 (Foundational) can run in parallel with each other but all still gate on T002/T004/T008 respectively as noted by their `[P]`/dependency annotations.
- Within US1, the backend track (T011-T021) and the frontend track (T022-T037) are independent of each other except that the frontend DTOs (T022) should match whatever shape the backend DTOs (T011) actually end up with — in practice, finish T011-T015 (or at least freeze the JSON shape per the contract) before starting T022, even though no compiler enforces this ordering across languages.
- T033, T034, T035 (US1 frontend specs) are mutually parallel.
- T051, T052, T053 (US2 frontend specs) are mutually parallel.
- T057, T061, T062 (US3 frontend specs) are mutually parallel; T056/T057 both depend on T055 but not on each other.
- T072, T073, T074 (Polish gates) are mutually parallel.

---

## Parallel Example: Foundational Phase

```bash
# After T002 (migration) and T004 (agent_config.rs compile fix) land sequentially:
Task: "shared/db/tests/schema.rs migration_0045 assertions"       # T003
Task: "server/tests/ai_agent.rs systemPrompt cleanup"              # T005
Task: "agent_audit.rs prompt audit helpers"                        # T010
```

## Parallel Example: User Story 1 frontend specs

```bash
Task: "prompt-api.service.spec.ts"      # T033
Task: "prompt.store.spec.ts"            # T034
Task: "prompt-page.component.spec.ts"   # T035
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. T001 (Setup).
2. T002-T010 (Foundational) — schema, validation, data layer, audit helpers.
3. T011-T037 (US1) — save/version/conflict/no-op, responder binding, the new prompt page, settings-card replacement.
4. **STOP and VALIDATE**: run the US1-relevant subset of quickstart.md steps 1-4. This alone is a shippable safety improvement over 017's silent-overwrite behavior.

### Incremental Delivery

1. Setup + Foundational → foundation ready (nothing user-visible yet).
2. + US1 → save is now versioned, safe, and conflict-aware (MVP).
3. + US2 → history + restore (the other half of "safe prompt management" — spec.md treats US1+US2 as co-critical).
4. + US3 → variables + live preview (maintainability/trust layer).
5. + US4 → validation hardening (prevents the "typo reaches customers" failure mode explicitly).
6. + US5 → dedicated audit proof (compliance sign-off).
7. Polish → gates green, quickstart walked end-to-end, docs updated.

### Notes

- [P] tasks touch different files with no ordering dependency on each other within their phase.
- Every task names its exact file path(s); where this codebase already has an equivalent pattern, the task says which existing file/lines to mirror — open that file before writing new code, don't improvise a different structure.
- Commit after each task or small logical group. Stop at any phase checkpoint to sanity-check before continuing.
- The two things most likely to break silently across this whole feature: (1) forgetting that `save_version_in_tx` is shared between PUT and restore — do not write two copies of the conflict/no-op logic; (2) forgetting that `ApiError::conflict()`/`ApiError::unprocessable_entity()` always emit fixed `code` strings (`"conflict"`/`"validation_failed"`) — the contract's `version_conflict`/`validation_failed` labels describe the *situation*, and structured details (`.with_details(...)`) carry the situation-specific data, exactly like `escalations/src/routes.rs` and `tenancy/src/invitations.rs` already do for their own 409s.

---

## Phase 9: Convergence

**Purpose**: Close the gaps between the implemented code and what spec.md / plan.md / the contracts actually require. Appended by `/speckit-converge` after assessing the built code; each task names the artifact that requires it and the evidence it is currently unmet. Every item below was verified absent or broken **in the code** — none duplicates work already done. Phases 1–8 are all marked complete, but these seven obligations were reported done without being met, so re-check the evidence line before assuming any of them is a no-op.

**Verified already done — do NOT redo**: migration 0045 (tables, CHECKs, uniques, backfill, `DROP COLUMN system_prompt`, append-only trigger); all 5 endpoints + router permission wiring; `save_version_in_tx` shared by PUT and restore with conflict/no-op; the responder's active-content read, runtime variable rendering, and `prompt_version` trace; `STARTER_PROMPT` + its validity test; the `dialog-shell` `drawer-right` variant; and 26 integration tests in `ai_agent_prompt.rs` (both RBAC role tests, audit matrix, snapshot attribution, restore-blocked-by-validation). Backend compiles; 56 `ai` unit tests pass.

- [X] T078 Implement the historical-vs-active diff per FR-005 / US2 AC2 (missing). The endpoint and store already load a historical version's full content (`prompt.store.ts` `selectedVersion`), but **nothing compares it to the active content** — `grep -riE "diff|differ|compare"` across all 14 files of `frontend/apps/dashboard/src/app/features/tenant/ai-agent/prompt/` returns no implementation, and `version-history-drawer.component.ts` renders the selected version's text alone. Add a pure `export function diffLines(previous: string, current: string): { kind: 'same' | 'added' | 'removed'; text: string }[]` to `prompt-lang.ts` (a naive line-by-line common-prefix/suffix walk or small LCS is sufficient — no diff library, `package.json` has none and this must stay a pure function per Constitution IV). Render it in the drawer's selected-version view, comparing `store.selectedVersion()!.content` against the active content (`store.bootstrap()!.prompt.content`), with added/removed lines visually distinguished (not colour alone — Principle IX's accessibility bar). Spec.md US2 AC2 requires the reader "**see how it differs from the currently active version**". Add a diff case to `version-history-drawer.component.spec.ts` and a `diffLines` table test to `prompt-lang.spec.ts`.

- [X] T079 Repair the shared validation fixture and actually wire both suites to it, per plan.md's R5 mirror-drift guarantee (contradicts). `specs/018-prompt-management/contracts/prompt-validation-fixture.json` **is not valid JSON**: line 5 contains the JavaScript expression `"X".repeat(8001)`, so `serde_json::from_str` / `JSON.parse` / `json.load` all fail on it (verify: `python3 -c "import json;json.load(open('specs/018-prompt-management/contracts/prompt-validation-fixture.json'))"` → `JSONDecodeError`). Neither side consumes it either — `grep -c include_str backend/crates/modules/ai/src/prompt_validate.rs` → `0`, and `prompt-lang.spec.ts` has no fixture import. Both suites currently hand-roll parallel test lists that agree only by coincidence, which is exactly the drift R5 exists to prevent. (a) Make the file valid JSON — replace the `.repeat(8001)` case with a declarative form both loaders expand, e.g. `{ "contentRepeat": { "unit": "X", "times": 8001 }, "expected": [{ "code": "too_long" }] }`. (b) Add a fixture-driven test to `prompt_validate.rs`'s `#[cfg(test)]` block that `include_str!`s the file, parses it, and asserts `validate_prompt` returns exactly the expected codes for every case. (c) Mirror it in `prompt-lang.spec.ts` per T057's documented fallback (copy into the feature dir with a cross-reference comment if the bundler rejects the out-of-tree path). Land T080's case in the same pass.

- [X] T080 Align the TS validator's length semantics with the Rust authority per plan.md R5 (partial). `prompt_validate.rs:70` measures `content.chars().count()` (Unicode scalar values); `prompt-lang.ts:95` measures `content.length` (UTF-16 code units). They disagree for every astral-plane character: 4,001 emoji is 8,002 UTF-16 units (client reports `too_long` and disables Save) but 4,001 scalars (server would accept) — the editor blocks a save the server allows, and the fixture that should have caught this is broken (T079). Change `prompt-lang.ts`'s `too_long` check to `[...content].length > MAX_CONTENT_LENGTH` (code-point count, matching Rust and the DB's `char_length`), and add an astral-plane boundary case to T079's fixture so this class of drift fails a test in future rather than reaching a user.

- [X] T081 Emit structured save/restore events per plan.md's Constitution VI commitment (missing). plan.md's Constitution VI row promises "save/restore handlers emit structured events (action, version numbers, latency) with request-id", but `prompt_routes.rs` contains only `tracing::error!` on failure paths — there is no success-path event on either mutation (`grep -n "tracing::" backend/crates/modules/ai/src/prompt_routes.rs` → all 13 hits are `error!`). The responder half **is** already done (`agent_responder.rs` emits `tracing::info!(prompt_version, "agent responder: reply sent")`) — do not touch it. Add a `tracing::info!` on the success path of `put_prompt` and `restore_prompt_version` carrying the action (`agent_prompt.version_created` / `agent_prompt.version_restored`), the new version number, `restored_from` where applicable, elapsed duration, and `ctx.request_id`. **Never log prompt content**, rendered or raw (015 invariant — the same rule the audit `details` payloads already correctly follow).

- [X] T082 Extend the composer's determinism tests to cover rendered input, per plan.md:19 ("composer byte-equality over rendered input") and prompt-runtime.md:44 ("its byte-equality determinism tests extend to cover rendered input") (missing). `agent_prompt.rs` was touched only to rename the `system_prompt` parameter to `prompt_content`; its `#[cfg(test)]` block still never exercises `compose_system_message` over `render_prompt` output. End-to-end behavior *is* covered by `ai_agent_prompt.rs::responder_substitutes_prompt_variables` (a DB+wiremock integration test), so this is a gap in the committed unit-level contract rather than a behavioral risk — but both artifacts promise it, and a unit test fails faster and without a database. Add a test to `agent_prompt.rs` that renders a placeholder-bearing prompt through `prompt_validate::render_prompt`, feeds the result to `compose_system_message` twice, and asserts byte equality plus that the substituted values appear verbatim in the composed output.

- [X] T083 [P] Record audit `content_length` in characters, not bytes (partial). `prompt_routes.rs:265` (`payload.content.len()`) and `prompt_routes.rs:545` (`source_row.content.len()`) pass Rust byte counts into `agent_audit`'s `content_length` detail, while validation (`chars().count()`), the DB CHECK (`char_length`), and the API's `maxContentLength` all mean **characters** — so an audit row for any multi-byte prompt reports a length no other part of the system agrees with. Change both call sites to `.chars().count()`. No audit-row shape change, no migration.

- [X] T084 [P] Align `created_by_user_id`'s delete behavior with the documented intent (partial). `backend/migrations/0045_agent_prompts.sql:44` declares `created_by_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL`, but data-model.md documents this column as "NULL **only** for the migration backfill", and the platform's append-only precedent — `audit_logs.actor_user_id` (migration 0006) — uses `ON DELETE RESTRICT`. As written, a hard user delete would silently rewrite immutable history rows, which FR-003 forbids. Migration 0045 is untracked (`git status` → `??`), i.e. never committed or shipped, so per Constitution VIII edit it **in place** to `ON DELETE RESTRICT` rather than adding 0046 — then re-run migrations against any local dev DB that already applied the old version. Add a `shared/db/tests/schema.rs` assertion that deleting a user referenced by a version row is rejected. Attribution display is unaffected either way: `created_by_display` is a snapshot, already proven by `ai_agent_prompt.rs::created_by_is_snapshot_not_live_lookup`.

---

## Phase 10: Convergence

**Purpose**: Close the one gap found by `/speckit-converge` after re-assessing the built code against spec.md, plan.md, and contracts/. Phases 1–9 (T001–T084) were re-verified against the code, not trusted from their checkboxes — every requirement they claim is implemented **is** implemented. Only genuinely-unbuilt work is listed here.

**Verified already done — do NOT redo**: migration 0045 (both tables, CHECKs, uniques, backfill, `DROP COLUMN system_prompt` — FR-018 is enforced by schema: `grep -rn "system_prompt" backend/crates --include=*.rs` returns only a doc comment, and `systemPrompt` is absent from the whole dashboard); all 5 endpoints + router `view`/`manage` wiring; `save_version_in_tx` (FOR UPDATE lock, `baseVersion` conflict → 409, byte-compare no-op, unique-violation race → 409) shared by PUT and restore; cursor-paginated history + `hasMore`; version detail + `isActive`; restore re-validation → 422 and `restoredFrom`; responder active-content read + runtime variable rendering + `prompt_version` trace; `STARTER_PROMPT`; audit for both actions with char-counted `content_length`; the FR-005 diff (`prompt-lang.ts::diffLines` rendered in `version-history-drawer.component.ts`); preview error chips; insert-at-cursor; the settings summary card; 26 integration tests covering the conflict/no-op/RBAC/isolation/audit/responder matrix. `rbac.rs` is deliberately **not** extended (contracts/rest-api.md:106 — `TENANT_OPERATIONS` is positionally `.zip()`ed against fixed-length arrays, so appending drops coverage silently); plan.md:98's "rbac.rs MODIFIED" line is stale against that decision — do not act on it.

- [X] T085 Validate `changeNote` length server-side in `backend/crates/modules/ai/src/prompt_validate.rs` and reject over-limit notes with a 422 instead of a 500 per Constitution V (API-First & Contract Consistency — "standard envelope/error vocabulary") and contracts/rest-api.md:30, which advertises `limits.maxChangeNoteLength: 500` to every client (partial). `MAX_CHANGE_NOTE_LENGTH` is defined (`prompt_validate.rs:34`) and surfaced through `LimitsDto` (`prompt_routes.rs:191`), but **nothing between the client and Postgres enforces it**: `validate_prompt(content)` takes only the content, both `put_prompt` (`prompt_routes.rs:225`) and `restore_prompt_version` (`prompt_routes.rs:511`) validate content alone, and `save_version_in_tx` binds `change_note` straight into the INSERT (`prompt_store.rs:176`). A 501-character `changeNote` therefore trips the DB CHECK `char_length(change_note) <= 500` (`migrations/0045_agent_prompts.sql:42`), which is not a unique violation, so it falls through to `SaveError::Db` → `ApiError::internal_error` → **500 on what is plainly a client input error**. The dashboard's `[maxLength]` binding (`prompt-page.component.ts:133`) is presentation-only and does not protect the API. Note this is *not* an FR-010 item — FR-010's rejection list is content-only — so scope the fix accordingly: (a) add a change-note length check emitting a `ValidationIssue { field: "changeNote", code: "invalid_length", … }`, mirroring 017's server-side string-length precedent in `agent_config.rs::validate_payload` (which uses `invalid_length` for exactly this class of field) and measuring in **characters** (`chars().count()`) to stay consistent with T080/T083's char-not-byte semantics and with the DB's `char_length`; wire it into both handlers so restore is covered too. (b) Add a unit test in `prompt_validate.rs` (note of exactly 500 chars passes, 501 fails) and an integration test in `backend/crates/server/tests/ai_agent_prompt.rs` asserting a 501-char `changeNote` returns **422 with the `changeNote` field named in details** — the file currently has no change-note test at all (`grep -in "change_note\|changeNote" backend/crates/server/tests/ai_agent_prompt.rs` → no hits). Do not add the note to `prompt-lang.ts`'s validator: that mirror covers prompt content only, and the editor already caps the field at the limit.
