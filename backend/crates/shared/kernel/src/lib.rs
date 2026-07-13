//! Kernel — shared domain primitives
//!
//! # Purpose
//! Types that cross every module boundary: error envelope, pagination, JSON
//! extraction, idempotency. No business logic.
//!
//! # Public Interfaces
//! - `ApiError` constructors + `IntoResponse`
//! - `ApiJson<T>` extractor
//! - `PageParams`, `Page<T>`
//! - Idempotency middleware
//!
//! # Dependencies
//! - `axum`, `serde`, `uuid`
//!
//! # Extension Points
//! - Add new `ApiError` constructors as the status map grows.

mod idempotency;

use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json;

pub use idempotency::{
    idempotency_middleware, CachedResponse, IdempotencyKey, IdempotencyStore,
    InMemoryIdempotencyStore,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorDetail {
    pub field: String,
    pub code: String,
    pub message: String,
}

impl From<ErrorDetail> for serde_json::Value {
    fn from(detail: ErrorDetail) -> Self {
        serde_json::to_value(detail).expect("ErrorDetail is always serializable")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub details: Vec<serde_json::Value>,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    envelope: ErrorEnvelope,
}

impl ApiError {
    fn new_with_code(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            envelope: ErrorEnvelope {
                error: ErrorBody {
                    code: code.to_owned(),
                    message: message.into(),
                    details: Vec::new(),
                    request_id: String::new(),
                },
            },
        }
    }

    pub fn validation_failed(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::BAD_REQUEST, "validation_failed", message)
    }

    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::UNAUTHORIZED, "unauthenticated", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::FORBIDDEN, "unauthorized", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::CONFLICT, "conflict", message)
    }

    /// `gone` — HTTP 410 with code `gone`.
    ///
    /// Used when a resource was valid but is no longer available and will not
    /// become available again at this URL (e.g. an expired or single-use
    /// invitation token that has already been consumed).
    pub fn gone(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::GONE, "gone", message)
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::UNPROCESSABLE_ENTITY, "unprocessable", message)
    }

    /// `unprocessable_entity` — HTTP 422 with code `validation_failed`.
    ///
    /// Used when a request is syntactically well-formed (so JSON extraction
    /// succeeds) but fails semantic validation.  The status matches the
    /// contract for per-field validation failures (see
    /// `specs/010-platform-tenant-management/contracts/rest-api.md`); the
    /// code `validation_failed` aligns with the rest of the platform's error
    /// vocabulary.
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new_with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            message,
        )
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    /// Attach structured details. Accepts any iterable whose items can be
    /// converted into `serde_json::Value` (e.g. `Vec<ErrorDetail>` or
    /// `Vec<serde_json::Value>`).
    pub fn with_details<I, V>(mut self, details: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<serde_json::Value>,
    {
        self.envelope.error.details = details.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.envelope.error.request_id = request_id.into();
        self
    }

    pub fn details(&self) -> &[serde_json::Value] {
        &self.envelope.error.details
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.envelope)).into_response()
    }
}

pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = match axum::body::Bytes::from_request(req, state).await {
            Ok(b) => b,
            Err(_) => return Err(ApiError::validation_failed("Invalid request body")),
        };
        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(classify_serde_error(error)),
        }
    }
}

/// `classify_serde_error` — turn a Serde JSON deserialization failure into
/// either a 400 `validation_failed` (body is not even valid JSON) or a 422
/// `validation_failed` with per-field `ErrorDetail` (body is well-formed but
/// the type/shape does not match the contract).
///
/// The two cases are deliberately distinct: malformed JSON is a transport-
/// layer problem, while shape errors are a contract violation the UI needs
/// to surface. `contracts/rest-api.md` requires 422 for the latter so the
/// caller can read the offending field name out of `details`.
fn classify_serde_error(error: serde_json::Error) -> ApiError {
    if error.is_syntax() || error.is_eof() {
        return ApiError::validation_failed("Request body is not valid JSON");
    }

    // The body is well-formed JSON.  Pull the field name (if any) out of the
    // Serde error message so the 422 envelope can name the offending field.
    // `deny_unknown_fields` produces "unknown field `foo`, expected ..." and
    // missing/invalid-value errors include the path before the colon.
    let message = error.to_string();
    let field = extract_field_from_serde_error(&message);

    let detail = ErrorDetail {
        field: field.unwrap_or_else(|| "<root>".to_string()),
        code: "invalid_value".to_string(),
        message: humanize_serde_error(&message),
    };
    ApiError::unprocessable_entity("Validation failed").with_details(vec![detail])
}

