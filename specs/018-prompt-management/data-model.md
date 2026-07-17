# Data Model: Prompt Management

**Feature**: 018-prompt-management | **Date**: 2026-07-16

One migration (`0045_agent_prompts.sql`): two new tables owned by `modules/ai`, a backfill, and a column drop on `agent_configurations`. Conventions per 005 (UUID v7 PKs, TIMESTAMPTZ, `set_updated_at` trigger where rows mutate, partial unique indexes); the versions table follows the `audit_logs` append-only precedent instead (no `updated_at`, no `deleted_at`, no trigger).

## Catalogs (code constants in `modules/ai`, not stored)

- **Prompt variables** (R4): `agent_name`, `tenant_name`, `customer_name`, `channel` — each with description, preview sample, runtime source, and deterministic runtime fallback. Exposed via `GET /tenant/ai/agent/prompt`; enforced by `validate_prompt`.
- **Placeholder syntax** (R4/R5): `{{snake_case_name}}`; single braces are literal prose; `{{`/`}}` sequences enter placeholder lexing.
- **Limits**: content 1–8000 chars (017's limit carried forward), change note ≤ 500 chars.
- **Starter default prompt**: `prompt_validate::STARTER_PROMPT` — a catalog-valid constant introduced by *this* feature (017 has none; its `system_prompt` column simply defaulted to `''`). Shown as the editable baseline when no prompt row exists (spec edge case); becomes stored content only on first save. A unit test asserts it passes `validate_prompt`, so a future catalog change can never ship an invalid baseline.

## `agent_prompts` (0045)

The tenant's managed prompt object (spec entity **Prompt**). One live row per tenant in v1; `prompt_kind` is the future multi-prompt discriminator, and the multi-agent unlock is an additive migration (add nullable `agent_id`, backfill, extend the unique index) — no redesign (R3). Created lazily on first successful save; may exist before or after the tenant's `agent_configurations` row (R10).

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` | Constitution II |
| `prompt_kind` | TEXT | NOT NULL DEFAULT `'system'`, CHECK in (`'system'`) | future kinds extend the CHECK |
| `active_version` | INTEGER | NOT NULL, CHECK `> 0` | always the highest `version_number` in v1 (save = activate); updated in the same tx as each version insert (R6) |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | `set_updated_at` trigger |
| `deleted_at` | TIMESTAMPTZ NULL | | convention only; no v1 delete path |

**Indexes**: `UNIQUE (tenant_id, prompt_kind) WHERE deleted_at IS NULL` — the v1 one-prompt cap.

**State transitions**: absent (editor shows starter default, `activeVersion = 0` in the API) → created with `active_version = 1` on first save → `active_version` increments per save/restore. No delete, no rollback of the pointer (restore rolls forward, R11).

## `agent_prompt_versions` (0045)

Immutable content snapshots (spec entity **Prompt Version**). Append-only: INSERT is the only statement any code path issues against this table.

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` | isolation on every read path without a join |
| `prompt_id` | UUID | NOT NULL, FK → `agent_prompts(id)` | |
| `version_number` | INTEGER | NOT NULL, CHECK `> 0` | dense, monotonic per prompt; assigned `baseVersion + 1` under the parent-row lock |
| `content` | TEXT | NOT NULL, CHECK `char_length(content) BETWEEN 1 AND 8000` | placeholders stored raw (never sample-substituted); non-empty per FR-010 |
| `change_note` | TEXT NULL | CHECK `char_length(change_note) <= 500` | optional user note (FR-002) |
| `restored_from` | INTEGER NULL | | source `version_number` when created by restore (R11) |
| `created_by_user_id` | UUID NULL | FK → `users(id)` | NULL only for the migration backfill |
| `created_by_display` | TEXT | NOT NULL | display-name snapshot at save time — survives deactivation (R8, US5 scenario 2) |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | append-only: no `updated_at` / `deleted_at` / trigger |

**Indexes**: `UNIQUE (prompt_id, version_number)` — race-safe version assignment (R6) and the history cursor's access path (R7; scanned descending with `tenant_id` + `prompt_id` equality).

**Validation (code, `validate_prompt` — R5)**: trimmed-non-empty; ≤ 8000 chars; well-formed placeholders (unclosed `{{`, stray `}}`, empty/ill-formed names → `malformed_placeholder` with offset); every placeholder name in the variables catalog (`unknown_variable` with name + offset). Runs on save **and** restore. Backfilled version-1 content is exempt at rest (historical); it re-validates like anything else on the next save/restore.

## `agent_configurations` change (0045)

```sql
-- after backfill:
ALTER TABLE agent_configurations DROP COLUMN system_prompt;
```

**Backfill (same migration, before the drop)**: for each live `agent_configurations` row with `system_prompt <> ''`, insert an `agent_prompts` row (`active_version = 1`) and a version-1 snapshot (`created_by_user_id NULL`, `created_by_display 'Migration backfill'`, `change_note NULL`). Rows with an empty prompt get no prompt row — their first real save creates version 1 (matches the unconfigured-tenant edge case).

Effects on 017 surfaces (R2/R10): `AgentConfigPayload` and agent DTOs lose `systemPrompt`; agent GET gains read-only `activePrompt` summary (`version`, `updatedAt`, `updatedBy`, `excerpt`, or `null`); `create_in_tx`/`update_in_tx` drop the prompt bind; the responder loads active content via the query below; the platform-persona branch keeps passing empty content.

## Active-content read (responder hot path)

```sql
SELECT v.content
FROM agent_prompts p
JOIN agent_prompt_versions v
  ON v.prompt_id = p.id AND v.version_number = p.active_version
WHERE p.tenant_id = $1 AND p.prompt_kind = 'system' AND p.deleted_at IS NULL
```

One indexed single-row read per responder run (partial unique on the parent + unique on the versions side); absent row ⇒ compose with empty prompt content, exactly 017's empty-prompt behavior.

## Audit actions (existing `audit_logs`, via `tenancy::audit::record_in_tx`, same tx as the write)

| Action | Resource | Details payload (R8 — never prompt content) |
|---|---|---|
| `agent_prompt.version_created` | `agent_prompt` | prompt id, new version number, content length, change-note presence |
| `agent_prompt.version_restored` | `agent_prompt` | prompt id, new version number, `restored_from`, content length |

No-op saves (FR-013) write nothing — no version, no audit row.

## Relationships

```text
tenants 1 ─── 0..1 agent_prompts (v1 cap via partial unique)          [modules/ai]
agent_prompts 1 ─── * agent_prompt_versions (append-only)             [modules/ai]
agent_prompts.active_version ──> agent_prompt_versions.version_number (same-tx invariant)
agent_prompt_versions.created_by_user_id ──> users (nullable; display snapshot is authoritative for rendering)
agent responder ──> active-content read ──> variable substitution (R4) ──> compose_system_message
agent_configurations ✂ system_prompt (dropped; decoupled from the prompt object)
```
