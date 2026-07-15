# Data Model: AI Provider Abstraction

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

Three new tables, all owned by the `ai` module crate. Migrations `0038`–`0040`. Conventions per 005 (UUID v7 PKs, `created_at`/`updated_at` TIMESTAMPTZ, `set_updated_at` trigger, soft delete via `deleted_at`) except where a deviation is recorded in plan.md Complexity Tracking.

## Provider catalog (no table)

The AI Provider entity (spec Key Entities) is a **fixed code catalog**, not a table: `ProviderKind ∈ {openai, anthropic, gemini}` with a `supports_streaming` capability flag (all true today). The DB mirrors it as CHECK constraints so rows can never reference an unknown vendor. Adding a provider is a code change (adapter + enum + CHECK migration) by design — FR-003.

## `ai_configurations` (0038)

One live row per scope: the platform default (`tenant_id IS NULL`) or a tenant override.

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID NULL | FK → `tenants(id)` | NULL = platform default scope (justified deviation, plan.md) |
| `provider` | TEXT | NOT NULL, CHECK in (`'openai'`,`'anthropic'`,`'gemini'`) | primary provider |
| `model` | TEXT | NOT NULL, non-empty | vendor model identifier, admin-supplied |
| `max_output_tokens` | INTEGER NULL | CHECK > 0 | NULL = vendor default |
| `temperature` | REAL NULL | CHECK 0 ≤ t ≤ 2 | NULL = vendor default |
| `fallbacks` | JSONB | NOT NULL DEFAULT `'[]'` | ordered array of `{"provider": …, "model": …}`; validated in code: ≤ 3 entries, catalog providers, no entry equal to the primary, no duplicates |
| `capture_content` | BOOLEAN | NOT NULL DEFAULT false | FR-018; meaningful only on tenant rows — resolution ignores the platform row's flag |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | `set_updated_at` trigger |
| `deleted_at` | TIMESTAMPTZ NULL | | deleting a tenant override reverts the tenant to the platform default |

**Indexes**
- `UNIQUE (tenant_id) WHERE tenant_id IS NOT NULL AND deleted_at IS NULL` — one live override per tenant.
- `UNIQUE ((true)) WHERE tenant_id IS NULL AND deleted_at IS NULL` — at most one live platform default.

**State transitions**: created → updated (in place, audited) → soft-deleted (audited). In-flight requests keep the row they resolved (spec edge case: config changes apply from the next request — resolution reads once per request).

## `ai_credentials` (0039)

One live key per (scope, provider). Platform scope = the default key for that vendor; tenant scope = BYOK, which fully replaces the platform key for that tenant (FR-004).

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID NULL | FK → `tenants(id)` | NULL = platform key |
| `provider` | TEXT | NOT NULL, CHECK in catalog | |
| `ciphertext` | BYTEA | NOT NULL | AES-256-GCM; AAD = scope ∥ provider (research R5) |
| `nonce` | BYTEA | NOT NULL | 12 random bytes per row/rotation |
| `key_hint` | TEXT | NOT NULL | last 4 chars of the plaintext, the only thing any read path returns (FR-008) |
| `created_at` / `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | trigger |
| `deleted_at` | TIMESTAMPTZ NULL | | deletion audited; next call falls back per resolution rules |

**Indexes**
- `UNIQUE (tenant_id, provider) WHERE tenant_id IS NOT NULL AND deleted_at IS NULL`
- `UNIQUE (provider) WHERE tenant_id IS NULL AND deleted_at IS NULL`

**Rotation** = UPDATE of `ciphertext`/`nonce`/`key_hint` in place (audited as `ai_credential.set`, hint only in the audit payload); effective next request because keys are resolved per call.

## `ai_usage_records` (0040)

Append-only ledger; one row per AI call that reached any vendor. No `updated_at`/`deleted_at` (justified deviation, plan.md).

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | UUID | PK | v7 |
| `tenant_id` | UUID | NOT NULL, FK → `tenants(id)` | isolation (FR-011) |
| `provider` | TEXT | NOT NULL, CHECK in catalog | the provider that **actually served** (or last attempted) the call — FR-017 attribution |
| `model` | TEXT | NOT NULL | ditto |
| `input_tokens` | INTEGER NULL | CHECK ≥ 0 | NULL = vendor did not report (never coerced to 0 — spec edge case) |
| `output_tokens` | INTEGER NULL | CHECK ≥ 0 | ditto; partial streams record what was reported |
| `status` | TEXT | NOT NULL, CHECK in (`'success'`,`'failure'`) | FR-010 |
| `error_category` | TEXT NULL | CHECK in (`'authentication'`,`'rate_limited'`,`'unavailable'`,`'timeout'`,`'invalid_request'`) | set iff `status = 'failure'` (FR-012 vocabulary) |
| `streamed` | BOOLEAN | NOT NULL | streamed vs blocking call |
| `latency_ms` | INTEGER | NOT NULL | wall-clock incl. retries/failover |
| `request_id` | TEXT NULL | | observability correlation (FR-015) |
| `request_content` | JSONB NULL | | messages as passed by the caller; populated only under tenant opt-in (FR-018) |
| `response_content` | TEXT NULL | | reply (or partial reply for interrupted streams); same gate |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

**Indexes**
- `(tenant_id, created_at DESC)` — the production query path: period listing, summary aggregation, cursor pagination.

**Invariants**
- A call rejected before any vendor attempt (NotConfigured, validation) writes **no** row.
- A call that exhausted retries and all fallbacks writes exactly **one** row: last provider attempted, `failure`, its normalized category.
- `request_content`/`response_content` are non-NULL only when the serving tenant's resolved `capture_content` was true at call time.

## Resolution (derived, not stored)

Per request, two independent lookups (FR-004):

1. **Configuration**: tenant's live `ai_configurations` row, else the live platform-default row, else → `NotConfigured` error (no vendor call, no usage row).
2. **Credential** (per provider being attempted, incl. each fallback): tenant's live `ai_credentials` row for that provider (BYOK), else the platform row for that provider, else → `NotConfigured` for that attempt (a fallback without any resolvable key is skipped with an observable trace event; if no attempt has a key, the caller gets `NotConfigured`).

## Audit actions (existing `audit_logs` table, via `tenancy::audit::record_in_tx`)

| Action | Resource | Payload (never secret material) |
|---|---|---|
| `ai_config.updated` | `ai_configuration` | scope, provider, model, params, fallbacks, changed-field names |
| `ai_config.deleted` | `ai_configuration` | scope |
| `ai_config.capture_content_changed` | `ai_configuration` | old/new flag (FR-018: toggle audited) |
| `ai_credential.set` | `ai_credential` | scope, provider, key_hint (set and rotate share the action; payload marks `rotated: bool`) |
| `ai_credential.deleted` | `ai_credential` | scope, provider |
