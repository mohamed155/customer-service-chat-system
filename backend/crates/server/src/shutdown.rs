//! Graceful shutdown order (MOH-81): readiness fails first, the server drains after.
//!
//! When the process gets SIGTERM or Ctrl-C it sets the `draining` flag at once, which makes
//! `/ready` answer 503 (the `shutdown` check, see `observability::health::DrainingCheck`). A load
//! balancer or orchestrator that polls readiness then stops sending new requests. The server keeps
//! accepting for a short delay so the poll can notice, and only then starts the graceful drain of
//! requests already in flight. Without the delay a request routed in the same instant is refused.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Longest the server keeps accepting after readiness has started failing.
pub const MAX_READINESS_DRAIN_DELAY: Duration = Duration::from_secs(5);

/// How long to keep serving after readiness fails: half of `SHUTDOWN_GRACE_SECONDS`, at most
/// [`MAX_READINESS_DRAIN_DELAY`]. `SHUTDOWN_GRACE_SECONDS=0` (the test setting) means no delay.
pub fn readiness_drain_delay(grace_seconds: u64) -> Duration {
    Duration::from_secs(grace_seconds / 2).min(MAX_READINESS_DRAIN_DELAY)
}

/// Fail readiness now, then keep serving for `delay` before the caller starts draining.
pub async fn drain_readiness(draining: &AtomicBool, delay: Duration) {
    draining.store(true, Ordering::SeqCst);
    tracing::info!(
        delay_ms = delay.as_millis() as u64,
        "readiness now fails; still accepting requests before the drain"
    );
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn the_delay_is_half_the_grace_period_capped_at_five_seconds() {
        assert_eq!(readiness_drain_delay(0), Duration::ZERO);
        assert_eq!(readiness_drain_delay(1), Duration::ZERO);
        assert_eq!(readiness_drain_delay(4), Duration::from_secs(2));
        assert_eq!(readiness_drain_delay(10), Duration::from_secs(5));
        assert_eq!(readiness_drain_delay(300), MAX_READINESS_DRAIN_DELAY);
    }

    #[tokio::test(start_paused = true)]
    async fn readiness_fails_before_the_delay_starts_and_the_drain_waits_for_it() {
        let draining = Arc::new(AtomicBool::new(false));
        let task = {
            let draining = draining.clone();
            tokio::spawn(async move { drain_readiness(&draining, Duration::from_secs(5)).await })
        };

        // Let the task run up to its sleep.
        tokio::task::yield_now().await;
        assert!(
            draining.load(Ordering::SeqCst),
            "the flag is set before any waiting"
        );
        assert!(
            !task.is_finished(),
            "the drain has not been allowed to start yet"
        );

        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(2)).await;
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn no_delay_means_the_flag_is_set_and_the_drain_starts_at_once() {
        let draining = AtomicBool::new(false);
        drain_readiness(&draining, Duration::ZERO).await;
        assert!(draining.load(Ordering::SeqCst));
    }
}
