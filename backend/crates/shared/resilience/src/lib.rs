//! Circuit breakers and bulkheads for outbound calls (MOH-40, architecture §10.2 and §15).
//!
//! # Purpose
//! When an AI provider or another third party is down or slow, calls keep going to it, tie up
//! workers and slow the whole system. A [`Guard`] sits in front of one dependency and does two
//! things:
//!
//! - **Circuit breaker** (closed, open, half-open): after [`Policy::failure_threshold`]
//!   consecutive failures inside [`Policy::failure_window`] the circuit opens and calls are refused
//!   at once, without touching the dependency. After [`Policy::open_duration`] one probe call is
//!   let through (half-open). If it succeeds the circuit closes, if it fails the circuit opens
//!   again.
//! - **Bulkhead**: at most [`Policy::max_concurrent`] calls are in flight to the dependency. A
//!   call waits up to [`Policy::acquire_timeout`] for a slot and is then refused, so one slow
//!   dependency can't use up every worker.
//!
//! # What counts as a failure
//! The caller decides. Only outcomes that say the dependency itself is unhealthy (5xx, timeouts,
//! connection errors) should be reported with [`Permit::failure`]. A refusal that proves the
//! dependency answered (401, 429, 400) is reported with [`Permit::success`], so one tenant's bad
//! API key never opens the circuit for every other tenant.
//!
//! # Dropping a permit
//! A permit dropped without an outcome (the caller's future was cancelled) frees its bulkhead slot
//! and, in half-open, the probe slot, and records nothing: a cancelled call is no evidence either
//! way.
//!
//! # Observability
//! Metrics carry the guard name only, never a tenant (FR-019):
//! `circuit_breaker_state{breaker}` (0 closed, 1 half-open, 2 open),
//! `circuit_breaker_trips_total{breaker}`, `circuit_breaker_rejections_total{breaker,reason}` and
//! `bulkhead_in_flight{breaker}`. A [`Hub`] lists every guard for the platform health view.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

/// Tuning for one [`Guard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Consecutive failures that open the circuit.
    pub failure_threshold: u32,
    /// A failure streak older than this is forgotten: the next failure starts a new streak.
    pub failure_window: Duration,
    /// How long the circuit stays open before one probe call is allowed.
    pub open_duration: Duration,
    /// Calls allowed in flight at once (the bulkhead).
    pub max_concurrent: usize,
    /// How long a call waits for a bulkhead slot before it is refused.
    pub acquire_timeout: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            open_duration: Duration::from_secs(30),
            max_concurrent: 16,
            acquire_timeout: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    HalfOpen,
    Open,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::HalfOpen => "half_open",
            Self::Open => "open",
        }
    }

    fn gauge(self) -> f64 {
        match self {
            Self::Closed => 0.0,
            Self::HalfOpen => 1.0,
            Self::Open => 2.0,
        }
    }
}

/// Why a call was refused before it reached the dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The circuit is open (or half-open with its probe already running).
    CircuitOpen,
    /// Every bulkhead slot stayed busy for the whole `acquire_timeout`.
    BulkheadFull,
}

impl Rejection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CircuitOpen => "circuit_open",
            Self::BulkheadFull => "bulkhead_full",
        }
    }
}

#[derive(Debug)]
struct Inner {
    state: State,
    consecutive_failures: u32,
    last_failure_at: Option<Instant>,
    opened_at: Option<Instant>,
    probe_in_flight: bool,
}

/// A circuit breaker and bulkhead for one dependency. Cheap to share: always used as `Arc<Guard>`.
#[derive(Debug)]
pub struct Guard {
    name: String,
    policy: Policy,
    inner: Mutex<Inner>,
    slots: Arc<Semaphore>,
    trips: AtomicU64,
    rejected_open: AtomicU64,
    rejected_bulkhead: AtomicU64,
}

/// Point-in-time view of a [`Guard`], for the health status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub name: String,
    pub state: State,
    pub consecutive_failures: u32,
    pub trips_total: u64,
    pub rejected_open_total: u64,
    pub rejected_bulkhead_total: u64,
    pub in_flight: usize,
    pub max_concurrent: usize,
    /// How long the circuit has been open (only while open or half-open).
    pub open_for: Option<Duration>,
}

