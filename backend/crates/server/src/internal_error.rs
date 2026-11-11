//! Keeps the detail of a server error out of the response and in the log (ADR-012, gap A1).
//!
//! `ApiError::internal_error` / `internal_error_from` already return a fixed public message and
//! park the real cause in the response extensions. This middleware sits inside the request-ID and
//! trace layers, so for every 5xx it can:
//!
//! * log the cause once, with the same `request_id` the client receives and the tenant ID when the
//!   request had one, and
//! * put that `request_id` into the JSON error body, which the handlers can't do because they
//!   don't know it.
//!
//! The log carries the context string and the text of the underlying error, never the request
//! body, headers or cookies.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use kernel::{ErrorEnvelope, InternalErrorDetail, ResponseTenant};
use observability::request_id::REQUEST_ID_HEADER;

/// Largest 5xx body this middleware will read back. Error envelopes are a few hundred bytes.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

pub async fn internal_error_middleware(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let response = next.run(request).await;
    if !response.status().is_server_error() {
        return response;
    }

    let detail = response.extensions().get::<InternalErrorDetail>().cloned();
    let tenant = response.extensions().get::<ResponseTenant>().copied();
    let status = response.status();

    if let Some(detail) = &detail {
        let tenant_id = detail
            .tenant_id
            .or(tenant.map(|tenant| tenant.0))
            .map(|tenant| tenant.to_string())
            .unwrap_or_else(|| "none".to_owned());
        tracing::error!(
            request_id = %request_id,
            tenant_id = %tenant_id,
            status = status.as_u16(),
            context = detail.context,
            error = detail.source.as_deref().unwrap_or(""),
            "request failed with a server error"
        );
    }

    with_request_id(response, &request_id).await
}

/// Fill `error.request_id` in a JSON error envelope that doesn't have one yet. Anything that is not
/// an envelope (a health probe's 503, for instance) is returned as it was.
async fn with_request_id(response: Response, request_id: &str) -> Response {
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_ERROR_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            // The body was larger than any error envelope; send the status without it.
            parts.headers.remove(header::CONTENT_LENGTH);
            return Response::from_parts(parts, Body::empty());
        }
    };

    let rewritten = serde_json::from_slice::<ErrorEnvelope>(&bytes)
        .ok()
        .filter(|envelope| envelope.error.request_id.is_empty())
        .and_then(|mut envelope| {
            envelope.error.request_id = request_id.to_owned();
            serde_json::to_vec(&envelope).ok()
        });

    match rewritten {
        Some(new_bytes) => {
            parts
                .headers
                .insert(header::CONTENT_LENGTH, HeaderValue::from(new_bytes.len()));
            Response::from_parts(parts, Body::from(new_bytes))
        }
        None => Response::from_parts(parts, Body::from(bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware::from_fn;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use kernel::ApiError;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;
    use uuid::Uuid;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Capture;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    async fn run(router: Router, uri: &str) -> (StatusCode, String, String) {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let response = router
            .layer(from_fn(internal_error_middleware))
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("x-request-id", "req_test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap(), logged)
    }

    /// Public routes that learn the tenant from a token tag the error themselves; the log must
    /// name that tenant, not "none".
    #[tokio::test]
    async fn error_tagged_with_for_tenant_logs_that_tenant() {
        let tenant = Uuid::now_v7();
        let router = Router::new().route(
            "/fail",
            get(move || async move {
                ApiError::internal_error_from("Transaction commit failed", "connection reset")
                    .for_tenant(tenant)
                    .into_response()
            }),
        );
        let (status, body, logged) = run(router, "/fail").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body.contains("\"request_id\":\"req_test\""), "{body}");
        assert!(!body.contains("connection reset"), "{body}");
        assert!(logged.contains(&tenant.to_string()), "{logged}");
        assert!(logged.contains("connection reset"), "{logged}");
    }

    #[tokio::test]
    async fn error_without_a_tenant_logs_none() {
        let router = Router::new().route(
            "/fail",
            get(|| async { ApiError::internal_error("Failed to load").into_response() }),
        );
        let (_, _, logged) = run(router, "/fail").await;
        assert!(logged.contains("tenant_id=none"), "{logged}");
        assert!(logged.contains("Failed to load"), "{logged}");
    }

    #[tokio::test]
    async fn response_tenant_extension_is_used_when_the_error_has_none() {
        let tenant = Uuid::now_v7();
        let router = Router::new().route(
            "/fail",
            get(move || async move {
                let mut response = ApiError::internal_error("Failed to load").into_response();
                response
                    .extensions_mut()
                    .insert(kernel::ResponseTenant(tenant));
                response
            }),
        );
        let (_, _, logged) = run(router, "/fail").await;
        assert!(logged.contains(&tenant.to_string()), "{logged}");
    }

    #[tokio::test]
    async fn non_envelope_5xx_and_4xx_pass_through_unchanged() {
        let router = Router::new()
            .route(
                "/probe",
                get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "not ready") }),
            )
            .route(
                "/bad",
                get(|| async { ApiError::validation_failed("Bad input").into_response() }),
            );
        let (status, body, _) = run(router.clone(), "/probe").await;
        assert_eq!(
            (status, body.as_str()),
            (StatusCode::SERVICE_UNAVAILABLE, "not ready")
        );
        let (status, body, _) = run(router, "/bad").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body.contains("Bad input") && body.contains("\"request_id\":\"\""),
            "{body}"
        );
    }
}
