# REST API Contract: AI Provider Abstraction

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

All endpoints follow the platform contract from `specs/001-ai-customer-service-platform/contracts/rest-api.md`: `ApiResponse<T>` envelope, standard error envelope with `X-Request-Id`, cursor pagination on lists. Tenant routes are mounted under the tenant-context middleware (require `X-Tenant-ID`); platform routes under the platform permission middleware. Cross-tenant access answers `not_found`. Route→permission map lives in [permissions.md](./permissions.md).

Provider values everywhere: `"openai" | "anthropic" | "gemini"`.

## Shared shapes

```jsonc
// AiConfigurationView — what every config read returns
{
  "scope": "tenant" | "platform_default",     // which row this is
  "provider": "anthropic",
  "model": "claude-sonnet-5",
  "max_output_tokens": 1024,                   // nullable
  "temperature": 0.7,                          // nullable
  "fallbacks": [ { "provider": "openai", "model": "gpt-5" } ],   // ordered, possibly empty
  "capture_content": false,                    // tenant rows only; platform view omits it
  "credential": {                              // resolved credential for the PRIMARY provider, or null
    "source": "tenant" | "platform",           // BYOK vs platform key
    "provider": "anthropic",
    "key_hint": "…a4Qz"                        // masked — full key is never returned anywhere (FR-008)
  },
  "updated_at": "2026-07-15T10:00:00Z"
}
```

## Tenant surface (`/tenant/ai/…`)

### `GET /tenant/ai/config` — `ai_agent.view`
Returns the tenant's **effective** configuration: the tenant override if live, else the platform default (`scope` tells which), else `404 not_found` with code `ai_not_configured`. Credential hint reflects FR-004 resolution (BYOK first).

### `PUT /tenant/ai/config` — `ai_agent.manage`
Create-or-replace the tenant override (idempotent full replace).

```jsonc
// request
{
  "provider": "anthropic",
  "model": "claude-sonnet-5",
  "max_output_tokens": 1024,        // optional
  "temperature": 0.7,               // optional, 0–2
  "fallbacks": [ { "provider": "openai", "model": "gpt-5" } ],  // optional, ≤3, no dup of primary
  "capture_content": false          // optional, default false on create, preserved on omit? NO — full replace: omitted = false
}
```
`200` with `AiConfigurationView` (`scope: "tenant"`). Validation failures → standard `validation_failed`. Audits `ai_config.updated` (+ `ai_config.capture_content_changed` when the flag changed). Takes effect from the next AI call (FR-006).

### `DELETE /tenant/ai/config` — `ai_agent.manage`
Soft-deletes the tenant override; the tenant reverts to the platform default. `204`. Audits `ai_config.deleted`. `404` if no live override.

### `PUT /tenant/ai/credentials/{provider}` — `ai_agent.manage`
Set or rotate the tenant's BYOK key for `{provider}`.

```jsonc
{ "api_key": "sk-…" }   // write-only; never echoed
```
`200` → `{ "provider": "anthropic", "source": "tenant", "key_hint": "…a4Qz" }`. Audits `ai_credential.set`. Effective next request (FR-009).

### `DELETE /tenant/ai/credentials/{provider}` — `ai_agent.manage`
Removes the BYOK key (tenant falls back to the platform key, if any). `204`. Audits `ai_credential.deleted`. `404` if none live.

### `POST /tenant/ai/config/test` — `ai_agent.manage`
Connectivity verification (FR-014): one minimal completion through the tenant's resolved config/credential (no fallback, no retry, no usage record, nothing customer-facing).

`200` → `{ "ok": true, "provider": "anthropic", "model": "claude-sonnet-5", "latency_ms": 412 }`
`422` → `{ "ok": false, "error_category": "authentication", "detail": "…" }` (normalized categories per FR-012; never key material)
`404 ai_not_configured` when nothing resolves.

### `GET /tenant/ai/usage` — `ai_agent.view`
Cursor-paginated usage records (metadata only — never content), newest first.

Query: `from`/`to` (RFC 3339, optional), `cursor`, `limit` (default 25, max 100).

```jsonc
// item
{
  "id": "…", "provider": "anthropic", "model": "claude-sonnet-5",
  "input_tokens": 812,          // null = unreported
  "output_tokens": 344,         // null = unreported
  "status": "success",          // "success" | "failure"
  "error_category": null,
  "streamed": true,
  "latency_ms": 1893,
  "created_at": "2026-07-15T10:00:00Z"
}
```

### `GET /tenant/ai/usage/summary` — `ai_agent.view`
Period totals (SC-003): `{ "from": …, "to": …, "calls": 128, "input_tokens": 90312, "output_tokens": 41220, "unreported_calls": 2 }` (calls with any NULL count are totaled separately, never as zero).

### `GET /tenant/ai/usage/{id}` — `ai_agent.manage`
Single record **including** captured content when present (FR-018 access restriction):

```jsonc
{ …usage item fields…, "request_content": [ {"role": "user", "content": "…"} ] | null, "response_content": "…" | null }
```
`404 not_found` for other tenants' ids.

## Platform surface (`/platform/ai/…`) — all `platform.admin`

### `GET /platform/ai/config`
The platform default `AiConfigurationView` (`scope: "platform_default"`, no `capture_content`), or `404 ai_not_configured`.

### `PUT /platform/ai/config`
Create-or-replace the platform default. Same body as the tenant PUT minus `capture_content`. Audits `ai_config.updated` with `tenant_id = NULL` scope.

### `PUT /platform/ai/credentials/{provider}` / `DELETE /platform/ai/credentials/{provider}`
Set/rotate/remove the platform default key per provider. Same shapes/audits as the tenant credential routes with `source: "platform"`.

### `POST /platform/ai/config/test`
Connectivity check of the platform default with platform keys. Same responses as the tenant test route.

## Error vocabulary additions

| Code | HTTP | When |
|---|---|---|
| `ai_not_configured` | 404 | No configuration (or no resolvable credential) for the acting scope — FR-004 |
| `ai_provider_error` | 502 | Internal `AiService` consumers surface vendor failure categories; admin `test` uses 422 as above. (Not returned by the admin CRUD routes.) |

Normalized categories (FR-012), used in `test` responses, usage records, and `AiService` errors: `authentication`, `rate_limited`, `unavailable`, `timeout`, `invalid_request`.

## Explicitly not exposed

- No HTTP completion endpoint. Chat completions are consumed **in-process** via `ai::AiService` (see [provider-contract.md](./provider-contract.md)); the future AI runtime is the caller. This keeps FR-001/FR-016 enforceable and avoids shipping an unauthenticated-model-access surface this feature doesn't need.
- No plaintext key read path exists anywhere (FR-008).
