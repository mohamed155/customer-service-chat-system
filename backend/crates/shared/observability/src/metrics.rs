use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::sync::LazyLock;
use std::time::Instant;

/// Upper bounds (seconds) of the request-duration buckets. They bracket the 300 ms API budget
/// and the 0.5 s `HighLatency` threshold, so `histogram_quantile` has data on both sides.
const REQUEST_DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

// Runs once, when the first `/metrics` or recorder user asks for the handle at process start: a
// recorder that cannot be installed (or constant buckets that are invalid) is a startup failure,
// not a request-path one.
#[allow(clippy::expect_used)]
static PROMETHEUS_HANDLE: LazyLock<PrometheusHandle> = LazyLock::new(|| {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_owned()),
            &REQUEST_DURATION_BUCKETS,
        )
        .expect("request duration buckets are valid")
        .install_recorder()
        .expect("failed to install prometheus recorder")
});

pub fn prometheus_handle() -> &'static PrometheusHandle {
    &PROMETHEUS_HANDLE
}

use std::sync::Arc;

use crate::health::{readiness, HealthCheck};

/// Middleware that records `http_requests_total` and
/// `http_request_duration_seconds` for every request.
///
/// Route label uses the URI path. Status label uses the response status code.
/// No tenant/identity labels are recorded (FR-019).
pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned())
        .unwrap_or_else(|| "<unmatched>".to_owned());
    let start = Instant::now();

    let response = next.run(request).await;

    let status = response.status().as_u16();
    let duration = start.elapsed().as_secs_f64();

    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "route" => route,
    )
    .record(duration);

    response
}

pub fn init() {
    LazyLock::force(&PROMETHEUS_HANDLE);
    metrics::describe_counter!(
        "http_requests_total",
        "Total number of HTTP requests received"
    );
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );
    metrics::describe_counter!(
        "rate_limit_refusals_total",
        "Total number of rate limit refusals"
    );
    metrics::describe_counter!(
        "rate_limit_degraded_total",
        "Total number of rate limit degradations"
    );
    metrics::describe_counter!(
        "error_envelope_legacy_total",
        "Error responses sent in the legacy envelope format instead of problem+json, by route \
         (ADR-012 gap A2); the envelope is removed once this stays at zero"
    );
    metrics::describe_counter!(
        "frontend_errors_total",
        "Errors reported by the dashboard in users' browsers (MOH-168), by kind and by the \
         release of the backend that received them"
    );
    metrics::describe_gauge!("outbox_backlog", "Number of pending outbox messages");
    metrics::describe_gauge!(
        "outbox_oldest_age_seconds",
        "Age in seconds of the oldest pending outbox message"
    );
    metrics::describe_gauge!(
        "dependency_up",
        "Whether a dependency is reachable (1 = up, 0 = down)"
    );
    resilience::describe_metrics();
    metrics::describe_gauge!(
        "backup_last_success_timestamp_seconds",
        "Unix timestamp of last successful backup"
    );
}

/// Count browser errors reported by the dashboard. `kind` is one of a closed set and `release` is
/// the receiving backend's own release, never a value from the client, so the number of series is
/// bounded (no tenant, user or client-chosen label).
pub fn record_frontend_errors(kind: &'static str, release: &str, count: u64) {
    metrics::counter!(
        "frontend_errors_total",
        "kind" => kind,
        "release" => release.to_owned(),
    )
    .increment(count);
}

/// Count an error answered in the legacy envelope format (ADR-012, gap A2). `route` is the matched
/// route pattern (`/api/v1/tenant/members/{id}`), never the concrete path, and there is no tenant,
/// user or client label, so the number of series is bounded by the number of routes. The envelope
/// is removed once this has shown nothing for two weeks across a release cycle.
pub fn record_legacy_error_envelope(route: &str) {
    metrics::counter!("error_envelope_legacy_total", "route" => route.to_owned()).increment(1);
}

pub fn set_outbox_backlog(count: i64) {
    metrics::gauge!("outbox_backlog", "queue" => "outbox").set(count as f64);
}

pub fn set_password_reset_queue_health(count: i64, age_seconds: f64) {
    metrics::gauge!("outbox_backlog", "queue" => "password_reset").set(count as f64);
    metrics::gauge!("outbox_oldest_age_seconds", "queue" => "password_reset").set(age_seconds);
}

pub fn set_login_lockout_queue_health(count: i64, age_seconds: f64) {
    metrics::gauge!("outbox_backlog", "queue" => "login_lockout").set(count as f64);
    metrics::gauge!("outbox_oldest_age_seconds", "queue" => "login_lockout").set(age_seconds);
}

/// Spawn a background task that periodically runs the health checks and
/// sets `dependency_up{ dependency="<name>" }` to 1 (pass) or 0 (fail).
///
/// Uses the existing readiness path — no new probes are written.
pub fn spawn_dependency_updater(checks: Vec<Arc<dyn HealthCheck>>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let report = readiness(checks.clone(), std::time::Duration::from_secs(5)).await;
            for check in &report.checks {
                metrics::gauge!("dependency_up", "dependency" => check.name.clone()).set(
                    if check.status == crate::health::CheckStatus::Ok {
                        1.0
                    } else {
                        0.0
                    },
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The latency alert (`HighLatency`) and the dashboard use `histogram_quantile` over
    /// `http_request_duration_seconds_bucket`. An exporter left on its default renders a summary
    /// (quantiles, no buckets), so the alert could never fire and the panels stayed empty.
    #[test]
    fn request_duration_is_exposed_as_a_histogram() {
        init();
        metrics::histogram!(
            "http_request_duration_seconds",
            "method" => "GET",
            "route" => "/histogram-test",
        )
        .record(0.12);

        let body = prometheus_handle().render();

        assert!(
            body.contains("http_request_duration_seconds_bucket{method=\"GET\",route=\"/histogram-test\",le=\"0.25\"} 1"),
            "request duration must have `_bucket` series, got:\n{body}"
        );
        assert!(
            !body.contains(
                "http_request_duration_seconds{method=\"GET\",route=\"/histogram-test\",quantile"
            ),
            "request duration must not be a summary"
        );
    }
}
