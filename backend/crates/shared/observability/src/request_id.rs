use axum::{extract::Request, http::HeaderName, middleware::Next, response::Response};
use uuid::Uuid;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub fn generate() -> String {
    format!("req_{}", Uuid::now_v7())
}

pub fn validate(id: &str) -> bool {
    if id.len() != 40 {
        return false;
    }
    if !id.starts_with("req_") {
        return false;
    }
    let uuid_part = &id[4..];
    if uuid_part != uuid_part.to_ascii_lowercase() {
        return false;
    }
    Uuid::parse_str(uuid_part).is_ok()
}

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| validate(v))
        .map(str::to_owned)
        .unwrap_or_else(generate);

    if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
        request
            .headers_mut()
            .insert(REQUEST_ID_HEADER.clone(), value);
    }

    let mut response = next.run(request).await;

    if !response.headers().contains_key(&REQUEST_ID_HEADER) {
        if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
            response
                .headers_mut()
                .insert(REQUEST_ID_HEADER.clone(), value);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_output_matches_format() {
        let id = generate();
        assert_eq!(id.len(), 40);
        assert!(id.starts_with("req_"));
        assert!(validate(&id), "generated ID should validate: {id}");
    }

    #[test]
    fn generate_produces_monotonically_sortable_ids() {
        let a = generate();
        let b = generate();
        assert!(a < b, "UUIDv7 should be time-sortable: {a} >= {b}");
    }

    #[test]
    fn validate_accepts_canonical_form() {
        let id = generate();
        assert!(validate(&id), "canonical ID should be valid: {id}");
    }

    #[test]
    fn validate_rejects_missing_prefix() {
        let id = "123e4567-e89b-12d3-a456-426614174000";
        assert!(!validate(id));
    }

    #[test]
    fn validate_rejects_uppercase_uuid() {
        let id = "req_123E4567-E89B-12D3-A456-426614174000";
        assert!(!validate(id), "uppercase UUID should be rejected");
    }

    #[test]
    fn validate_rejects_overlong() {
        let id = &(generate() + "extra");
        assert!(!validate(id), "overlong ID should be rejected");
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(!validate(""));
    }

    #[test]
    fn validate_rejects_script_tag() {
        assert!(!validate("<script>"));
    }
}
