/// Number of exponential doublings needed for a delay to reach the cap.
///
/// The reservation query applies the same cap inside PostgreSQL so counting and locking stay
/// atomic. This value bounds its exponent and avoids evaluating an unbounded power.
pub(crate) fn max_doublings(base_secs: i32, max_secs: i32) -> i32 {
    let mut delay = i64::from(base_secs);
    let cap = i64::from(max_secs);
    let mut doublings = 0;
    while delay < cap {
        delay = delay.saturating_mul(2).min(cap);
        doublings += 1;
    }
    doublings
}

/// Delay applied after `failed_attempts` consecutive failures. Returns zero before the threshold.
pub(crate) fn lock_duration_secs(
    failed_attempts: i32,
    threshold: i32,
    base_secs: i32,
    max_secs: i32,
) -> i32 {
    if failed_attempts < threshold {
        return 0;
    }
    let exponent = failed_attempts
        .saturating_sub(threshold)
        .min(max_doublings(base_secs, max_secs));
    let multiplier = 1_i64.checked_shl(exponent as u32).unwrap_or(i64::MAX);
    (i64::from(base_secs)
        .saturating_mul(multiplier)
        .min(i64::from(max_secs))) as i32
}

#[cfg(test)]
mod tests {
    use super::lock_duration_secs;

    #[test]
    fn lock_delay_doubles_after_threshold_and_caps() {
        let cases = [
            (4, 0),
            (5, 60),
            (6, 120),
            (7, 240),
            (8, 480),
            (9, 900),
            (10, 900),
            (i32::MAX, 900),
        ];

        for (failed_attempts, expected_secs) in cases {
            assert_eq!(
                lock_duration_secs(failed_attempts, 5, 60, 900),
                expected_secs,
                "failed_attempts={failed_attempts}"
            );
        }
    }

    #[test]
    fn lock_delay_handles_a_cap_near_the_integer_limit_without_overflow() {
        assert_eq!(lock_duration_secs(40, 5, 1, i32::MAX), i32::MAX);
        assert_eq!(lock_duration_secs(7, 5, 1, i32::MAX), 4);
    }
}
