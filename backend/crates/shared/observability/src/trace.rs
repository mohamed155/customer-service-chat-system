use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::Instrument;

pub async fn trace_middleware(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let request_id = request
        .headers()
        .get(&super::request_id::REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let start = Instant::now();
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %method,
        path = %path,
        principal.id = tracing::field::Empty,
        principal.kind = tracing::field::Empty,
        tenant.id = tracing::field::Empty,
    );

    let response = next.run(request).instrument(span).await;

    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    tracing::info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        status = status,
        latency_ms = latency_ms,
        "request completed"
    );

    response
}
