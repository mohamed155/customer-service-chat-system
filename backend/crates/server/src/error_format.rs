//! Chooses the format of an error response (ADR-012, gap A2, MOH-197).
//!
//! Every error comes from [`kernel::ApiError`], which renders the legacy envelope and leaves its
//! parts on the response. This layer sees the request, so it decides:
//!
//! * the request asks for `application/problem+json` (see [`kernel::prefers_problem_json`]): the
//!   body is rewritten as an RFC 9457 document, `type`, `title`, `status`, `detail`, `instance`
//!   (the request path), `code`, `request_id` and `errors[]`;
//! * everyone else keeps the legacy envelope, byte for byte, and gets `Deprecation` and a `Link`
//!   to the migration note, and the counter `error_envelope_legacy_total{route}` records it, so the
//!   removal step can see when nobody uses it any more.
//!
//! It sits outside `internal_error_middleware`, so it sees the finished response (the 5xx cause
//! already logged and dropped, the request ID filled in) and a 5xx body is as free of internal
//! detail in one format as in the other.

use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use kernel::{
    prefers_problem_json, ErrorResponseParts, LEGACY_DEPRECATION, LEGACY_DEPRECATION_LINK,
    PROBLEM_JSON,
};
use observability::request_id::REQUEST_ID_HEADER;

