# Data Model: Analytics Foundation

No new tables. This feature is a read model over existing tables plus one index-only migration. Source-table facts below are verified against migrations 0026–0051.

## Source tables (read-only)

| Table | Columns used | Notes |
|-------|-------------|-------|
| `conversations` | `tenant_id`, `id`, `channel`, `status`, `created_at`, `deleted_at` | status ∈ open/pending/resolved/closed; channel ∈ email/phone/web_chat/whatsapp/telegram/widget; soft-delete via `deleted_at` |
| `messages` | `tenant_id`, `conversation_id`, `kind`, `created_at`, `seq` | kind ∈ customer/reply/note/ai/system; append-only |
| `escalations` | `tenant_id`, `conversation_id` | row existence = conversation was handed off (rows persist after close) |
| `conversation_feedback` | `tenant_id`, `rating`, `submitted_at`, `channel` | append-only; rating 1–5; ≤1 row per conversation |
| `ai_usage_records` | `tenant_id`, `input_tokens`, `output_tokens`, `created_at` | no conversation linkage on this table |
| `ai_generations` | `tenant_id`, `conversation_id`, `usage_record_id` | bridge for channel-attributing token usage |

## Migration 0052 (indexes only)

```sql
CREATE INDEX conversations_tenant_created_idx
    ON conversations (tenant_id, created_at)
    WHERE deleted_at IS NULL;

CREATE INDEX ai_generations_tenant_usage_record_idx
    ON ai_generations (tenant_id, usage_record_id)
    WHERE usage_record_id IS NOT NULL;
```

## Derived read models (Rust, `analytics/src/model.rs`; all utoipa `ToSchema`)

### AnalyticsSummary

| Field | Type | Definition |
|-------|------|------------|
| `range` | `DateRange { from, to }` | Echo of applied UTC date range |
| `channel` | `Option<String>` | Echo of applied channel filter |
| `conversation_volume` | `i64` | Conversations created in range, not soft-deleted |
| `concluded_count` | `i64` | Volume subset with status resolved/closed |
| `ai_resolution_rate` | `Option<f64>` | ai_resolved / concluded; `None` when concluded = 0 |
| `handoff_rate` | `Option<f64>` | handed_off / volume; `None` when volume = 0 |
| `avg_first_response_seconds` | `Option<f64>` | Mean first customer-msg → first reply/ai gap; `None` when no responded conversations |
| `avg_response_seconds` | `Option<f64>` | All-pairs mean (secondary metric); `None` when no pairs |
| `satisfaction_avg` | `Option<f64>` | AVG(rating) over feedback submitted in range; `None` when no ratings |
| `satisfaction_count` | `i64` | Ratings submitted in range |
| `total_tokens` | `i64` | Σ input+output tokens in range (channel-filtered via ai_generations join when filter active) |
| `unattributed_tokens` | `i64` | Tokens with no generation→conversation link (0 relevance when channel filter active — excluded there) |
| `channels` | `Vec<ChannelBreakdownItem>` | Per-channel volume for the range (ignores channel filter; drives the breakdown widget) |

### ChannelBreakdownItem

| Field | Type | Definition |
|-------|------|------------|
| `channel` | `String` | DB channel value |
| `conversation_count` | `i64` | Conversations created in range on this channel |
| `share` | `f64` | count / total volume (0 when volume = 0) |

### AnalyticsTimeseries

| Field | Type | Definition |
|-------|------|------------|
| `range`, `channel` | as above | Echoes |
| `days` | `Vec<TimeseriesDay>` | One entry per UTC calendar day in range, zero-filled |

### TimeseriesDay

| Field | Type | Definition |
|-------|------|------------|
| `date` | `NaiveDate` | UTC day |
| `conversation_volume` | `i64` | Created that day |
| `ai_resolved` | `i64` | Created that day, concluded, never escalated (current status at query time) |
| `handed_off` | `i64` | Created that day with an escalations row |
| `satisfaction_avg` | `Option<f64>` | AVG(rating) of feedback submitted that day |
| `satisfaction_count` | `i64` | Ratings that day |
| `total_tokens` | `i64` | Tokens recorded that day |

## Cohort & filter rules (apply to every query)

1. `tenant_id = $1` always (Principle II).
2. Conversation cohort: `created_at >= $from AND created_at < $to + 1 day`, `deleted_at IS NULL`; channel filter adds `channel = $c`.
3. Feedback and token metrics use their own timestamps (`submitted_at`, `created_at`) against the same range; channel filter applies via `conversation_feedback.channel` and the `ai_generations → conversations` join respectively.
4. Zero-filling: series generated with `generate_series($from_date, $to_date, '1 day')` LEFT JOINed to aggregates — never gaps. **`generate_series` is inclusive of its end value**, so it must be bound with the inclusive `to` *date* — never with the exclusive `to + 1 day` timestamp used for the conversation cohort, which would emit one bogus trailing day. The series must contain exactly `to − from + 1` rows.
5. Rates computed in Rust from counts (avoid SQL divide-by-zero; `None` = no-data state).

## State transitions

None introduced. Analytics reads current state; the only retroactive-change vector is late feedback (by design, spec assumption) and conversations concluding/escalating after their creation day (rates reflect current outcome at query time, consistent with the spec's "terminal outcome by query time" rule).

## Frontend domain models (`core/api/tenant-api.models.ts`)

Wire interfaces mirror the JSON contract (snake_case) with `analyticsSummaryFromWire` / `analyticsTimeseriesFromWire` mappers producing camelCase domain models, following the existing conversations/feedback mapper pattern. Store state: `{ preset: '7d'|'30d'|'90d'|'custom', from, to, channel: string|null, summary, timeseries, loading, error }`.
