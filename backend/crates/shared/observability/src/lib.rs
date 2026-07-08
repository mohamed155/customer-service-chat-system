//! Observability — logging, tracing, health probes, and request IDs
//!
//! # Purpose
//! Centralise all observability concerns: structured log initialisation,
//! per-request trace spans, request-ID generation/validation, and the
//! readiness health-check system.
//!
//! # Public Interfaces
//! - `init_observability(LogFormat)` — global subscriber init
//! - `request_id::generate()`, `validate()`, `request_id_middleware`
//! - `trace::trace_middleware`
//! - `health::liveness`, `health::readiness`, `health::HealthCheck` trait
//! - `metrics()` (stub)
//! - `redact(&str) -> &str`
//!
//! # Dependencies
//! - `axum`, `tracing`, `tracing-subscriber`, `uuid`, `config`
//!
//! # Extension Points
//! - New `HealthCheck` implementations in other crates.
//! - OpenTelemetry export layer can be added without modifying middleware.

pub mod health;
pub mod request_id;
pub mod trace;

use config::LogFormat;
use tracing_subscriber::EnvFilter;

pub fn init_observability(log_format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let _ = match log_format {
        LogFormat::Json => builder.json().try_init(),
        LogFormat::Pretty => builder.pretty().try_init(),
    };
}

pub use health::liveness;
pub use health::readiness;

pub async fn metrics() -> impl axum::response::IntoResponse {
    (
        [("content-type", "text/plain; charset=utf-8")],
        "# no metrics yet\n",
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
