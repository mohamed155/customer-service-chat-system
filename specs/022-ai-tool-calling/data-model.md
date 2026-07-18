# Data Model: AI Tool Calling

**Feature**: 022-ai-tool-calling | **Migration**: `backend/migrations/0049_ai_tool_calling.sql`

Conventions per 005: UUID PKs, `created_at`/`updated_at` timestamps, soft delete via `deleted_at`, `tenant_id` on every tenant-owned table, partial unique indexes over live rows.

## Entity: `tenant_tools` (tenant-defined external tools)

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | uuid | PK | |
| `tenant_id` | uuid | NOT NULL, FK → tenants | owner; never visible cross-tenant |
| `name` | text | NOT NULL | AI-facing identifier, `^[a-z][a-z0-9_]{2,63}$` |
| `description` | text | NOT NULL | AI-facing purpose text |
| `input_schema` | jsonb | NOT NULL | JSON Schema object for arguments |
| `endpoint_url` | text | NOT NULL | HTTPS-only; SSRF-checked at write + call |
| `credential_ciphertext` | text | NULL | sealed (AES-256-GCM master-key envelope); NULL = no credential |
| `classification` | text | NOT NULL CHECK IN (`'auto'`,`'approval'`) | default `'approval'` on creation (FR-003) |
| `enabled` | boolean | NOT NULL DEFAULT true | |
| `created_by_membership_id` | uuid | NOT NULL, FK | |
| `created_at` / `updated_at` | timestamptz | NOT NULL | |
| `deleted_at` | timestamptz | NULL | soft delete; records outlive the tool (FR-017) |

Indexes: partial unique `(tenant_id, name) WHERE deleted_at IS NULL` (also must not collide with built-in names — enforced in application validation); `(tenant_id) WHERE deleted_at IS NULL`.

## Entity: `tenant_tool_policies` (per-tenant policy over built-in tools)

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | uuid | PK | |
| `tenant_id` | uuid | NOT NULL, FK → tenants | |
| `tool_name` | text | NOT NULL | references the static built-in catalog by name |
| `enabled` | boolean | NOT NULL DEFAULT false | built-ins are opt-in per tenant |
| `require_approval` | boolean | NOT NULL DEFAULT false | tighten-only: effective classification = platform classification OR this flag (FR-003) |
| `updated_by_membership_id` | uuid | NOT NULL, FK | |
| `created_at` / `updated_at` | timestamptz | NOT NULL | |

Indexes: unique `(tenant_id, tool_name)`. No soft delete — absence of a row = platform defaults (disabled).

**Effective policy resolution** (in `tools::policy`, unit-tested):
- Built-in: available iff catalog row exists AND policy `enabled`; approval-required iff platform classification is `approval` OR policy `require_approval`. Tenants can never produce `auto` for a platform-`approval` tool.
- Tenant-defined: available iff row live AND `enabled`; approval-required iff `classification = 'approval'` (admin-set, defaults to approval).
- Spec assembly is deterministic: tools sorted by (source, name) before entering the prompt/request.

## Entity: `tool_requests` (request lifecycle = audit trail)

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | uuid | PK | |
| `tenant_id` | uuid | NOT NULL, FK → tenants | |
| `conversation_id` | uuid | NOT NULL, FK → conversations | |
| `generation_id` | uuid | NOT NULL, FK → ai_generations | requesting generation |
| `tool_name` | text | NOT NULL | |
| `tool_source` | text | NOT NULL CHECK IN (`'builtin'`,`'tenant'`) | |
| `tenant_tool_id` | uuid | NULL, FK → tenant_tools | set iff `tool_source='tenant'` (CHECK) |
| `arguments` | jsonb | NOT NULL | validated against the tool's schema |
| `status` | text | NOT NULL CHECK (vocabulary below) | |
| `approval_required` | boolean | NOT NULL | effective classification at request time |
| `expires_at` | timestamptz | NULL | set iff awaiting approval |
| `decided_by_membership_id` | uuid | NULL, FK | approve/deny decider |
| `decided_at` | timestamptz | NULL | |
| `started_at` / `finished_at` | timestamptz | NULL | execution window; duration derived |
| `result` | jsonb | NULL | success payload (staff-visible; never credentials) |
| `error` | text | NULL | sanitized failure detail |
| `chain_index` | smallint | NOT NULL | position within the generation's chain (0-based) |
| `created_at` / `updated_at` | timestamptz | NOT NULL | `created_at` = requested-at |

