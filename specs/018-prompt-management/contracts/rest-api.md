# REST API Contract: Prompt Management

**Feature**: 018-prompt-management | **Date**: 2026-07-16

Five tenant endpoints under the existing agent prefix, mounted with the 017 `routes!().map(require_permission)` pattern. Standard platform envelope (`ApiResponse<T>` / `ErrorEnvelope`), standard error vocabulary. All routes are tenant-context routes (`X-Tenant-ID` contract, middleware-enforced isolation); cross-tenant access answers `not_found`. GETs require `ai_agent.view`; mutations require `ai_agent.manage` (Owner/Admin only after 017's matrix narrowing — no new permission codes, FR-015/R12).

Shared shapes: `ValidationIssue { field, code, message }` (017's shape, plus prompt validation codes carrying a character `offset` embedded in `message` and a machine `code` of `required | too_long | malformed_placeholder | unknown_variable`).

## 1. `GET /tenant/ai/agent/prompt`

Editor bootstrap: active prompt, variables catalog, limits — one round trip (R9).

**200 body** (`data`):

```jsonc
{
  "prompt": {
    "exists": true,            // false ⇒ no prompt row yet; content below is the starter default
    "activeVersion": 4,        // 0 when exists = false (first save sends baseVersion: 0)
    "content": "You are {{agent_name}} for {{tenant_name}}…",
    "updatedAt": "2026-07-16T10:12:00Z",   // null when exists = false
    "updatedBy": "Dana Ops"                // created_by_display snapshot; null when exists = false
  },
  "variables": [
    { "name": "agent_name", "description": "The AI agent's customer-facing name", "sample": "Aria" },
    { "name": "tenant_name", "description": "The tenant's business name", "sample": "Acme Support" },
    { "name": "customer_name", "description": "The customer's display name", "sample": "Jamie Lee" },
    { "name": "channel", "description": "The conversation's channel", "sample": "web_chat" }
  ],
  "limits": { "maxContentLength": 8000, "maxChangeNoteLength": 500 }
}
```

## 2. `PUT /tenant/ai/agent/prompt`

Save = validate + version + activate, one transaction (R2/R5/R6). Idempotent in effect: replaying the same body yields `created: false`.

**Request**: `{ "content": string, "changeNote": string | null, "baseVersion": number }`

**Responses**:
- **200** `{ "version": 5, "created": true, "updatedAt": …, "updatedBy": … }` — new version activated; audit `agent_prompt.version_created`.
- **200** `{ "version": 4, "created": false, … }` — content byte-equal to active version (FR-013): no version, no audit; message surfaced client-side as "no changes".
- **409** `conflict` — `baseVersion != active_version`: `details[0].activeVersion` carries the current version so the client can offer review-and-retry (US1 scenario 5). Also the mapping for the residual unique-index race. (Kernel's `ApiError::conflict` always emits `code: "conflict"` — there is no `version_conflict` code in the platform vocabulary; the situation is conveyed by the details payload, exactly as 017's agent-config 409 and `escalations`/`invitations` do.)
- **422** `validation_failed` — `ValidationIssue[]`; editor content is never discarded client-side (FR-011).
- **404** — tenant mismatch (isolation convention).

## 3. `GET /tenant/ai/agent/prompt/versions?limit&before`

History for the drawer, newest first (R7). `limit` 1–100 (default 25); `before` = exclusive `version_number` cursor.

**200 body** (`data`):

```jsonc
{
  "items": [
    {
      "versionNumber": 5,
      "contentPreview": "You are {{agent_name}} for {{tenant_name}}… (first 160 chars, single-line)",
      "changeNote": "Tightened refund wording",
      "restoredFrom": null,          // e.g. 2 ⇒ render "Restored from v2"
      "createdAt": "2026-07-16T10:12:00Z",
      "createdBy": "Dana Ops",
      "isActive": true
    }
  ],
  "hasMore": true
}
```

Empty history (no prompt row) ⇒ `{ "items": [], "hasMore": false }`.

## 4. `GET /tenant/ai/agent/prompt/versions/{number}`

Full immutable snapshot for viewing/diffing a historical version (US2 scenario 2; diff computed client-side against the active content).

**200 body** (`data`): `{ "versionNumber", "content", "changeNote", "restoredFrom", "createdAt", "createdBy", "isActive" }`
**404** — unknown version number or tenant mismatch.

## 5. `POST /tenant/ai/agent/prompt/versions/{number}/restore`

Roll-forward restore (R11): re-validates the source content, applies the same conflict/no-op rules as PUT, creates a new version with `restoredFrom = {number}`.

**Request**: `{ "baseVersion": number }`

**Responses**: same shape/vocabulary as PUT —
- **200** `{ "version": 6, "created": true, "restoredFrom": 2, … }` — audit `agent_prompt.version_restored`.
- **200** `{ …, "created": false }` — source content identical to active.
- **409** `conflict`; **422** `validation_failed` (e.g. source references a variable no longer in the catalog — spec US4 scenario 5); **404** unknown version / tenant mismatch.

## 017 contract changes (R10)

- `PUT /tenant/ai/agent`: request payload **removes** `systemPrompt` (unknown-field handling per existing serde policy). Prompt content is no longer writable here — FR-018.
- `GET /tenant/ai/agent`: response **removes** `systemPrompt`, **adds** `activePrompt: { version, updatedAt, updatedBy, excerpt } | null` (read-only summary for the settings card).
- OpenAPI: new paths/DTOs registered; `openapi_contract.rs` expectations updated for both the additions and the agent DTO change.

## RBAC coverage (`server/tests/ai_agent_prompt.rs`)

| Route | Permission |
|---|---|
| `GET /tenant/ai/agent/prompt` | `ai_agent.view` |
| `PUT /tenant/ai/agent/prompt` | `ai_agent.manage` |
| `GET /tenant/ai/agent/prompt/versions` | `ai_agent.view` |
| `GET /tenant/ai/agent/prompt/versions/{number}` | `ai_agent.view` |
| `POST /tenant/ai/agent/prompt/versions/{number}/restore` | `ai_agent.manage` |

Enforced by real-route role tests (manager/agent/viewer → 403, owner/admin → success), mirroring 017's `ai_agent.rs::unauthorized_roles_get_403`. The `ai_agent.*` permission codes themselves stay covered by `rbac.rs`'s existing synthetic `/test/tenant/ai/{view,manage}` routes. `rbac.rs`'s `TENANT_OPERATIONS` is deliberately **not** extended: it is positionally `.zip()`ed against fixed-length `expected: [bool; 18]` arrays, so appending silently drops coverage instead of failing — the same reason 017 registered no agent routes there.
