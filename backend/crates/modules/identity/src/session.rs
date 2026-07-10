use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, jsonwebtoken::errors::Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue_token(secret: &str, ttl: u64, user_id: Uuid) -> Result<(String, Uuid, DateTime<Utc>)> {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(ttl as i64);
    let jti = Uuid::new_v4();
    let claims = SessionClaims {
        sub: user_id,
        jti,
        iat: issued_at.timestamp(),
        exp: expires_at.timestamp(),
    };
    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok((jwt, jti, expires_at))
}

pub fn validate_token(secret: &str, jwt: &str) -> Result<SessionClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.reject_tokens_expiring_in_less_than = 1;
    validation.validate_exp = true;

    decode::<SessionClaims>(
        jwt,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|token| token.claims)
}

pub fn build_session_cookie(jwt: &str, ttl: u64) -> String {
    session_cookie_string(jwt, ttl)
}

pub fn clear_session_cookie() -> String {
    session_cookie_string("", 0)
}

fn session_cookie_string(value: &str, max_age: u64) -> String {
    format!("app_session={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-with-at-least-thirty-two-bytes";
    const OTHER_SECRET: &str = "other-secret-with-at-least-thirty-two-bytes";

    #[test]
    fn issue_and_validate_round_trip() {
        let user_id = Uuid::new_v4();

        let (jwt, jti, expires_at) = issue_token(SECRET, 60, user_id).unwrap();
        let claims = validate_token(SECRET, &jwt).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.jti, jti);
        assert_eq!(claims.exp, expires_at.timestamp());
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn expired_token_is_rejected() {
        let (jwt, _, _) = issue_token(SECRET, 0, Uuid::new_v4()).unwrap();

        assert!(validate_token(SECRET, &jwt).is_err());
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let (jwt, _, _) = issue_token(SECRET, 60, Uuid::new_v4()).unwrap();
        let mut parts: Vec<&str> = jwt.split('.').collect();
        parts[1] = if parts[1] == "e30" { "e31" } else { "e30" };
        let tampered = parts.join(".");

        assert!(validate_token(SECRET, &tampered).is_err());
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let (jwt, _, _) = issue_token(SECRET, 60, Uuid::new_v4()).unwrap();

        assert!(validate_token(OTHER_SECRET, &jwt).is_err());
    }

    #[test]
    fn session_cookie_attributes_are_exact() {
        assert_eq!(
            build_session_cookie("jwt-value", 28800),
            "app_session=jwt-value; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800"
        );
    }

    #[test]
    fn clear_session_cookie_attributes_are_exact() {
        assert_eq!(
            clear_session_cookie(),
            "app_session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
        );
    }
}
