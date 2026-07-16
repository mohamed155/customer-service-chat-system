# REST API Contract: AI Agent Configuration

**Feature**: 017-ai-agent-config | **Date**: 2026-07-16

All endpoints: standard envelope (`{"data": …}` / `{"error": {code, message, details?}}`), `X-Tenant-ID` tenant context, deny-by-default `.guarded()` routing. Cross-tenant access answers `not_found`. Mounted under `mount_tenant`.

## Permissions (no new codes)

| Route | Permission |
|---|---|
| `GET /tenant/ai/agent` | `ai_agent.view` |
| `GET /tenant/ai/agent/options` | `ai_agent.view` |
| `PUT /tenant/ai/agent` | `ai_agent.manage` |
| `PUT /tenant/ai/agent/avatar` | `ai_agent.manage` |
| `GET /tenant/ai/agent/avatar` | `ai_agent.view` |
| `POST /tenant/conversations/{id}/ai-handling` | `conversations.manage` |

Matrix change (R11): `ai_agent.view`/`ai_agent.manage` removed from Manager; `ai_agent.view` removed from Viewer and platform staff Developer tenant-context set. Owner/Admin keep both. This also narrows the 015 AI provider/usage routes guarded by the same codes — intended (spec FR-013).

## `GET /tenant/ai/agent`

Returns the agent configuration, or editable platform defaults when never configured.

**200 data**:

```json
{
  "configured": true,
  "agent": {
    "id": "uuid | null when configured=false",
    "name": "Ava",
    "is_default": true,
    "avatar": { "kind": "preset", "preset": "spark", "upload_url": null },
    "tone": "professional",
    "system_prompt": "…",
    "business_rules": ["…"],
    "escalation_rules": [
      {
        "id": "uuid",
        "name": "Refund requests",
        "trigger": "topic_keywords",
        "keywords": ["refund", "chargeback"],
        "required_skill_ids": ["uuid"],
        "broken_skill_refs": ["uuid — subset of required_skill_ids no longer live"]
      }
    ],
    "enabled_channels": ["web_chat"],
    "provider_selection": {
      "provider": "anthropic | null",
      "model": "claude-sonnet-5 | null",
      "stale": false
    },
    "version": 3,
    "updated_at": "… | null when configured=false"
  }
}
```

- `configured=false`: `agent` carries the platform default template (generic name, default preset avatar, `professional` tone, starter prompt, empty rules, `enabled_channels: ["web_chat"]`, null provider override, `version: null`). Never a 404 (US1 scenario 1).
- `avatar.upload_url` = `/tenant/ai/agent/avatar` when `kind=upload`, else null.
- `provider_selection.stale=true` when the override's provider has no resolvable credential (FR-008 surfacing); `stale` is always false for a null override.
- `broken_skill_refs` computed on read (US3 scenario 4).

## `PUT /tenant/ai/agent`

