use std::time::Duration;

use super::*;

fn policy() -> Policy {
    Policy {
        failure_threshold: 3,
        failure_window: Duration::from_secs(60),
        open_duration: Duration::from_secs(30),
        max_concurrent: 2,
        acquire_timeout: Duration::from_millis(200),
    }
}

async fn fail(guard: &Arc<Guard>) {
    guard.acquire().await.expect("admitted").failure();
}

async fn succeed(guard: &Arc<Guard>) {
    guard.acquire().await.expect("admitted").success();
}

#[tokio::test(start_paused = true)]
async fn opens_after_repeated_failures_and_refuses_calls() {
    let guard = Guard::new("t", policy());
    fail(&guard).await;
    fail(&guard).await;
    assert_eq!(guard.snapshot().state, State::Closed);
    assert!(!guard.would_reject());

    fail(&guard).await;
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::Open);
    assert_eq!(snap.trips_total, 1);
    assert!(guard.would_reject());

    assert_eq!(guard.acquire().await.unwrap_err(), Rejection::CircuitOpen);
    assert_eq!(guard.snapshot().rejected_open_total, 1);
}

#[tokio::test(start_paused = true)]
async fn a_success_resets_the_failure_streak() {
    let guard = Guard::new("t", policy());
    fail(&guard).await;
    fail(&guard).await;
    succeed(&guard).await;
    fail(&guard).await;
    fail(&guard).await;
    let snap = guard.snapshot();
    assert_eq!(
        snap.state,
        State::Closed,
        "2 + 2 failures with a success between"
    );
    assert_eq!(snap.consecutive_failures, 2);
}

#[tokio::test(start_paused = true)]
async fn an_old_failure_streak_is_forgotten() {
    let guard = Guard::new("t", policy());
    fail(&guard).await;
    fail(&guard).await;
    tokio::time::advance(Duration::from_secs(61)).await;
    fail(&guard).await;
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::Closed);
    assert_eq!(snap.consecutive_failures, 1, "the streak restarted");
}

