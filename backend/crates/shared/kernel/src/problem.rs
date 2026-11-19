//! RFC 9457 Problem Details, next to the legacy error envelope (ADR-012 gap A2, MOH-197).
//!
//! During the deprecation window the API answers an error in one of two formats, chosen by the
//! request's `Accept` header: `application/problem+json` for a client that asks for it, the legacy
//! `{ "error": { code, message, details, request_id } }` envelope for everyone else. Both come
//! from the one [`ApiError`](crate::ApiError): it renders the legacy body and leaves its parts on
//! the response ([`ErrorResponseParts`]), and the server's `error_format_middleware`, which can see
//! the request, rewrites the body when problem+json was asked for and marks the legacy answer as
//! deprecated otherwise.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{ErrorEnvelope, INTERNAL_ERROR_MESSAGE};

/// The media type of an RFC 9457 error body.
pub const PROBLEM_JSON: &str = "application/problem+json";

/// The `Deprecation` header of a legacy-format error response (RFC 9745: the date the format was
/// deprecated, as a Unix timestamp in a structured-field date: 2026-11-19T00:00:00Z).
pub const LEGACY_DEPRECATION: &str = "@1795046400";

/// The `Link` header of a legacy-format error response: where the migration is described.
pub const LEGACY_DEPRECATION_LINK: &str = "<https://github.com/mohamed155/customer-service-chat-system/blob/master/docs/adr/0012-api-conventions.md>; rel=\"deprecation\"";

/// A field-level error: which field, and the machine code a client translates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProblemFieldError {
    pub field: String,
    pub code: String,
}

/// An error as RFC 9457 Problem Details, with the API's `code`, `request_id` and `errors`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ProblemDetails {
    /// A stable URI per `code`: `urn:problem:<code>`.
    #[serde(rename = "type")]
    pub type_uri: String,
    /// A short, per-code title.
    pub title: String,
    pub status: u16,
    /// The specific message for this occurrence. A 5xx never carries internal detail.
    pub detail: String,
    /// The request path (no query string).
    pub instance: String,
    /// The stable machine code clients branch on.
    pub code: String,
    /// The request ID to quote to support.
    pub request_id: String,
    /// Field errors, one per invalid field.
    pub errors: Vec<ProblemFieldError>,
    /// Other structured context the error carries (an ID of the thing in conflict, for example).
    /// Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<serde_json::Value>,
}

/// What an [`ApiError`](crate::ApiError) was, left in the response extensions so a layer that sees
/// the request can render it as problem+json.
#[derive(Debug, Clone)]
pub struct ErrorResponseParts {
    pub status: StatusCode,
    pub envelope: ErrorEnvelope,
}

/// The stable type URI of an error code.
pub fn problem_type(code: &str) -> String {
    format!("urn:problem:{code}")
}