pub async fn error_format_middleware(request: Request, next: Next) -> Response {
    let wants_problem = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(prefers_problem_json);
    let instance = request.uri().path().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
        .unwrap_or_else(|| "<unmatched>".to_owned());
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let mut response = next.run(request).await;
    let Some(parts) = response.extensions().get::<ErrorResponseParts>().cloned() else {
        return response;
    };

    // The answer depends on `Accept`, whichever format it takes.
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("accept"));

    if !wants_problem {
        let headers = response.headers_mut();
        headers.insert("deprecation", HeaderValue::from_static(LEGACY_DEPRECATION));
        headers.append(
            header::LINK,
            HeaderValue::from_static(LEGACY_DEPRECATION_LINK),
        );
        observability::metrics::record_legacy_error_envelope(&route);
        return response;
    }

    let problem = parts.to_problem(&instance, &request_id);
    let Ok(body) = serde_json::to_vec(&problem) else {
        // A struct of strings, numbers and JSON values always serialises; if it somehow does not,
        // the legacy body is still a correct answer.
        return response;
    };
    let (mut head, _) = response.into_parts();
    head.headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
    head.headers
        .insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
    Response::from_parts(head, Body::from(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal_error::internal_error_middleware;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::middleware::from_fn;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::Router;
    use http_body_util::BodyExt;
    use kernel::{ApiError, ErrorDetail};
    use serde_json::Value;
    use std::time::Duration;
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route(
                "/invalid",
                get(|| async {
                    ApiError::unprocessable_entity("Validation failed").with_details(vec![
                        ErrorDetail {
                            field: "email".into(),
                            code: "invalid_format".into(),
                            message: "Not an email address".into(),
                        },
                    ])
                }),
            )
            .route(
                "/slow-down",
                get(|| async {
                    ApiError::rate_limited("Too many attempts", Duration::from_secs(42))
                }),
            )
            .route(
                "/hidden/{id}",
                get(|| async { ApiError::not_found("Workspace not found") }),
            )
            .route(
                "/boom",
                get(|| async {
                    ApiError::internal_error_from("Insert failed", "password=hunter2 on db-7")
                }),
            )
            .route("/ok", get(|| async { "fine".into_response() }))
            .layer(from_fn(internal_error_middleware))
            .layer(from_fn(error_format_middleware))
    }

    async fn get_with(
        uri: &str,
        accept: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut request = HttpRequest::builder()
            .uri(uri)
            .header("x-request-id", "req_test");
        if let Some(accept) = accept {
            request = request.header("accept", accept);
        }
        let response = app()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, headers, body)
    }

    const PROBLEM: &str = "application/problem+json, application/json;q=0.5";

    #[tokio::test]
    async fn problem_json_is_returned_to_a_client_that_asks_for_it() {
        let (status, headers, body) = get_with("/invalid?token=secret", Some(PROBLEM)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(headers[header::CONTENT_TYPE], PROBLEM_JSON);
        assert_eq!(headers[header::CONTENT_LENGTH], body.len().to_string());
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["type"], "urn:problem:validation_failed");
        assert_eq!(json["title"], "Validation failed");
        assert_eq!(json["status"], 422);
        assert_eq!(json["detail"], "Validation failed");
        assert_eq!(
            json["instance"], "/invalid",
            "the path, never the query string"
        );
        assert_eq!(json["code"], "validation_failed");
        assert_eq!(json["request_id"], "req_test");
        assert_eq!(
            json["errors"],
            serde_json::json!([{"field": "email", "code": "invalid_format"}])
        );
        // Not the legacy envelope, and not marked deprecated.
        assert!(json.get("error").is_none());
        assert!(headers.get("deprecation").is_none() && headers.get(header::LINK).is_none());
        assert_eq!(headers[header::VARY], "accept");
    }

    #[tokio::test]
    async fn everyone_else_keeps_the_legacy_envelope_byte_for_byte() {
        let expected = {
            // What the handler produces without this layer.
            let response = ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![ErrorDetail {
                    field: "email".into(),
                    code: "invalid_format".into(),
                    message: "Not an email address".into(),
                }])
                .into_response();
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec()
        };
        for accept in [
            None,
            Some("application/json"),
            Some("*/*"),
            Some("text/html"),
            Some("application/json, application/problem+json;q=0.5"),
        ] {
            let (status, headers, body) = get_with("/invalid", accept).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{accept:?}");
            assert_eq!(
                body, expected,
                "the legacy body changed for Accept {accept:?}"
            );
            assert_eq!(headers[header::CONTENT_TYPE], "application/json");
            // ...and says it is going away.
            assert_eq!(headers["deprecation"], LEGACY_DEPRECATION);
            assert_eq!(headers[header::LINK], LEGACY_DEPRECATION_LINK);
            assert_eq!(headers[header::VARY], "accept");
        }
    }

    #[tokio::test]
    async fn retry_after_survives_in_both_formats() {
        for accept in [None, Some(PROBLEM)] {
            let (status, headers, body) = get_with("/slow-down", accept).await;
            assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(headers[header::RETRY_AFTER], "42", "{accept:?}");
            let json: Value = serde_json::from_slice(&body).unwrap();
            let code = if accept.is_some() {
                &json["code"]
            } else {
                &json["error"]["code"]
            };
            assert_eq!(code, "rate_limited");
        }
    }

    #[tokio::test]
    async fn a_workspace_the_caller_cannot_see_is_a_404_in_both_formats() {
        let (status, _, body) = get_with("/hidden/abc", Some(PROBLEM)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "not_found");
        assert_eq!(json["instance"], "/hidden/abc");
        let (status, _, body) = get_with("/hidden/abc", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"],
            "not_found"
        );
    }

    #[tokio::test]
    async fn a_server_error_hides_its_cause_in_both_formats() {
        for accept in [None, Some(PROBLEM)] {
            let (status, _, body) = get_with("/boom", accept).await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            let text = String::from_utf8(body.clone()).unwrap();
            for secret in ["password", "hunter2", "db-7", "Insert failed"] {
                assert!(
                    !text.contains(secret),
                    "{secret} leaked ({accept:?}): {text}"
                );
            }
            let json: Value = serde_json::from_slice(&body).unwrap();
            if accept.is_some() {
                assert_eq!(json["detail"], kernel::INTERNAL_ERROR_MESSAGE);
                assert_eq!(json["request_id"], "req_test");
                assert_eq!(json["errors"], serde_json::json!([]));
            } else {
                assert_eq!(json["error"]["message"], kernel::INTERNAL_ERROR_MESSAGE);
                assert_eq!(json["error"]["request_id"], "req_test");
            }
        }
    }

    #[tokio::test]
    async fn a_success_is_left_alone() {
        for accept in [None, Some(PROBLEM)] {
            let (status, headers, body) = get_with("/ok", accept).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body, b"fine");
            assert!(headers.get("deprecation").is_none() && headers.get(header::VARY).is_none());
        }
    }
}
