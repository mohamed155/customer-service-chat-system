use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

pub fn init_observability() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .try_init();
}

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let header = HeaderName::from_static("x-request-id");
    let request_id = request
        .headers()
        .get(&header)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("req_{}", uuid::Uuid::now_v7()));
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        request.headers_mut().insert(header.clone(), value);
    }
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(header, value);
    }
    response
}

/// Replaces a sensitive value before it is recorded in a tracing field.
/// Call this at the log source for passwords, tokens, authorization headers,
/// message content, provider prompts, and customer PII.
pub fn redact(_sensitive_value: &str) -> &'static str {
    "[REDACTED]"
}

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
}
pub async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}
pub async fn ready() -> Json<Health> {
    Json(Health { status: "ok" })
}
pub async fn metrics() -> impl IntoResponse {
    (
        [("content-type", "text/plain; charset=utf-8")],
        "# no metrics yet\n",
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn sensitive_values_are_redacted() {
        assert_eq!(super::redact("secret"), "[REDACTED]");
    }
}
