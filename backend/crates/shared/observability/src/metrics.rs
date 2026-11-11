use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::LazyLock;
use std::time::Instant;

static PROMETHEUS_HANDLE: LazyLock<PrometheusHandle> = LazyLock::new(|| {
    PrometheusBuilder::new()
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
    metrics::describe_gauge!("outbox_backlog", "Number of pending outbox messages");
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

pub fn set_outbox_backlog(count: i64) {
    metrics::gauge!("outbox_backlog", "queue" => "outbox").set(count as f64);
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
