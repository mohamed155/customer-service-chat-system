//! Observability — logging, tracing, health probes, metrics, error capture
//!
//! # Purpose
//! Centralise all observability concerns: structured log initialisation,
//! per-request trace spans, request-ID generation/validation, readiness
//! health-check system, Prometheus metrics exposition (7 series including
//! `http_requests_total`, `http_request_duration_seconds`, `dependency_up`,
//! `rate_limit_refusals_total`, `rate_limit_degraded_total`), OTLP trace
//! export to an OpenTelemetry collector, and Sentry error capture with
//! PII scrubbing.
//!
//! # Public Interfaces
//! - `init_observability(LogFormat, otlp_endpoint, trace_sample_ratio, sentry_dsn, release_sha)`
//! - `request_id::generate()`, `validate()`, `request_id_middleware`
//! - `trace::trace_middleware`
//! - `metrics::metrics_middleware` — records http_requests_total + duration_seconds
//! - `metrics::spawn_dependency_updater` — populates dependency_up gauge
//! - `health::liveness`, `health::readiness`, `health::HealthCheck` trait
//! - `metrics()` — Prometheus text exposition (replaced stub)
//! - `otlp::build_layer` — OTLP/gRPC tracing layer
//! - `errors::init` — Sentry capture with before_send scrubber
//! - `redact(&str) -> &str`
//!
//! # Dependencies
//! - `axum`, `tracing`, `tracing-subscriber`, `uuid`, `config`
//! - `metrics`, `metrics-exporter-prometheus`, `opentelemetry`, `tracing-opentelemetry`
//! - `sentry`, `sentry-tracing`
//!
//! # Extension Points
//! - New `HealthCheck` implementations in other crates.
//! - Additional Prometheus metric series can be registered in `metrics::init`.

pub mod errors;
pub mod health;
pub mod metrics;
pub mod otlp;
pub mod request_id;
pub mod trace;

use axum::response::Json;
use config::LogFormat;
use serde_json::json;
use tracing_subscriber::EnvFilter;

pub fn init_observability(
    log_format: LogFormat,
    otlp_endpoint: Option<String>,
    trace_sample_ratio: f64,
    sentry_dsn: Option<String>,
    release_sha: Option<String>,
) {
    metrics::init();
    errors::init(sentry_dsn.as_deref(), release_sha.as_deref());
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if let Some(endpoint) = otlp_endpoint {
        use tracing_subscriber::layer::{Layer, SubscriberExt};
        use tracing_subscriber::util::SubscriberInitExt;
        use tracing_subscriber::Registry;

        let otlp_layer = otlp::build_layer(&endpoint, trace_sample_ratio).with_filter(filter.clone());
        let sentry_layer = sentry_tracing::layer().event_filter(|md| match *md.level() {
            tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
            tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb,
            _ => sentry_tracing::EventFilter::Ignore,
        });
        if matches!(log_format, LogFormat::Json) {
            let _ = Registry::default()
                .with(otlp_layer)
                .with(sentry_layer)
                .with(tracing_subscriber::fmt::layer().json().with_filter(filter))
                .try_init();
        } else {
            let _ = Registry::default()
                .with(otlp_layer)
                .with(sentry_layer)
                .with(
                    tracing_subscriber::fmt::layer()
                        .pretty()
                        .with_filter(filter),
                )
                .try_init();
        }
    } else {
        use tracing_subscriber::layer::{Layer, SubscriberExt};
        use tracing_subscriber::util::SubscriberInitExt;
        use tracing_subscriber::Registry;

        match log_format {
            LogFormat::Json => {
                let _ = Registry::default()
                    .with(tracing_subscriber::fmt::layer().json().with_filter(filter))
                    .with(
                        sentry_tracing::layer().event_filter(|md| match *md.level() {
                            tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
                            tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb,
                            _ => sentry_tracing::EventFilter::Ignore,
                        }),
                    )
                    .try_init();
            }
            LogFormat::Pretty => {
                let _ = Registry::default()
                    .with(
                        tracing_subscriber::fmt::layer()
                            .pretty()
                            .with_filter(filter),
                    )
                    .with(
                        sentry_tracing::layer().event_filter(|md| match *md.level() {
                            tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
                            tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb,
                            _ => sentry_tracing::EventFilter::Ignore,
                        }),
                    )
                    .try_init();
            }
        }
    }
}

pub use health::readiness;

/// Liveness probe. Returns `{"status": "ok"}` with HTTP 200 while the process
/// is running. Used by orchestrators to decide when to restart a process.
#[utoipa::path(
    get,
    path = "/health",
    tag = "ops",
    responses(
        (status = 200, description = "Process is alive", body = serde_json::Value,
         example = json!({"status": "ok"})),
    )
)]
pub async fn liveness() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

/// Metrics endpoint — Prometheus text exposition.
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "ops",
    responses(
        (status = 200, description = "Prometheus text exposition format (text/plain; charset=utf-8)", content_type = "text/plain; charset=utf-8", body = String),
    )
)]
pub async fn metrics() -> impl axum::response::IntoResponse {
    (
        [("content-type", "text/plain; charset=utf-8")],
        metrics::prometheus_handle().render(),
    )
}

/// Replaces a sensitive value before it is recorded in a tracing field.
/// Call this at the log source for passwords, tokens, authorization headers,
/// message content, provider prompts, and customer PII.
pub fn redact(_sensitive_value: &str) -> &'static str {
    "[REDACTED]"
}

#[cfg(test)]
mod tests {
    #[test]
    fn sensitive_values_are_redacted() {
        assert_eq!(super::redact("secret"), "[REDACTED]");
    }
}
