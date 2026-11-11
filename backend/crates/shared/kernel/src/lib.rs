//! Kernel — shared domain primitives
//!
//! # Purpose
//! Types that cross every module boundary: error envelope with Retry-After
//! support, pagination, JSON extraction, idempotency, and the async
//! `RateLimiter` trait with its `InMemoryRateLimitStore` implementation.
//! No business logic.
//!
//! # Public Interfaces
//! - `ApiError` constructors (rate_limited accepts a Duration for Retry-After)
//! - `ApiJson<T>` extractor
//! - `PageParams`, `Page<T>`
//! - Idempotency middleware
//! - `RateLimiter` trait, `Scope`, `Budget`, `Decision`, `InMemoryRateLimitStore`
//!
//! # Dependencies
//! - `axum`, `serde`, `uuid`, `strum`, `async-trait`
//!
//! # Extension Points
//! - Add new `ApiError` constructors as the status map grows.
//! - Add new `Scope` variants for new rate-limit domains.

mod idempotency;
pub mod rate_limit;
pub use rate_limit::{
    Budget, Decision, InMemoryRateLimitStore, RateLimiter, Scope, TenantRateLimit,
};

use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use utoipa::{IntoParams, ToSchema};

pub use idempotency::{
    idempotency_middleware, CachedResponse, IdempotencyKey, IdempotencyStore,
    InMemoryIdempotencyStore,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub details: Vec<serde_json::Value>,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// The text every 5xx built by [`ApiError::internal_error`] shows to the client.
pub const INTERNAL_ERROR_MESSAGE: &str = "Internal server error";

/// What went wrong behind a 5xx response. It never reaches the client: `ApiError::into_response`
/// leaves it in the response extensions, and the server's `internal_error_middleware` logs it
/// with the request ID and the tenant ID and then drops it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalErrorDetail {
    /// Fixed description of the failed step, for example `"Transaction commit failed"`.
    pub context: &'static str,
    /// Text of the underlying error, when there is one.
    pub source: Option<String>,
    /// Tenant the failed request was working for, when the handler knows it but the tenant
    /// middleware didn't run (public routes that find the tenant from a token).
    pub tenant_id: Option<uuid::Uuid>,
}

/// Set by the tenant middleware on a response so the error log can name the tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseTenant(pub uuid::Uuid);

#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    envelope: ErrorEnvelope,
    // Whole seconds for the `Retry-After` header. A `u32` (not `Duration`) keeps `ApiError` under
    // the size clippy::result_large_err allows.
    retry_after: Option<u32>,
    // Boxed so `Result<_, ApiError>` stays small (clippy::result_large_err).
    internal: Option<Box<InternalErrorDetail>>,
}

impl ApiError {
    pub fn new_with_code(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
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
            retry_after: None,
            internal: None,
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

    pub fn rate_limited(message: impl Into<String>, retry_after: Duration) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            envelope: ErrorEnvelope {
                error: ErrorBody {
                    code: "rate_limited".to_owned(),
                    message: message.into(),
                    details: Vec::new(),
                    request_id: String::new(),
                },
            },
            retry_after: Some(u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX)),
            internal: None,
        }
    }

    /// HTTP 500 with code `internal_error`. The client always gets
    /// [`INTERNAL_ERROR_MESSAGE`]; `context` goes to the server log only.
    ///
    /// The parameter is `&'static str` on purpose: a `format!` that interpolates an error does
    /// not compile, so error text can't be put into a response by accident. Pass the underlying
    /// error to [`ApiError::internal_error_from`] instead.
    pub fn internal_error(context: &'static str) -> Self {
        Self::internal(context, None)
    }

    /// Like [`ApiError::internal_error`], and keeps the text of `source` for the server log.
    pub fn internal_error_from(context: &'static str, source: impl std::fmt::Display) -> Self {
        Self::internal(context, Some(source.to_string()))
    }

    /// Name the tenant in the server log for this error. Only affects 5xx built by
    /// `internal_error*`; use it on public routes that learn the tenant from a token, where no
    /// tenant middleware tags the response.
    pub fn for_tenant(mut self, tenant_id: uuid::Uuid) -> Self {
        if let Some(detail) = self.internal.as_mut() {
            detail.tenant_id = Some(tenant_id);
        }
        self
    }

    fn internal(context: &'static str, source: Option<String>) -> Self {
        let mut error = Self::new_with_code(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            INTERNAL_ERROR_MESSAGE,
        );
        error.internal = Some(Box::new(InternalErrorDetail {
            context,
            source,
            tenant_id: None,
        }));
        error
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
        let mut res = (self.status, Json(self.envelope)).into_response();
        if let Some(detail) = self.internal {
            res.extensions_mut().insert(*detail);
        }
        if let Some(retry_after) = self.retry_after {
            res.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from(retry_after),
            );
        }
        res
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, IntoParams)]
#[into_params(parameter_in = Query)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
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
        let err = ApiError::rate_limited("too fast", Duration::from_secs(30));
        assert_eq!(err.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.envelope.error.code, "rate_limited");
        assert_eq!(err.retry_after, Some(30));
    }

    #[test]
    fn internal_error_has_500_and_correct_code() {
        let err = ApiError::internal_error("server error");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.envelope.error.code, "internal_error");
    }

    #[test]
    fn internal_error_shows_a_generic_message_not_the_context() {
        let err = ApiError::internal_error("Failed to update integration");
        assert_eq!(err.envelope.error.message, INTERNAL_ERROR_MESSAGE);
    }

    #[tokio::test]
    async fn internal_error_from_keeps_source_out_of_the_body_and_in_the_extensions() {
        let err = ApiError::internal_error_from(
            "Database query failed",
            "relation \"customers\" does not exist",
        );
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let detail = response
            .extensions()
            .get::<InternalErrorDetail>()
            .expect("detail is attached for the server log")
            .clone();
        assert_eq!(detail.context, "Database query failed");
        assert_eq!(
            detail.source.as_deref(),
            Some("relation \"customers\" does not exist")
        );
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            !text.contains("customers"),
            "body leaked the source: {text}"
        );
        assert!(
            !text.contains("Database query failed"),
            "body leaked the context: {text}"
        );
        assert!(text.contains(INTERNAL_ERROR_MESSAGE));
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
