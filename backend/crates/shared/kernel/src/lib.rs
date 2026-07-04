mod idempotency;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
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
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            envelope: ErrorEnvelope {
                error: ErrorBody {
                    code: code.into(),
                    message: message.into(),
                    details: Vec::new(),
                    request_id: format!("req_{}", uuid::Uuid::now_v7()),
                },
            },
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
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