Indexes: `(tenant_id, conversation_id, created_at DESC)` (timeline); `(tenant_id, status) WHERE status = 'awaiting_approval'` (pending approvals panel); `(status, expires_at) WHERE status = 'awaiting_approval'` (expiry sweep); `(generation_id)`.

### `tool_requests.status` state machine

```text
                                ┌──────────► refused          (validation/policy failure — terminal)
 (created) ── validate ─────────┤
                                │  auto ───► executing ──► succeeded | failed | timed_out   (terminal)
                                │
                                └─ approval ► awaiting_approval ──► approved ──► executing ──► succeeded | failed | timed_out
                                                    │
                                                    ├──► denied      (staff decision — terminal, never executed)
                                                    ├──► expired     (sweep past expires_at — terminal, never executed)
                                                    └──► cancelled   (escalation/claim, or requesting generation
                                                                      cancelled while still in-flight — terminal)
```

Transition rules (FR-008, FR-013, FR-014, FR-015, research §6):
- Every transition is a conditional UPDATE gated on the expected prior status; zero-rows-affected = lost race, settled state returned.
- `executing` is entered exactly once; terminal states never transition again.
- `denied`/`expired`/`cancelled` rows MUST have `started_at IS NULL` (never executed) — CHECK constraint.
- Approve/deny/expire/cancel transitions emit `ai.tool_decision` on `outbox_events` in the same transaction (research §2).

## Migration note: `ai_generations.outcome` extension

The 021 migration (`0048_ai_conversation_engine.sql`) constrains `ai_generations.outcome` to `'success','superseded','cancelled_escalation','failed','fallback'`. A generation that ends because it posted an interim holding message and is waiting on a tool approval decision is none of these — it neither answered definitively (`success`) nor failed. Migration `0049` MUST drop and recreate that CHECK constraint adding `'awaiting_tool_approval'`:

```sql
ALTER TABLE ai_generations DROP CONSTRAINT ai_generations_outcome_check;
ALTER TABLE ai_generations ADD CONSTRAINT ai_generations_outcome_check CHECK (
    outcome IN ('success','superseded','cancelled_escalation','failed','fallback','awaiting_tool_approval')
);
```

A generation with this outcome still has a `response_message_id` (the interim message) — it is a "successful but incomplete" turn, distinct from `success` (a complete answer) so staff and metrics can tell the two apart.

## Reused: `outbox_events` (no schema change)

New `event_type = 'ai.tool_decision'`, payload `{ conversation_id, tool_request_id, outcome }`. Claimed by the existing responder worker; coalescing rules unchanged (a newer customer-message event for the same conversation supersedes normally).

## Relationships

```text
tenants 1─N tenant_tools
tenants 1─N tenant_tool_policies ── (by name) ──> built-in catalog (static, in code)
ai_generations 1─N tool_requests N─1 conversations
tool_requests N─0..1 tenant_tools
memberships 1─N tool_requests (as decider)
```

## Built-in catalog (code, not DB)

`tools::registry` — `BuiltinTool` trait: `fn spec(&self) -> ToolSpec` (name, description, JSON Schema) + `fn classification(&self) -> Classification` + `async fn execute(&self, ctx: ToolExecutionCtx, args: Value) -> Result<Value, ToolError>`. `ToolExecutionCtx` carries `tenant_id`, `conversation_id`, pool handle — implementations use tenant-scoped module queries only. v1 catalog: `lookup_customer` (auto), `update_customer_contact` (approval) — research §7.

## Validation rules (from spec FRs)

- FR-005: arguments validated against `input_schema` before any execution; failures → `refused`.
- FR-003a: `credential_ciphertext` write-only; API responses expose only a boolean `has_credential`; excluded from every SELECT feeding responses/logs/AI context.
- FR-002: every query filters `tenant_id`; tenant tool name resolution never crosses tenants.
- FR-008a: `chain_index < max_calls_per_generation` enforced by the engine before creating a request; cutoff recorded on the generation record.
- SC-008: `result`/`error` sanitization strips credential material (existing `sanitize_error_detail` + never echoing request headers).
