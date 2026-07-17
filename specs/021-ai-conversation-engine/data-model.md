# Data Model: AI Conversation Engine

Single migration: `backend/migrations/0048_ai_conversation_engine.sql`. Follows repo conventions (UUID PK via `gen_random_uuid()`, `timestamptz` timestamps, `tenant_id` on every tenant-owned table, indexes on production query paths).

## 1. `ai_generations` (new table)

One row per engine run (a claimed customer-message trigger), regardless of how many provider attempts it made. This is the inspectable Generation Record of FR-015 / SC-008.

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `id` | `uuid` | PK, default `gen_random_uuid()` | generation id; also used as `generationId` in SSE events |
| `tenant_id` | `uuid` | NOT NULL, FK → `tenants(id)` | Principle II |
| `conversation_id` | `uuid` | NOT NULL, FK → `conversations(id)` | |
| `trigger_message_id` | `uuid` | NOT NULL, FK → `messages(id)` | the customer message that started the run (newest at claim time after coalescing) |
| `response_message_id` | `uuid` | NULL, FK → `messages(id)` | set only when outcome = `success` |
| `usage_record_id` | `uuid` | NULL, FK → `ai_usage_records(id)` | last provider-call usage row of the run; linkage, not duplication |
| `provider` | `text` | NULL | resolved provider actually called (NULL if run ended before any call) |
| `model` | `text` | NULL | |
| `outcome` | `text` | NOT NULL, CHECK in (`success`, `superseded`, `cancelled_escalation`, `failed`, `fallback`) | `failed` = ended in error but fallback insert itself failed (rare); normal exhaustion path is `fallback` |
| `error_category` | `text` | NULL | last provider error category (`authentication`, `rate_limited`, `unavailable`, `timeout`, `invalid_request`) when relevant |
| `attempts` | `smallint` | NOT NULL, default 0 | provider attempts made (0 for early supersede) |
| `continuation_used` | `boolean` | NOT NULL, default false | resume-by-continuation happened (research §4) |
| `retrieval_chunk_count` | `smallint` | NOT NULL, default 0 | chunks injected (0–5) |
| `retrieval_top_similarity` | `real` | NULL | grounding input to confidence formula |
| `retrieval_degraded` | `boolean` | NOT NULL, default false | retrieval failed/timed out (FR-004) |
| `confidence_score` | `real` | NULL, CHECK (0–1) | copy of the stored message score, for record-level inspection without join |
| `latency_ms` | `integer` | NOT NULL | claim → terminal outcome |
| `request_id` | `text` | NULL | tracing correlation |
| `created_at` | `timestamptz` | NOT NULL, default `now()` | |

**Indexes**: `(tenant_id, conversation_id, created_at DESC)` — conversation inspection path; `(tenant_id, created_at DESC)` — tenant-wide operational queries.

**Lifecycle**: insert-once at run end (terminal outcome known); append-only, never updated or deleted (audit-style).

## 2. `messages` (altered — additive)

| Column | Type | Constraints | Notes |
|---|---|---|---|
| `ai_confidence_score` | `real` | NULL, CHECK (`ai_confidence_score >= 0 AND ai_confidence_score <= 1`) | set only on `kind = 'ai'` rows at insert; NULL for all other kinds and for pre-021 AI messages |

Band (`high` ≥ 0.70, `medium` ≥ 0.40, `low` < 0.40) is **derived, never stored** — one backend function is the source of truth; the API returns both score and derived band (see `contracts/message-confidence.md`).

No other message changes: the fallback message reuses the existing `system` kind (auto-ack precedent from 017); AI responses keep `kind = 'ai'`; citations remain as built in 020.

## 3. Entities without new storage

| Spec entity | Realization |
|---|---|
| AI Response | existing `messages` row (`kind='ai'`) + `ai_confidence_score` + existing `message_citations` |
| Generation Record | `ai_generations` row + `engine.generate` tracing span |
| Confidence Metadata | `messages.ai_confidence_score` (+ copy on `ai_generations`); band derived |
| Fallback Message | existing `messages` row (`kind='system'`, platform default body) + escalation via existing `escalations` tables |
| Conversation Summary | **not persisted** — request-scoped API response only (spec assumption) |
| In-flight generation state | **not persisted** — worker-local + SSE events; timeline remains source of truth on reload |

## 4. State transitions (engine run)

```
claimed ──gates fail──────────────────────────────► (no record; event deleted — pre-existing 017 behavior)
claimed ──rule match──────────────────────────────► (escalation routed; pre-existing 017 behavior)
claimed ─► generating ──done + checks pass────────► success   (message + citations + confidence in one tx)
           generating ──newer customer message───► superseded (partial discarded; newer event re-triggers)
           generating ──escalated/human claim────► cancelled_escalation (partial discarded; engine silent)
           generating ──retriable error──────────► retry / continuation (≤3 attempts, 45 s deadline) ─┐
           generating ──non-retriable / exhausted─► fallback  (system message + escalation route in tx)◄┘
```

Invariants: at most one non-terminal run per conversation (enforced by outbox single-claim + coalescing); every terminal state deletes the outbox event; `success` is the only outcome that writes an `ai` message; `fallback` is the only outcome that writes a `system` message.

## 5. Validation rules

- Confidence formula inputs and thresholds per research §6; exact-value unit tests required.
- `ai_generations.outcome` transitions are insert-only terminal states — no UPDATE path exists.
- Summary window: last 50 messages, tenant-scoped query ordered by `(created_at, seq)` (existing timeline ordering).
- History window for generation: existing `recent_history(…, 20)`.
- All new queries carry `tenant_id` predicates (isolation integration tests assert cross-tenant invisibility for `ai_generations` and confidence fields).
