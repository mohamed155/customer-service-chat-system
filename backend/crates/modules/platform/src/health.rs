use crate::model::{HealthVerdict, ProviderVerdict};
use resilience::{Snapshot, State};
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

/// Health of one circuit breaker (MOH-40). Open is unhealthy: every call to that dependency is
/// being refused, so AI turns are handed to a human and WhatsApp messages wait in the queue.
/// Half-open (one probe running) is degraded. A closed circuit is healthy, with a note when it
/// has opened since start so a flapping dependency does not go unnoticed.
pub fn circuit_verdict(snapshot: &Snapshot) -> (HealthVerdict, Option<String>) {
    let trips = snapshot.trips_total;
    match snapshot.state {
        State::Open => (
            HealthVerdict::Unhealthy,
            Some(format!(
                "circuit open for {}s (opened {trips} time(s) since start); calls are refused until the next probe",
                snapshot.open_for.map_or(0, |d| d.as_secs())
            )),
        ),
        State::HalfOpen => (
            HealthVerdict::Degraded,
            Some(format!(
                "circuit half-open: testing the dependency with one call (opened {trips} time(s) since start)"
            )),
        ),
        // A full bulkhead is checked before past trips, so it is never hidden by an old one.
        State::Closed if snapshot.in_flight >= snapshot.max_concurrent => (
            HealthVerdict::Degraded,
            Some(format!(
                "all {} slots busy; further calls wait or are refused{}",
                snapshot.max_concurrent,
                if trips > 0 {
                    format!(" (circuit opened {trips} time(s) since start)")
                } else {
                    String::new()
                }
            )),
        ),
        State::Closed if trips > 0 => (
            HealthVerdict::Healthy,
            Some(format!("circuit closed; it opened {trips} time(s) since start")),
        ),
        State::Closed => (HealthVerdict::Healthy, None),
    }
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

    fn snapshot(state: State, trips: u64, in_flight: usize) -> Snapshot {
        Snapshot {
            name: "ai:openai".into(),
            state,
            consecutive_failures: 0,
            trips_total: trips,
            rejected_open_total: 0,
            rejected_bulkhead_total: 0,
            in_flight,
            max_concurrent: 4,
            open_for: Some(std::time::Duration::from_secs(12)),
        }
    }

    #[test]
    fn circuit_open_is_unhealthy_with_a_reason() {
        let (verdict, reason) = circuit_verdict(&snapshot(State::Open, 2, 0));
        assert_eq!(verdict, HealthVerdict::Unhealthy);
        let reason = reason.unwrap();
        assert!(
            reason.contains("open for 12s") && reason.contains("2 time(s)"),
            "{reason}"
        );
    }

    #[test]
    fn circuit_half_open_is_degraded() {
        assert_eq!(
            circuit_verdict(&snapshot(State::HalfOpen, 1, 1)).0,
            HealthVerdict::Degraded
        );
    }

    #[test]
    fn circuit_closed_is_healthy_and_notes_past_trips_or_a_full_bulkhead() {
        assert_eq!(
            circuit_verdict(&snapshot(State::Closed, 0, 1)),
            (HealthVerdict::Healthy, None)
        );
        let (verdict, reason) = circuit_verdict(&snapshot(State::Closed, 3, 0));
        assert_eq!(verdict, HealthVerdict::Healthy);
        assert!(reason.unwrap().contains("3 time(s)"));
        assert_eq!(
            circuit_verdict(&snapshot(State::Closed, 0, 4)).0,
            HealthVerdict::Degraded
        );
    }

    #[test]
    fn a_full_bulkhead_is_not_hidden_by_past_trips() {
        let (verdict, reason) = circuit_verdict(&snapshot(State::Closed, 3, 4));
        assert_eq!(verdict, HealthVerdict::Degraded);
        let reason = reason.unwrap();
        assert!(
            reason.contains("all 4 slots busy") && reason.contains("opened 3 time(s)"),
            "{reason}"
        );
    }

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
    fn storage_healthy_when_probe_ok() {
        let backend = StorageBackend {
            kind: StorageKind::S3,
            s3_configured: true,
        };
        assert_eq!(storage_verdict(backend, Ok(())).0, HealthVerdict::Healthy);
    }

    #[test]
    fn storage_unhealthy_when_s3_configured_but_running_in_memory() {
        let backend = StorageBackend {
            kind: StorageKind::InMemory,
            s3_configured: true,
        };
        assert_eq!(storage_verdict(backend, Ok(())).0, HealthVerdict::Unhealthy);
    }

    #[test]
    fn storage_unhealthy_on_probe_error() {
        let backend = StorageBackend {
            kind: StorageKind::S3,
            s3_configured: true,
        };
        assert_eq!(
            storage_verdict(backend, Err("boom".into())).0,
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