/// Pull a field name out of a Serde error message when one is present.
///
/// Known Serde message shapes we recognise:
///   * `unknown field `foo`, expected ...`            → `foo`
///   * `missing field `name``                          → `name`
///   * `invalid type: integer ..., expected a string`  → empty (root)
///   * `invalid value: ...`                            → empty (root)
///
/// The shape we return is the field NAME, not a path — for the tenant-
/// management payloads the rejected fields are always at the top level,
/// so a name is what the UI's per-control error mapping expects.
fn extract_field_from_serde_error(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix("unknown field `") {
        if let Some(end) = rest.find('`') {
            return Some(rest[..end].to_string());
        }
    }
    if let Some(rest) = message.strip_prefix("missing field `") {
        if let Some(end) = rest.find('`') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Convert the raw Serde error text into a concise human-readable message
/// the UI can display verbatim.  We strip the line/column tail Serde appends
/// (" at line N column M") so the message reads cleanly in a form.
fn humanize_serde_error(message: &str) -> String {
    let trimmed = match message.find(" at line ") {
        Some(idx) => &message[..idx],
        None => message,
    };
    trimmed.trim().to_string()
}

#[cfg(test)]
mod extractor_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn run<T>(body: &'static str) -> (u16, serde_json::Value)
    where
        T: DeserializeOwned + Send + 'static,
        ApiJson<T>: FromRequest<(), Rejection = ApiError>,
    {
        let svc = axum::Router::new().route(
            "/probe",
            axum::routing::post(|_payload: ApiJson<T>| async move {
                axum::Json(serde_json::json!({ "ok": true }))
            }),
        );
        let req = Request::post("/probe")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = svc.oneshot(req).await.unwrap();
        let status = res.status().as_u16();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, value)
    }

    #[tokio::test]
    async fn malformed_json_returns_400() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Probe {
            #[allow(dead_code)]
            name: String,
        }
        let (status, body) = run::<Probe>("{ not valid json").await;
        assert_eq!(
            status, 400,
            "malformed JSON must be 400, got {status}: {body}"
        );
        assert_eq!(body["error"]["code"], "validation_failed");
    }

    #[tokio::test]
    async fn unknown_field_returns_422_with_field_named() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct Probe {
            #[allow(dead_code)]
            name: String,
        }
        let (status, body) = run::<Probe>(r#"{"name": "x", "extraneous": "y"}"#).await;
        assert_eq!(
            status, 422,
            "unknown field must be 422, got {status}: {body}"
        );
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "extraneous"),
            "expected `extraneous` in details, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn wrong_type_returns_422() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Probe {
            #[allow(dead_code)]
            name: String,
        }
        let (status, body) = run::<Probe>(r#"{"name": 5}"#).await;
        assert_eq!(
            status, 422,
            "wrong-typed field must be 422, got {status}: {body}"
        );
        let details = body["error"]["details"].as_array().expect("details array");
        assert_eq!(details.len(), 1);
    }

    #[tokio::test]
    async fn missing_field_returns_422_with_field_named() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Probe {
            #[allow(dead_code)]
            name: String,
            #[allow(dead_code)]
            slug: String,
        }
        let (status, body) = run::<Probe>(r#"{"name": "x"}"#).await;
        assert_eq!(
            status, 422,
            "missing required field must be 422, got {status}: {body}"
        );
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected `slug` in details, got: {details:?}"
        );
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct PageParams {
    pub limit: u32,
    pub cursor: Option<String>,
}

impl Default for PageParams {
    fn default() -> Self {
        Self {
            limit: 25,
            cursor: None,
        }
    }
}

impl PageParams {
    pub fn normalized(mut self) -> Self {
        self.limit = self.limit.clamp(1, 100);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_failed_has_400_and_correct_code() {
        let err = ApiError::validation_failed("bad input");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.envelope.error.code, "validation_failed");
    }

    #[test]
    fn unauthenticated_has_401_and_correct_code() {
        let err = ApiError::unauthenticated("login required");
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.envelope.error.code, "unauthenticated");
    }

    #[test]
    fn unauthorized_has_403_and_correct_code() {
        let err = ApiError::unauthorized("not allowed");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.envelope.error.code, "unauthorized");
    }

    #[test]
    fn not_found_has_404_and_correct_code() {
        let err = ApiError::not_found("missing");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.envelope.error.code, "not_found");
    }

    #[test]
    fn conflict_has_409_and_correct_code() {
        let err = ApiError::conflict("duplicate");
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(err.envelope.error.code, "conflict");
    }

    #[test]
    fn unprocessable_has_422_and_correct_code() {
        let err = ApiError::unprocessable("invalid data");
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.envelope.error.code, "unprocessable");
    }

    #[test]
    fn unprocessable_entity_has_422_and_validation_failed_code() {
        let err = ApiError::unprocessable_entity("validation failed");
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.envelope.error.code, "validation_failed");
        assert_eq!(err.envelope.error.message, "validation failed");
    }

    #[test]
    fn rate_limited_has_429_and_correct_code() {
        let err = ApiError::rate_limited("too fast");
        assert_eq!(err.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.envelope.error.code, "rate_limited");
    }

    #[test]
    fn internal_error_has_500_and_correct_code() {
        let err = ApiError::internal_error("server error");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.envelope.error.code, "internal_error");
    }

    #[test]
    fn details_serialize_in_envelope() {
        let details = vec![ErrorDetail {
            field: "email".into(),
            code: "invalid_format".into(),
            message: "Invalid email format".into(),
        }];
        let err = ApiError::validation_failed("bad request").with_details(details);
        assert_eq!(err.envelope.error.details.len(), 1);
        assert_eq!(
            err.envelope.error.details[0]["field"].as_str().unwrap(),
            "email"
        );
    }

    #[test]
    fn envelope_json_shape_matches_contract() {
        let err = ApiError::not_found("Route not found").with_request_id("req_abc123");
        let json = serde_json::to_value(&err.envelope).unwrap();
        assert!(json.get("error").is_some());
        assert_eq!(json["error"]["code"], "not_found");
        assert_eq!(json["error"]["message"], "Route not found");
        assert_eq!(json["error"]["request_id"], "req_abc123");
    }

    #[test]
    fn pagination_defaults_and_clamps() {
        assert_eq!(PageParams::default().limit, 25);
        assert_eq!(
            PageParams {
                limit: 1000,
                cursor: None
            }
            .normalized()
            .limit,
            100
        );
    }
}
