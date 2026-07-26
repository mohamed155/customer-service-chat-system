use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const PROVIDERS: [&str; 3] = ["openai", "anthropic", "gemini"];

// ── Summary (US1) ──

#[derive(Debug, Deserialize, ToSchema)]
pub struct SummaryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedRange {
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub from_ts: DateTime<Utc>,
    pub to_ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DateRangeDto {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantCountsDto {
    pub total: i64,
    pub active: i64,
    pub suspended: i64,
    pub created_in_range: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiUsageTotalsDto {
    pub total_calls: i64,
    pub successful_calls: i64,
    pub failed_calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlatformUsageSummaryDto {
    pub range: DateRangeDto,
    pub tenants: TenantCountsDto,
    pub active_conversations: i64,
    pub ai_usage: AiUsageTotalsDto,
    pub generated_at: DateTime<Utc>,
}

pub fn resolve_range(query: SummaryQuery, today: NaiveDate) -> Result<ResolvedRange, String> {
    let to_date = match query.to {
        Some(ref v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| "Invalid date format, expected YYYY-MM-DD".to_string())?,
        None => today,
    };
    let from_date = match query.from {
        Some(ref v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| "Invalid date format, expected YYYY-MM-DD".to_string())?,
        None => to_date - chrono::Duration::days(29),
    };
    if from_date > to_date {
        return Err("from must be on or before to".into());
    }
    if (to_date - from_date).num_days() + 1 > 365 {
        return Err("Date range must not exceed 365 days".into());
    }
    Ok(ResolvedRange {
        from_date,
        to_date,
        from_ts: from_date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        to_ts: (to_date + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc(),
    })
}

// ── Health (US2) ──

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HealthVerdict {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DependencyDetail {
    Queue {
        pending_count: i64,
        oldest_pending_age_seconds: Option<i64>,
        stuck_retry_count: i64,
    },
    Storage {
        backend: String,
        s3_configured: bool,
    },
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DependencyHealthDto {
    pub name: String,
    pub verdict: HealthVerdict,
    pub reason: Option<String>,
    pub probe_ms: u64,
    pub detail: Option<DependencyDetail>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct SystemHealthDto {
    pub overall: HealthVerdict,
    pub dependencies: Vec<DependencyHealthDto>,
    pub read_at: DateTime<Utc>,
}

// ── AI Providers (US3) ──

#[derive(Debug, Deserialize, ToSchema)]
pub struct WindowQuery {
    pub window: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderVerdict {
    Healthy,
    Degraded,
    Unhealthy,
    InsufficientData,
    NoActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FailuresByCategoryDto {
    pub authentication: i64,
    pub rate_limited: i64,
    pub unavailable: i64,
    pub timeout: i64,
    pub invalid_request: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderMetricsDto {
    pub provider: String,
    pub verdict: ProviderVerdict,
    pub total_calls: i64,
    pub successes: i64,
    pub failures: i64,
    pub failure_rate: Option<f64>,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub failures_by_category: FailuresByCategoryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderFailureDto {
    pub provider: String,
    pub model: String,
    pub error_category: Option<String>,
    pub latency_ms: i64,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WindowDto {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub preset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProviderStatusDto {
    pub window: WindowDto,
    pub providers: Vec<ProviderMetricsDto>,
    pub recent_failures: Vec<ProviderFailureDto>,
    pub generated_at: DateTime<Utc>,
}

pub fn resolve_window(
    q: WindowQuery,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>, String), String> {
    let preset = q.window.unwrap_or_else(|| "24h".into());
    let from = match preset.as_str() {
        "1h" => now - chrono::Duration::hours(1),
        "24h" => now - chrono::Duration::hours(24),
        "7d" => now - chrono::Duration::days(7),
        _ => return Err(format!("Unknown window: {preset}")),
    };
    Ok((from, now, preset))
}