Full-replace upsert (idempotent). First save creates the row (agent becomes active — clarification #2) and returns `201`; later saves return `200`.

**Request**:

```json
{
  "name": "Ava",
  "avatar": { "kind": "preset", "preset": "spark" },
  "tone": "professional",
  "system_prompt": "…",
  "business_rules": ["…"],
  "escalation_rules": [ { "id": "uuid | omitted for new", "name": "…", "trigger": "…", "keywords": [], "required_skill_ids": [] } ],
  "enabled_channels": ["web_chat"],
  "provider_selection": { "provider": "anthropic", "model": "claude-sonnet-5" },
  "version": 3
}
```

- `version` **required** when the agent exists; must match the live row else `409 conflict` ("configuration changed since it was loaded" — FR-017). Omitted/null only on first save; if a row already exists then, also `409`.
- `avatar.kind = "upload"` is only accepted when a live upload exists (otherwise `422 validation_failed`); switching to `preset` soft-deletes the upload row.
- `provider_selection` nullable as a whole (null = follow AI-layer default). Provider must be in the catalog; a provider without resolvable credential → `422` (selector never offered it — FR-007).
- Validation failures → `422 validation_failed` with per-field `details` (FR-005, atomic — no partial write).
- `required_skill_ids` must all exist live in the tenant at save time → else `422` naming the offending rule.
- Writes audit `agent_config.created` / `agent_config.updated` in the same transaction (FR-014).

**Responses**: `200/201` with the same body as `GET` (fresh `version`); `409 conflict`; `422 validation_failed`; `403 forbidden`.

## `PUT /tenant/ai/agent/avatar`

Upload a custom avatar. Body: raw image bytes, `Content-Type: image/png|image/jpeg|image/webp`, ≤ 256 KB (else `422`/`413`). Requires the agent to exist (`404 not_found` otherwise — upload is an edit, not a first save). Transactionally upserts `agent_avatar_uploads` and sets `avatar_kind='upload'`; bumps `version`; audits `agent_config.avatar_updated`. **200 data**: the `avatar` object plus new `version`. A failed upload changes nothing (spec edge case).

## `GET /tenant/ai/agent/avatar`

Serves the live uploaded avatar bytes with its stored content type (`Cache-Control: private, max-age=300`). `404 not_found` when the agent has no live upload.

## `GET /tenant/ai/agent/options`

Selector inputs; everything the settings page needs beyond the config itself.

**200 data**:

```json
{
  "tones": ["professional", "friendly", "casual", "formal", "empathetic"],
  "channels": ["email", "phone", "web_chat", "whatsapp", "telegram"],
  "avatar_presets": ["spark", "orbit", "…"],
  "providers": [
    {
      "provider": "anthropic",
      "credential_available": true,
      "models": ["claude-sonnet-5", "…"]
    }
  ],
  "ai_layer_default": { "provider": "openai | null", "model": "gpt-… | null" },
  "prompt_max_length": 8000,
  "limits": { "business_rules_max": 20, "escalation_rules_max": 20 }
}
```

- `credential_available` via 015 credential resolution (tenant BYOK else platform key).
- `ai_layer_default` null/null when the tenant has no resolvable AI-layer configuration — the frontend shows "AI layer not configured" next to the "follow default" choice.

## `POST /tenant/conversations/{id}/ai-handling`

Per-conversation fallback decision while the tenant's agent is unconfigured (FR-004b, R13). Guarded by `conversations.manage` (this is conversation handling, not AI settings) but implemented as an `ai`-crate route handler — `ai` is the only crate that reaches both `conversations` and `escalations` without a dependency cycle (see tasks.md T046).

**Request**: `{ "mode": "platform_ai" | "human" }`

- Valid only while the tenant has **no** live agent configuration (`409 conflict` otherwise — the configured agent supersedes, FR-004c) and the conversation is not `resolved`/`closed` (`409`).
- `platform_ai`: requires the AI layer to resolve for the tenant (015 config + credential) → else `422 validation_failed` with reason (US6 scenario 4). Sets `ai_handling='platform_ai'`; subsequent customer messages are answered by the platform default persona.
- `human`: sets `ai_handling='human'` and, in the same transaction, creates an escalation via the existing 014 routing entry with the fixed reason "no AI agent configured"; response includes the escalation reference.
- Repeat calls: switching `platform_ai` → `human` is allowed (escalates); `human` → `platform_ai` is `409` once an escalation exists (claim/queue state owns the conversation).
- Audits `conversation.ai_handling_set`.

**Response 200 data**: the conversation detail (existing shape) — which now carries `ai_handling: "platform_ai" | "human" | null` and derived `awaiting_ai_decision: bool` (true iff tenant unconfigured ∧ `ai_handling` null ∧ auto-ack sent). These two fields are additive to the conversation detail/inbox DTOs for the dashboard banner.

## Error vocabulary

Standard codes only: `validation_failed` (422), `conflict` (409), `forbidden` (403), `not_found` (404), `payload_too_large` (413 avatar). No new error shapes.

## OpenAPI

All six routes and their DTOs registered in the existing utoipa document (conversation detail DTO gains `ai_handling`/`awaiting_ai_decision`; message kind enum gains `ai`/`system`); `openapi_contract.rs` expectations extended.
