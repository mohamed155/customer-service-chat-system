use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Channel values permitted by the conversations_channel_check constraint.
pub const ALLOWED_CHANNELS: [&str; 6] =
    ["email", "phone", "web_chat", "whatsapp", "telegram", "widget"];

pub const MAX_RANGE_DAYS: i64 = 366;
pub const DEFAULT_RANGE_DAYS: i64 = 30;

/// Raw query string parameters for both analytics endpoints.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AnalyticsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub channel: Option<String>,
}

/// Validated, resolved query parameters.
#[derive(Debug, Clone)]
pub struct ResolvedQuery {
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub from_ts: DateTime<Utc>,
    /// Exclusive upper bound: to_date + 1 day at 00:00:00Z.
    pub to_ts: DateTime<Utc>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DateRangeDto {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

pub fn resolve_query(query: AnalyticsQuery, today: NaiveDate) -> Result<ResolvedQuery, String> {
    let to_date = match query.to {
        Some(ref v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| "Invalid date format, expected YYYY-MM-DD".to_string())?,
        None => today,
    };
    let from_date = match query.from {
        Some(ref v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| "Invalid date format, expected YYYY-MM-DD".to_string())?,
        None => to_date - chrono::Duration::days(DEFAULT_RANGE_DAYS - 1),
    };

    if from_date > to_date {
        return Err("from must be on or before to".into());
    }
    if (to_date - from_date).num_days() + 1 > MAX_RANGE_DAYS {
        return Err("Date range must not exceed 366 days".into());
    }
    if let Some(ref c) = query.channel {
        if !ALLOWED_CHANNELS.contains(&c.as_str()) {
            return Err("Unknown channel".into());
        }
    }

    let from_ts = from_date
        .and_hms_opt(0, 0, 0)
        .ok_or("invalid date")?
        .and_utc();
    let to_ts = (to_date + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or("invalid date")?
        .and_utc();

    Ok(ResolvedQuery {
        from_date,
        to_date,
        from_ts,
        to_ts,
        channel: query.channel,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChannelBreakdownItem {
    pub channel: String,
    pub conversation_count: i64,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnalyticsSummaryDto {
    pub range: DateRangeDto,
    pub channel: Option<String>,
    pub conversation_volume: i64,
    pub concluded_count: i64,
    pub ai_resolution_rate: Option<f64>,
    pub handoff_rate: Option<f64>,
    pub avg_first_response_seconds: Option<f64>,
    pub avg_response_seconds: Option<f64>,
    pub satisfaction_avg: Option<f64>,
    pub satisfaction_count: i64,
    pub total_tokens: i64,
    pub unattributed_tokens: i64,
    pub channels: Vec<ChannelBreakdownItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnalyticsSummaryResponse {
    pub data: AnalyticsSummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimeseriesDay {
    pub date: NaiveDate,
    pub conversation_volume: i64,
    pub ai_resolved: i64,
    pub handed_off: i64,
    pub satisfaction_avg: Option<f64>,
    pub satisfaction_count: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnalyticsTimeseriesDto {
    pub range: DateRangeDto,
    pub channel: Option<String>,
    pub days: Vec<TimeseriesDay>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_range_is_30_inclusive_days_ending_today() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 19).unwrap();
        let r = resolve_query(AnalyticsQuery {
            from: None,
            to: None,
            channel: None,
        }, today).unwrap();
        assert_eq!(r.to_date, today);
        assert_eq!(r.from_date, NaiveDate::from_ymd_opt(2026, 6, 20).unwrap());
        assert_eq!((r.to_date - r.from_date).num_days() + 1, 30);
    }

    #[test]
    fn explicit_valid_range_round_trips() {
        let r = resolve_query(AnalyticsQuery {
            from: Some("2026-03-10".into()),
            to: Some("2026-03-12".into()),
            channel: None,
        }, NaiveDate::from_ymd_opt(2026, 7, 19).unwrap()).unwrap();
        assert_eq!(r.from_date, NaiveDate::from_ymd_opt(2026, 3, 10).unwrap());
        assert_eq!(r.to_date, NaiveDate::from_ymd_opt(2026, 3, 12).unwrap());
        assert_eq!(r.from_ts.to_rfc3339(), "2026-03-10T00:00:00+00:00");
        assert_eq!(r.to_ts.to_rfc3339(), "2026-03-13T00:00:00+00:00");
    }

    #[test]
    fn from_after_to_errors() {
        let err = resolve_query(AnalyticsQuery {
            from: Some("2026-03-12".into()),
            to: Some("2026-03-10".into()),
            channel: None,
        }, NaiveDate::from_ymd_opt(2026, 7, 19).unwrap()).unwrap_err();
        assert_eq!(err, "from must be on or before to");
    }

    #[test]
    fn range_exceeds_366_days_errors() {
        let err = resolve_query(AnalyticsQuery {
            from: Some("2025-01-01".into()),
            to: Some("2026-12-31".into()),
            channel: None,
        }, NaiveDate::from_ymd_opt(2026, 7, 19).unwrap()).unwrap_err();
        assert_eq!(err, "Date range must not exceed 366 days");
    }

    #[test]
    fn bad_channel_errors() {
        let err = resolve_query(AnalyticsQuery {
            from: None,
            to: None,
            channel: Some("carrier-pigeon".into()),
        }, NaiveDate::from_ymd_opt(2026, 7, 19).unwrap()).unwrap_err();
        assert_eq!(err, "Unknown channel");
    }

    #[test]
    fn bad_date_string_errors() {
        let err = resolve_query(AnalyticsQuery {
            from: Some("notadate".into()),
            to: None,
            channel: None,
        }, NaiveDate::from_ymd_opt(2026, 7, 19).unwrap()).unwrap_err();
        assert_eq!(err, "Invalid date format, expected YYYY-MM-DD");
    }
}
