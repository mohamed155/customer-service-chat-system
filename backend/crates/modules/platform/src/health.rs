use crate::model::{HealthVerdict, ProviderVerdict};
use storage::{StorageBackend, StorageKind};

pub fn queue_verdict(
    pending: i64,
    oldest_age_seconds: Option<i64>,
    stuck: i64,
) -> (HealthVerdict, Option<String>) {
    let age = oldest_age_seconds.unwrap_or(0);
    if pending >= 1000 || age > 1800 {
        let reason = format!("{pending} events waiting; oldest waiting {}s", age);
        return (HealthVerdict::Unhealthy, Some(reason));
    }
    if pending >= 100 || age >= 300 || stuck > 0 {
        let reason = format!("{pending} events waiting; oldest waiting {}s", age);
        return (HealthVerdict::Degraded, Some(reason));
    }
    (HealthVerdict::Healthy, None)
}

pub fn storage_verdict(
    backend: StorageBackend,
    probe_ok: Result<(), String>,
) -> (HealthVerdict, Option<String>) {
    if backend.kind == StorageKind::InMemory && backend.s3_configured {
        return (
            HealthVerdict::Unhealthy,
            Some("configured for S3 but running on the in-process fallback backend".into()),
        );
    }
    if let Err(e) = probe_ok {
        return (HealthVerdict::Unhealthy, Some(e));
    }
    (HealthVerdict::Healthy, None)
}

pub fn overall(verdicts: &[HealthVerdict]) -> HealthVerdict {
    verdicts
        .iter()
        .copied()
        .max()
        .unwrap_or(HealthVerdict::Healthy)
}

pub fn provider_verdict(total_calls: i64, failures: i64) -> ProviderVerdict {
    if total_calls == 0 {
        return ProviderVerdict::NoActivity;
    }
    if total_calls < 20 {
        return ProviderVerdict::InsufficientData;
    }
    let rate = failures as f64 / total_calls as f64;
    if rate >= 0.10 {
        ProviderVerdict::Unhealthy
    } else if rate >= 0.02 {
        ProviderVerdict::Degraded
    } else {
        ProviderVerdict::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HealthVerdict;

    #[test]
    fn queue_healthy_below_thresholds() {
        assert_eq!(queue_verdict(99, Some(299), 0).0, HealthVerdict::Healthy);
        assert_eq!(queue_verdict(0, None, 0).0, HealthVerdict::Healthy);
    }

    #[test]
    fn queue_degraded_at_thresholds() {
        assert_eq!(queue_verdict(100, Some(0), 0).0, HealthVerdict::Degraded);
        assert_eq!(queue_verdict(999, Some(0), 0).0, HealthVerdict::Degraded);
        assert_eq!(queue_verdict(0, Some(300), 0).0, HealthVerdict::Degraded);
        assert_eq!(queue_verdict(0, Some(1800), 0).0, HealthVerdict::Degraded);
    }

    #[test]
    fn queue_unhealthy_at_thresholds() {
        assert_eq!(queue_verdict(1000, Some(0), 0).0, HealthVerdict::Unhealthy);
        assert_eq!(queue_verdict(0, Some(1801), 0).0, HealthVerdict::Unhealthy);
    }

    #[test]
    fn queue_stuck_retries_cause_degraded() {
        assert_eq!(queue_verdict(0, None, 1).0, HealthVerdict::Degraded);
    }

    #[test]
    fn overall_max_of_empty_is_healthy() {
        assert_eq!(overall(&[]), HealthVerdict::Healthy);
    }

    #[test]
    fn overall_max_of_mixed() {
        assert_eq!(
            overall(&[
                HealthVerdict::Healthy,
                HealthVerdict::Degraded,
                HealthVerdict::Unhealthy
            ]),
            HealthVerdict::Unhealthy
        );
    }

    #[test]
    fn provider_no_activity() {
        assert_eq!(provider_verdict(0, 0), ProviderVerdict::NoActivity);
    }

    #[test]
    fn provider_insufficient_data() {
        assert_eq!(provider_verdict(19, 0), ProviderVerdict::InsufficientData);
    }

    #[test]
    fn provider_unhealthy_at_10pc() {
        assert_eq!(provider_verdict(100, 10), ProviderVerdict::Unhealthy);
        assert_eq!(provider_verdict(100, 25), ProviderVerdict::Unhealthy);
    }

    #[test]
    fn provider_degraded_at_2pc() {
        assert_eq!(provider_verdict(100, 2), ProviderVerdict::Degraded);
        assert_eq!(provider_verdict(100, 4), ProviderVerdict::Degraded);
    }

    #[test]
    fn provider_healthy_below_2pc() {
        assert_eq!(provider_verdict(100, 1), ProviderVerdict::Healthy);
    }
}