impl Guard {
    pub fn new(name: impl Into<String>, policy: Policy) -> Arc<Self> {
        let guard = Arc::new(Self {
            name: name.into(),
            policy,
            inner: Mutex::new(Inner {
                state: State::Closed,
                consecutive_failures: 0,
                last_failure_at: None,
                opened_at: None,
                probe_in_flight: false,
            }),
            slots: Arc::new(Semaphore::new(policy.max_concurrent.max(1))),
            trips: AtomicU64::new(0),
            rejected_open: AtomicU64::new(0),
            rejected_bulkhead: AtomicU64::new(0),
        });
        guard.publish_state(State::Closed);
        metrics::gauge!("bulkhead_in_flight", "breaker" => guard.name.clone()).set(0.0);
        guard
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // The critical sections below never panic, but a poisoned lock must not take the
        // dependency down with it: the state is plain data, so carry on with it.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn publish_state(&self, state: State) {
        metrics::gauge!("circuit_breaker_state", "breaker" => self.name.clone()).set(state.gauge());
    }

    /// True when a call made now would be refused by the circuit: it is open and not yet due for a
    /// probe, or half-open with the probe already running. Callers with an alternative (a fallback
    /// provider) use this to skip straight to it instead of waiting for the refusal.
    pub fn would_reject(&self) -> bool {
        let inner = self.lock();
        match inner.state {
            State::Closed => false,
            State::HalfOpen => inner.probe_in_flight,
            State::Open => !self.open_elapsed(&inner),
        }
    }

    fn open_elapsed(&self, inner: &Inner) -> bool {
        inner
            .opened_at
            .is_none_or(|at| at.elapsed() >= self.policy.open_duration)
    }

    /// Ask to make a call. Returns a [`Permit`] to report the outcome with, or the reason the call
    /// was refused. Waits up to `acquire_timeout` for a bulkhead slot.
    pub async fn acquire(self: &Arc<Self>) -> Result<Permit, Rejection> {
        let probe = self.admit()?;

        let slot = match tokio::time::timeout(
            self.policy.acquire_timeout,
            Arc::clone(&self.slots).acquire_owned(),
        )
        .await
        {
            Ok(Ok(slot)) => slot,
            // The semaphore is never closed, so `Ok(Err(_))` cannot happen; treat it as full.
            Ok(Err(_)) | Err(_) => {
                if probe {
                    self.release_probe();
                }
                self.reject(Rejection::BulkheadFull);
                return Err(Rejection::BulkheadFull);
            }
        };

        self.publish_in_flight();
        Ok(Permit {
            guard: Arc::clone(self),
            slot: Some(slot),
            probe,
            done: false,
        })
    }

    /// Circuit check. `Ok(true)` means this call is the half-open probe.
    fn admit(&self) -> Result<bool, Rejection> {
        let mut inner = self.lock();
        match inner.state {
            State::Closed => Ok(false),
            State::Open => {
                if self.open_elapsed(&inner) {
                    inner.state = State::HalfOpen;
                    inner.probe_in_flight = true;
                    drop(inner);
                    self.publish_state(State::HalfOpen);
                    tracing::info!(breaker = %self.name, "circuit half-open: letting one probe call through");
                    Ok(true)
                } else {
                    drop(inner);
                    self.reject(Rejection::CircuitOpen);
                    Err(Rejection::CircuitOpen)
                }
            }
            State::HalfOpen => {
                if inner.probe_in_flight {
                    drop(inner);
                    self.reject(Rejection::CircuitOpen);
                    Err(Rejection::CircuitOpen)
                } else {
                    inner.probe_in_flight = true;
                    Ok(true)
                }
            }
        }
    }

    fn reject(&self, why: Rejection) {
        match why {
            Rejection::CircuitOpen => self.rejected_open.fetch_add(1, Ordering::Relaxed),
            Rejection::BulkheadFull => self.rejected_bulkhead.fetch_add(1, Ordering::Relaxed),
        };
        metrics::counter!(
            "circuit_breaker_rejections_total",
            "breaker" => self.name.clone(),
            "reason" => why.as_str(),
        )
        .increment(1);
    }

    fn release_probe(&self) {
        self.lock().probe_in_flight = false;
    }

    fn publish_in_flight(&self) {
        let in_flight = self.policy.max_concurrent.max(1) - self.slots.available_permits();
        metrics::gauge!("bulkhead_in_flight", "breaker" => self.name.clone()).set(in_flight as f64);
    }

    /// Record a successful call. Only the half-open probe may close an open circuit: a call that was
    /// already running when the circuit opened says nothing about the dependency now.
    fn record_success(&self, probe: bool) {
        let mut inner = self.lock();
        match inner.state {
            State::Closed => {
                inner.consecutive_failures = 0;
                inner.last_failure_at = None;
            }
            State::HalfOpen if probe => {
                inner.consecutive_failures = 0;
                inner.last_failure_at = None;
                inner.probe_in_flight = false;
                inner.state = State::Closed;
                inner.opened_at = None;
                drop(inner);
                self.publish_state(State::Closed);
                tracing::info!(breaker = %self.name, "circuit closed: the dependency answered again");
            }
            State::HalfOpen | State::Open => {}
        }
    }

