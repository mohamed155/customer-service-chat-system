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
        let tenant_id = tenant
            .map(|tenant| tenant.0.to_string())
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
