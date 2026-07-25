use billing::model::metric_usage;
use billing::queries::current_period;
use chrono::TimeZone;

// ── metric_usage tests ─────────────────────────────────────────

#[test]
fn unlimited_allowance_is_always_normal() {
    let result = metric_usage(999_999, None);
    assert_eq!(result.used, 999_999);
    assert_eq!(result.allowance, None);
    assert_eq!(result.percent, None);
    assert_eq!(result.state, "normal");
}

#[test]
fn zero_allowance_is_exceeded() {
    let result = metric_usage(0, Some(0));
    assert_eq!(result.used, 0);
    assert_eq!(result.allowance, Some(0));
    assert_eq!(result.percent, Some(100));
    assert_eq!(result.state, "exceeded");
}

#[test]
fn negative_allowance_is_exceeded() {
    let result = metric_usage(5, Some(-1));
    assert_eq!(result.allowance, Some(-1));
    assert_eq!(result.percent, Some(100));
    assert_eq!(result.state, "exceeded");
}

#[test]
fn zero_used_normal() {
    let result = metric_usage(0, Some(100));
    assert_eq!(result.percent, Some(0));
    assert_eq!(result.state, "normal");
}

#[test]
fn below_eighty_percent_is_normal() {
    let result = metric_usage(79, Some(100));
    assert_eq!(result.percent, Some(79));
    assert_eq!(result.state, "normal");
}

#[test]
fn exactly_eighty_percent_is_warning() {
    let result = metric_usage(80, Some(100));
    assert_eq!(result.percent, Some(80));
    assert_eq!(result.state, "warning");
}

#[test]
fn ninety_nine_percent_is_warning() {
    let result = metric_usage(99, Some(100));
    assert_eq!(result.percent, Some(99));
    assert_eq!(result.state, "warning");
}

#[test]
fn exactly_at_allowance_is_exceeded() {
    let result = metric_usage(100, Some(100));
    assert_eq!(result.percent, Some(100));
    assert_eq!(result.state, "exceeded");
}

#[test]
fn over_allowance_is_exceeded() {
    let result = metric_usage(150, Some(100));
    assert_eq!(result.percent, Some(150));
    assert_eq!(result.state, "exceeded");
}

#[test]
fn percent_is_floored_not_rounded() {
    let result = metric_usage(1, Some(3));
    assert_eq!(result.percent, Some(33));
    assert_eq!(result.state, "normal");
}

// ── current_period tests ───────────────────────────────────────

#[test]
fn mid_month_period() {
    let now = chrono::Utc
        .with_ymd_and_hms(2026, 7, 15, 10, 30, 0)
        .unwrap();
    let (start, end) = current_period(now);
    assert_eq!(
        start,
        chrono::Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        end,
        chrono::Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn first_of_month() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
    let (start, end) = current_period(now);
    assert_eq!(
        start,
        chrono::Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        end,
        chrono::Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn last_of_month() {
    let now = chrono::Utc
        .with_ymd_and_hms(2026, 1, 31, 23, 59, 59)
        .unwrap();
    let (start, end) = current_period(now);
    assert_eq!(
        start,
        chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        end,
        chrono::Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn december_wraps_to_january() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 12, 15, 0, 0, 0).unwrap();
    let (start, end) = current_period(now);
    assert_eq!(
        start,
        chrono::Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        end,
        chrono::Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn leap_year_february() {
    let now = chrono::Utc.with_ymd_and_hms(2024, 2, 15, 0, 0, 0).unwrap();
    let (start, end) = current_period(now);
    assert_eq!(
        start,
        chrono::Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap()
    );
    assert_eq!(
        end,
        chrono::Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap()
    );
}