    /// Record an unhealthy call. A failed probe opens the circuit again for a full period; a failure
    /// from a call that started before the circuit opened changes nothing.
    fn record_failure(&self, probe: bool) {
        let mut inner = self.lock();
        let now = Instant::now();

        let trip = match inner.state {
            State::HalfOpen if probe => {
                inner.probe_in_flight = false;
                true
            }
            State::HalfOpen | State::Open => false,
            State::Closed => {
                let stale = inner
                    .last_failure_at
                    .is_some_and(|at| now.duration_since(at) > self.policy.failure_window);
                inner.consecutive_failures = if stale {
                    1
                } else {
                    inner.consecutive_failures.saturating_add(1)
                };
                inner.last_failure_at = Some(now);
                inner.consecutive_failures >= self.policy.failure_threshold
            }
        };

        if trip {
            inner.state = State::Open;
            inner.opened_at = Some(now);
            let failures = inner.consecutive_failures;
            drop(inner);
            self.trips.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("circuit_breaker_trips_total", "breaker" => self.name.clone())
                .increment(1);
            self.publish_state(State::Open);
            tracing::warn!(
                breaker = %self.name,
                consecutive_failures = failures,
                open_for_secs = self.policy.open_duration.as_secs(),
                "circuit opened: calls are refused until the next probe"
            );
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.lock();
        Snapshot {
            name: self.name.clone(),
            state: inner.state,
            consecutive_failures: inner.consecutive_failures,
            trips_total: self.trips.load(Ordering::Relaxed),
            rejected_open_total: self.rejected_open.load(Ordering::Relaxed),
            rejected_bulkhead_total: self.rejected_bulkhead.load(Ordering::Relaxed),
            in_flight: self.policy.max_concurrent.max(1) - self.slots.available_permits(),
            max_concurrent: self.policy.max_concurrent.max(1),
            open_for: inner.opened_at.map(|at| at.elapsed()),
        }
    }
}

/// Permission to make one call. Report the outcome with [`success`](Self::success) or
/// [`failure`](Self::failure); dropping it frees the bulkhead slot.
#[derive(Debug)]
pub struct Permit {
    guard: Arc<Guard>,
    slot: Option<OwnedSemaphorePermit>,
    probe: bool,
    done: bool,
}

impl Permit {
    /// The dependency answered (including a refusal that is not its fault, such as a 401 or 429).
    /// Closes a half-open circuit and clears the failure streak. The first outcome wins.
    pub fn success(&mut self) {
        if !self.done {
            self.done = true;
            self.guard.record_success(self.probe);
        }
    }

    /// The dependency is unhealthy (5xx, timeout, connection error). The first outcome wins.
    pub fn failure(&mut self) {
        if !self.done {
            self.done = true;
            self.guard.record_failure(self.probe);
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if !self.done && self.probe {
            self.guard.release_probe();
        }
        self.slot.take();
        self.guard.publish_in_flight();
    }
}

/// The set of guards in one process, by name. The platform health view reads it.
#[derive(Debug, Default)]
pub struct Hub {
    guards: Mutex<BTreeMap<String, Arc<Guard>>>,
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The guard called `name`, created with `policy` the first time. A later call with a
    /// different policy returns the existing guard unchanged.
    pub fn guard(&self, name: &str, policy: Policy) -> Arc<Guard> {
        let mut guards = self.guards.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            guards
                .entry(name.to_owned())
                .or_insert_with(|| Guard::new(name, policy)),
        )
    }

    pub fn get(&self, name: &str) -> Option<Arc<Guard>> {
        self.guards
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    /// Every guard, sorted by name.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let guards: Vec<Arc<Guard>> = self
            .guards
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        guards.iter().map(|g| g.snapshot()).collect()
    }
}

/// Register the metric descriptions. Call once at startup, after the recorder is installed.
pub fn describe_metrics() {
    metrics::describe_gauge!(
        "circuit_breaker_state",
        "Circuit breaker state per dependency (0 = closed, 1 = half-open, 2 = open)"
    );
    metrics::describe_counter!(
        "circuit_breaker_trips_total",
        "Times a circuit breaker opened"
    );
    metrics::describe_counter!(
        "circuit_breaker_rejections_total",
        "Calls refused before reaching the dependency (reason: circuit_open or bulkhead_full)"
    );
    metrics::describe_gauge!(
        "bulkhead_in_flight",
        "Calls in flight to a dependency (bulkhead occupancy)"
    );
}

#[cfg(test)]
mod tests;