#[tokio::test(start_paused = true)]
async fn half_open_lets_one_probe_through_and_closes_on_success() {
    let guard = Guard::new("t", policy());
    for _ in 0..3 {
        fail(&guard).await;
    }
    assert!(guard.would_reject());

    tokio::time::advance(Duration::from_secs(30)).await;
    assert!(!guard.would_reject(), "due for a probe");

    let mut probe = guard.acquire().await.expect("the probe is admitted");
    assert_eq!(guard.snapshot().state, State::HalfOpen);
    assert!(guard.would_reject(), "the probe is running");
    assert_eq!(
        guard.acquire().await.unwrap_err(),
        Rejection::CircuitOpen,
        "only one probe at a time"
    );

    probe.success();
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::Closed);
    assert_eq!(snap.consecutive_failures, 0);
    assert!(snap.open_for.is_none());
    assert!(!guard.would_reject());
    succeed(&guard).await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_probe_opens_the_circuit_for_another_full_period() {
    let guard = Guard::new("t", policy());
    for _ in 0..3 {
        fail(&guard).await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;

    fail(&guard).await; // the probe
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::Open);
    assert_eq!(snap.trips_total, 2);

    tokio::time::advance(Duration::from_secs(29)).await;
    assert_eq!(guard.acquire().await.unwrap_err(), Rejection::CircuitOpen);
    tokio::time::advance(Duration::from_secs(1)).await;
    succeed(&guard).await;
    assert_eq!(guard.snapshot().state, State::Closed);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_probe_frees_the_probe_slot_without_recording_anything() {
    let guard = Guard::new("t", policy());
    for _ in 0..3 {
        fail(&guard).await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;

    let probe = guard.acquire().await.expect("probe");
    drop(probe); // the caller's future was cancelled
    assert_eq!(guard.snapshot().state, State::HalfOpen);
    assert_eq!(guard.snapshot().trips_total, 1);

    // The next caller becomes the probe instead of being refused forever.
    succeed(&guard).await;
    assert_eq!(guard.snapshot().state, State::Closed);
}

#[tokio::test(start_paused = true)]
async fn the_first_outcome_wins() {
    let guard = Guard::new("t", policy());
    let mut permit = guard.acquire().await.unwrap();
    permit.failure();
    permit.success();
    permit.failure();
    assert_eq!(guard.snapshot().consecutive_failures, 1);
}

#[tokio::test(start_paused = true)]
async fn bulkhead_refuses_when_every_slot_stays_busy() {
    let guard = Guard::new("t", policy());
    let first = guard.acquire().await.unwrap();
    let second = guard.acquire().await.unwrap();
    assert_eq!(guard.snapshot().in_flight, 2);

    let started = tokio::time::Instant::now();
    assert_eq!(guard.acquire().await.unwrap_err(), Rejection::BulkheadFull);
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(200),
        "waited the whole timeout"
    );

    let snap = guard.snapshot();
    assert_eq!(
        snap.state,
        State::Closed,
        "a full bulkhead is not a dependency failure"
    );
    assert_eq!(snap.consecutive_failures, 0);
    assert_eq!(snap.rejected_bulkhead_total, 1);

    drop(first);
    let mut third = guard.acquire().await.expect("a slot is free again");
    third.success();
    drop(second);
    drop(third);
    assert_eq!(guard.snapshot().in_flight, 0);
}

#[tokio::test(start_paused = true)]
async fn a_waiting_call_gets_the_slot_when_one_frees_up() {
    let guard = Guard::new("t", policy());
    let first = guard.acquire().await.unwrap();
    let _second = guard.acquire().await.unwrap();

    let waiter = {
        let guard = Arc::clone(&guard);
        tokio::spawn(async move { guard.acquire().await.map(|_| ()) })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(first);
    assert_eq!(waiter.await.unwrap(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn a_bulkhead_refusal_of_the_probe_leaves_the_probe_slot_free() {
    let guard = Guard::new("t", policy());
    for _ in 0..3 {
        fail(&guard).await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;
    // Keep both slots busy so the probe can't get one.
    let _a = guard.slots.clone().acquire_owned().await.unwrap();
    let _b = guard.slots.clone().acquire_owned().await.unwrap();
    assert_eq!(guard.acquire().await.unwrap_err(), Rejection::BulkheadFull);
    assert!(
        !guard.would_reject(),
        "no probe is running, so one is still due"
    );
}

#[test]
fn hub_returns_the_same_guard_and_lists_them_sorted() {
    let hub = Hub::new();
    let a = hub.guard("b-second", Policy::default());
    let same = hub.guard(
        "b-second",
        Policy {
            max_concurrent: 1,
            ..Policy::default()
        },
    );
    assert!(Arc::ptr_eq(&a, &same));
    assert_eq!(same.policy().max_concurrent, 16, "the first policy wins");
    hub.guard("a-first", Policy::default());

    let names: Vec<String> = hub.snapshots().into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["a-first", "b-second"]);
    assert!(hub.get("missing").is_none());
    assert!(hub.get("a-first").is_some());
}

#[tokio::test(start_paused = true)]
async fn a_call_that_started_before_the_circuit_opened_cannot_close_or_reopen_it() {
    let guard = Guard::new(
        "t",
        Policy {
            max_concurrent: 8,
            ..policy()
        },
    );
    // Three calls are already running when the circuit opens.
    let mut slow_ok = guard.acquire().await.unwrap();
    let mut slow_fail = guard.acquire().await.unwrap();
    let mut slow_ok_later = guard.acquire().await.unwrap();
    for _ in 0..3 {
        fail(&guard).await;
    }
    assert_eq!(guard.snapshot().state, State::Open);

    slow_ok.success();
    assert_eq!(
        guard.snapshot().state,
        State::Open,
        "a late success is not a probe"
    );

    tokio::time::advance(Duration::from_secs(30)).await;
    let mut probe = guard.acquire().await.unwrap();
    assert_eq!(guard.snapshot().state, State::HalfOpen);

    slow_fail.failure();
    slow_ok_later.success();
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::HalfOpen, "only the probe decides");
    assert_eq!(snap.trips_total, 1);
    assert!(guard.would_reject(), "the probe is still running");

    probe.success();
    assert_eq!(guard.snapshot().state, State::Closed);
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_waiting_probe_releases_the_probe_reservation() {
    let guard = Guard::new(
        "t",
        Policy {
            max_concurrent: 1,
            ..policy()
        },
    );
    for _ in 0..3 {
        fail(&guard).await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;

    // The only slot is busy, so the probe waits for it; its caller gives up after 10 ms.
    let busy = guard.slots.clone().acquire_owned().await.unwrap();
    let cancelled = tokio::time::timeout(Duration::from_millis(10), guard.acquire()).await;
    assert!(
        cancelled.is_err(),
        "the waiting call was cancelled, not refused"
    );
    drop(busy);

    // The reservation went with it: the next caller becomes the probe instead of being refused
    // for good.
    let mut probe = guard.acquire().await.expect("a new probe is admitted");
    probe.success();
    assert_eq!(guard.snapshot().state, State::Closed);
}

#[tokio::test(start_paused = true)]
async fn a_waiting_call_is_refused_if_the_circuit_opened_while_it_waited() {
    let guard = Guard::new(
        "t",
        Policy {
            failure_threshold: 1,
            max_concurrent: 1,
            acquire_timeout: Duration::from_secs(5),
            ..policy()
        },
    );
    let mut holder = guard.acquire().await.unwrap();
    let waiter = {
        let guard = Arc::clone(&guard);
        tokio::spawn(async move { guard.acquire().await.map(|_| ()) })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;

    holder.failure(); // opens the circuit
    drop(holder); // frees the slot the waiter was queued for
    assert_eq!(waiter.await.unwrap(), Err(Rejection::CircuitOpen));
    assert_eq!(guard.snapshot().rejected_open_total, 1);
}

#[tokio::test(start_paused = true)]
async fn outcomes_from_before_a_recovery_cannot_reopen_the_circuit_or_clear_a_new_streak() {
    let guard = Guard::new(
        "t",
        Policy {
            max_concurrent: 16,
            ..policy()
        },
    );
    // Calls already running when the circuit trips.
    let mut old: Vec<Permit> = Vec::new();
    for _ in 0..5 {
        old.push(guard.acquire().await.unwrap());
    }
    for _ in 0..3 {
        fail(&guard).await;
    }
    tokio::time::advance(Duration::from_secs(30)).await;
    succeed(&guard).await; // the probe: the circuit is closed again
    assert_eq!(guard.snapshot().state, State::Closed);

    // The old calls finish after the recovery. Five timeouts must not reopen the circuit.
    for permit in &mut old[..4] {
        permit.failure();
    }
    let snap = guard.snapshot();
    assert_eq!(snap.state, State::Closed);
    assert_eq!(snap.consecutive_failures, 0);
    assert_eq!(snap.trips_total, 1);

    // And an old success doesn't wipe a streak that has started since.
    fail(&guard).await;
    fail(&guard).await;
    old[4].success();
    assert_eq!(guard.snapshot().consecutive_failures, 2);
}

#[tokio::test(start_paused = true)]
async fn rejects_now_counts_the_skipped_call() {
    let guard = Guard::new("t", policy());
    assert!(!guard.rejects_now());
    assert_eq!(guard.snapshot().rejected_open_total, 0);
    for _ in 0..3 {
        fail(&guard).await;
    }
    assert!(guard.rejects_now());
    assert!(guard.rejects_now());
    assert!(guard.would_reject(), "the plain check does not count");
    assert_eq!(guard.snapshot().rejected_open_total, 2);
}
