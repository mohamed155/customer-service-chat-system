use axum::http::StatusCode;
use kernel::ApiError;
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub const SESSION_TTL_HOURS: i64 = 24;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_vec()
}

pub async fn authenticate_session(
    pool: &PgPool,
    auth_header: Option<&str>,
) -> Result<super::model::WidgetSessionRow, ApiError> {
    let token = auth_header
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            ApiError::new_with_code(
                StatusCode::UNAUTHORIZED,
                "session_invalid",
                "Missing or invalid authorization header",
            )
        })?;

    let token_hash = hash_token(token);
    let session = super::queries::find_session_by_token_hash(pool, &token_hash)
        .await
        .map_err(|_| ApiError::internal_error("Failed to look up session"))?
        .ok_or_else(|| {
            ApiError::new_with_code(
                StatusCode::UNAUTHORIZED,
                "session_invalid",
                "Session not found or expired",
            )
        })?;

    if session.expires_at < chrono::Utc::now() {
        return Err(ApiError::new_with_code(
            StatusCode::UNAUTHORIZED,
            "session_invalid",
            "Session has expired",
        ));
    }

    let new_expires_at = chrono::Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);
    super::queries::touch_session(pool, session.id, new_expires_at)
        .await
        .map_err(|_| ApiError::internal_error("Failed to refresh session"))?;

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_is_64_hex_chars() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn two_generated_tokens_differ() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "test-token-123";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_token_never_returns_the_raw_token() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_ne!(hash, token.as_bytes());
    }
}
