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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub details: Vec<ErrorDetail>,
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

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::UNPROCESSABLE_ENTITY, "unprocessable", message)
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::TOO_MANY_REQUESTS, "rate_limited", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new_with_code(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    pub fn with_details(mut self, details: Vec<ErrorDetail>) -> Self {
        self.envelope.error.details = details;
        self
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.envelope.error.request_id = request_id.into();
        self
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
        match Json::<T>::from_request(req, state).await {
            Ok(json) => Ok(Self(json.0)),
            Err(_) => Err(ApiError::validation_failed("Invalid request body")),
        }
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
        assert_eq!(err.envelope.error.details[0].field, "email");
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