/// A short title for an error code: a few that read badly when derived are spelled out, the rest
/// are the code with underscores turned into spaces (`validation_failed` is "Validation failed").
pub fn problem_title(code: &str) -> String {
    match code {
        "unauthenticated" => "Authentication required".to_owned(),
        "unauthorized" => "Permission denied".to_owned(),
        "internal_error" => INTERNAL_ERROR_MESSAGE.to_owned(),
        other => {
            let spaced = other.replace('_', " ");
            let mut chars = spaced.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

impl ErrorResponseParts {
    /// The problem document for this error. `instance` is the request path; `request_id` is used
    /// when the error does not carry one yet. A 5xx keeps only its public message and no details.
    pub fn to_problem(&self, instance: &str, request_id: &str) -> ProblemDetails {
        let error = &self.envelope.error;
        let server_error = self.status.is_server_error();
        let (errors, details) = if server_error {
            (Vec::new(), Vec::new())
        } else {
            split_details(&error.details)
        };
        ProblemDetails {
            type_uri: problem_type(&error.code),
            title: problem_title(&error.code),
            status: self.status.as_u16(),
            detail: error.message.clone(),
            instance: instance.to_owned(),
            code: error.code.clone(),
            request_id: if error.request_id.is_empty() {
                request_id.to_owned()
            } else {
                error.request_id.clone()
            },
            errors,
            details,
        }
    }
}

/// Field errors (an object with a string `field` and a string `code`) become `errors[]`; any other
/// detail is kept whole under `details`, so nothing a client could read is dropped.
fn split_details(
    details: &[serde_json::Value],
) -> (Vec<ProblemFieldError>, Vec<serde_json::Value>) {
    let mut errors = Vec::new();
    let mut other = Vec::new();
    for detail in details {
        let field = detail.get("field").and_then(|v| v.as_str());
        let code = detail.get("code").and_then(|v| v.as_str());
        match (field, code) {
            (Some(field), Some(code)) => errors.push(ProblemFieldError {
                field: field.to_owned(),
                code: code.to_owned(),
            }),
            _ => other.push(detail.clone()),
        }
    }
    (errors, other)
}

/// A `qvalue` as RFC 9110 writes it: `0` or `1`, optionally followed by a dot and up to three
/// digits, and nothing above 1. Anything else (`1e0`, `.5`, `0.8000`, `1.5`, `-1`) is not a quality,
/// even where Rust's float parser would take it.
fn parse_qvalue(raw: &str) -> Option<f32> {
    let raw = raw.trim();
    let (whole, fraction) = match raw.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (raw, None),
    };
    let digits = fraction.unwrap_or("");
    if digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let thousandths: u32 = if digits.is_empty() {
        0
    } else {
        digits.parse::<u32>().ok()? * 10u32.pow(3 - digits.len() as u32)
    };
    match whole {
        "0" => Some(thousandths as f32 / 1000.0),
        "1" if thousandths == 0 => Some(1.0),
        _ => None,
    }
}

/// Whether the `Accept` header asks for problem+json: it lists `application/problem+json` with a
/// quality above zero and at least that of `application/json`. Only the exact media type opts in:
/// `*/*`, `application/*` and no header at all keep the legacy envelope, so no existing client
/// changes by accident.
pub fn prefers_problem_json(accept: &str) -> bool {
    let mut problem: Option<f32> = None;
    let mut json: Option<f32> = None;
    for range in accept.split(',') {
        let mut parts = range.split(';');
        let media = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let mut quality = Some(1.0_f32);
        for parameter in parts {
            let parameter = parameter.trim();
            if let Some(value) = parameter
                .strip_prefix("q=")
                .or_else(|| parameter.strip_prefix("Q="))
            {
                quality = parse_qvalue(value);
            }
        }
        // An unusable quality value makes the range unusable, not "1".
        let Some(quality) = quality else { continue };
        let slot = match media.as_str() {
            PROBLEM_JSON => &mut problem,
            "application/json" => &mut json,
            _ => continue,
        };
        *slot = Some(slot.map_or(quality, |q| q.max(quality)));
    }
    match problem {
        Some(p) if p > 0.0 => p >= json.unwrap_or(0.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApiError, ErrorBody};

    fn parts(
        status: StatusCode,
        code: &str,
        message: &str,
        details: Vec<serde_json::Value>,
    ) -> ErrorResponseParts {
        ErrorResponseParts {
            status,
            envelope: ErrorEnvelope {
                error: ErrorBody {
                    code: code.to_owned(),
                    message: message.to_owned(),
                    details,
                    request_id: String::new(),
                },
            },
        }
    }

    #[test]
    fn content_negotiation_follows_the_accept_header() {
        for (accept, wants_problem) in [
            ("", false),
            ("application/json", false),
            ("*/*", false),
            ("application/*", false),
            ("text/html", false),
            ("application/problem+json", true),
            ("APPLICATION/PROBLEM+JSON", true),
            ("application/problem+json, application/json;q=0.5", true),
            ("application/json, application/problem+json;q=0.5", false),
            (
                "application/json;q=0.8, application/problem+json;q=0.8",
                true,
            ),
            ("application/problem+json;q=0", false),
            ("application/problem+json;q=abc, application/json", false),
            ("application/problem+json;q=1.5", false),
            ("application/problem+json;q=1e0", false),
            ("application/problem+json;q=.5", false),
            ("application/problem+json;q=0.8000", false),
            ("application/problem+json;q=-1", false),
            ("application/problem+json;q=1.0001", false),
            ("application/problem+json;q=1.000", true),
            ("application/problem+json;q=1", true),
            ("application/problem+json;q=0.001", true),
            (
                "application/problem+json;q=0.800, application/json;q=0.5",
                true,
            ),
            ("application/problem+json;q=0., application/json;q=1", false),
            ("text/html, application/problem+json;q=0.9, */*;q=0.1", true),
        ] {
            assert_eq!(
                prefers_problem_json(accept),
                wants_problem,
                "Accept: {accept:?}"
            );
        }
    }

    #[test]
    fn a_validation_error_becomes_field_errors() {
        let error = parts(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Validation failed",
            vec![
                serde_json::json!({"field": "email", "code": "invalid_format", "message": "bad"}),
                serde_json::json!({"field": "name", "code": "too_long", "message": "long"}),
            ],
        );
        let problem = error.to_problem("/api/v1/tenant/members", "req_1");
        assert_eq!(problem.type_uri, "urn:problem:validation_failed");
        assert_eq!(problem.title, "Validation failed");
        assert_eq!(problem.status, 422);
        assert_eq!(problem.detail, "Validation failed");
        assert_eq!(problem.instance, "/api/v1/tenant/members");
        assert_eq!(problem.code, "validation_failed");
        assert_eq!(problem.request_id, "req_1");
        assert_eq!(
            problem.errors,
            vec![
                ProblemFieldError {
                    field: "email".into(),
                    code: "invalid_format".into()
                },
                ProblemFieldError {
                    field: "name".into(),
                    code: "too_long".into()
                },
            ]
        );
        assert!(problem.details.is_empty());
        let json = serde_json::to_value(&problem).unwrap();
        assert!(
            json.get("details").is_none(),
            "empty context is omitted: {json}"
        );
        assert_eq!(json["type"], "urn:problem:validation_failed");
    }

    #[test]
    fn context_details_are_kept_not_dropped() {
        let error = parts(
            StatusCode::CONFLICT,
            "conflict",
            "Conversation already has an active escalation",
            vec![serde_json::json!({"escalationId": "e1"})],
        );
        let problem = error.to_problem("/p", "req_1");
        assert!(problem.errors.is_empty());
        assert_eq!(
            problem.details,
            vec![serde_json::json!({"escalationId": "e1"})]
        );
    }

    #[test]
    fn the_errors_own_request_id_wins_over_the_fallback() {
        let mut error = parts(StatusCode::NOT_FOUND, "not_found", "Gone", vec![]);
        error.envelope.error.request_id = "req_own".into();
        assert_eq!(error.to_problem("/p", "req_header").request_id, "req_own");
    }

    #[test]
    fn a_server_error_carries_no_internal_detail_or_context() {
        let error = parts(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            INTERNAL_ERROR_MESSAGE,
            vec![
                serde_json::json!({"field": "x", "code": "y"}),
                serde_json::json!({"row": "secret"}),
            ],
        );
        let problem = error.to_problem("/p", "req_1");
        assert_eq!(problem.detail, INTERNAL_ERROR_MESSAGE);
        assert_eq!(problem.title, INTERNAL_ERROR_MESSAGE);
        assert!(problem.errors.is_empty() && problem.details.is_empty());
    }

    #[test]
    fn titles_read_well() {
        assert_eq!(problem_title("not_found"), "Not found");
        assert_eq!(problem_title("rate_limited"), "Rate limited");
        assert_eq!(problem_title("service_unavailable"), "Service unavailable");
        assert_eq!(problem_title("unauthenticated"), "Authentication required");
        assert_eq!(problem_title("unauthorized"), "Permission denied");
        assert_eq!(problem_title(""), "");
        assert_eq!(
            problem_type("membership_disabled"),
            "urn:problem:membership_disabled"
        );
    }

    #[tokio::test]
    async fn an_api_error_response_still_carries_the_legacy_body_and_its_parts() {
        use axum::response::IntoResponse;
        let response = ApiError::conflict("Taken")
            .with_details(vec![
                serde_json::json!({"field": "slug", "code": "taken", "message": "m"}),
            ])
            .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        let parts = response
            .extensions()
            .get::<ErrorResponseParts>()
            .expect("the parts are left for the negotiating layer");
        assert_eq!(parts.status, StatusCode::CONFLICT);
        assert_eq!(parts.envelope.error.code, "conflict");
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "conflict");
        assert_eq!(json["error"]["details"][0]["message"], "m");
    }
}
