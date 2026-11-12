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
    /// How the user authenticated (RFC 8176 values): `pwd` for the password, `otp` for an
    /// authenticator code, `rcv` for a recovery code. Sessions issued before MFA have no `amr`;
    /// they read as empty, which counts as "no second factor".
    #[serde(default)]
    pub amr: Vec<String>,
    /// When the second factor was last proven (Unix seconds), for step-up checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa_at: Option<i64>,
}

impl SessionClaims {
    /// Whether a second factor was used for this session.
    pub fn has_second_factor(&self) -> bool {
        self.amr.iter().any(|m| m == AMR_OTP || m == AMR_RECOVERY)
    }

    /// Whether a second factor was proven within the last `window_seconds`, at `now`.
    pub fn second_factor_fresh(&self, now: i64, window_seconds: i64) -> bool {
        self.has_second_factor()
            && self
                .mfa_at
                .is_some_and(|at| at <= now && now - at <= window_seconds)
    }
}

pub const AMR_PASSWORD: &str = "pwd";
pub const AMR_OTP: &str = "otp";
pub const AMR_RECOVERY: &str = "rcv";

/// How a session was earned, recorded in its claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAuth {
    pub amr: Vec<String>,
    pub mfa_at: Option<i64>,
}

impl SessionAuth {
    /// A password alone.
    pub fn password() -> Self {
        Self {
            amr: vec![AMR_PASSWORD.to_owned()],
            mfa_at: None,
        }
    }

    /// A password and a second factor proven now (`method` is [`AMR_OTP`] or [`AMR_RECOVERY`]).
    pub fn second_factor(method: &str) -> Self {
        Self {
            amr: vec![AMR_PASSWORD.to_owned(), method.to_owned()],
            mfa_at: Some(Utc::now().timestamp()),
        }
    }
}

/// A session with a password alone (sign-up, invitation accept). Sign-in with a second factor uses
/// [`issue_session`].
pub fn issue_token(secret: &str, ttl: u64, user_id: Uuid) -> Result<(String, Uuid, DateTime<Utc>)> {
    issue_session(secret, ttl, user_id, SessionAuth::password())
}

pub fn issue_session(
    secret: &str,
    ttl: u64,
    user_id: Uuid,
    auth: SessionAuth,
) -> Result<(String, Uuid, DateTime<Utc>)> {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(ttl as i64);
    // uuid-v4-ok: session token ID: random so it reveals no time and can't be guessed from the clock
    let jti = Uuid::new_v4();
    let claims = SessionClaims {
        sub: user_id,
        jti,
        iat: issued_at.timestamp(),
        exp: expires_at.timestamp(),
        amr: auth.amr,
        mfa_at: auth.mfa_at,
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

// ---------------------------------------------------------------------------
// The sign-in challenge: proof of the password, waiting for the second factor.
// ---------------------------------------------------------------------------

/// How long the second step may take after the password step.
pub const PENDING_TTL_SECONDS: u64 = 300;
/// The `aud` of a challenge token. A session token has no audience and the session check accepts
/// none, so a challenge can never be used as a session; the challenge check requires this one, so a
/// session can never be used as a challenge.
const PENDING_AUDIENCE: &str = "mfa_pending";
pub const PENDING_COOKIE_NAME: &str = "app_mfa_pending";
/// The cookie is only sent to the second-step routes.
pub const PENDING_COOKIE_PATH: &str = "/api/v1/auth/mfa";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingClaims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub iat: i64,
    pub exp: i64,
    pub aud: String,
}

pub fn issue_pending(secret: &str, user_id: Uuid) -> Result<(String, PendingClaims)> {
    let issued_at = Utc::now();
    let claims = PendingClaims {
        sub: user_id,
        // uuid-v4-ok: challenge token ID: random so it reveals no time and can't be guessed
        jti: Uuid::new_v4(),
        iat: issued_at.timestamp(),
        exp: (issued_at + Duration::seconds(PENDING_TTL_SECONDS as i64)).timestamp(),
        aud: PENDING_AUDIENCE.to_owned(),
    };
    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok((jwt, claims))
}

pub fn validate_pending(secret: &str, jwt: &str) -> Result<PendingClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.reject_tokens_expiring_in_less_than = 1;
    validation.validate_exp = true;
    validation.set_audience(&[PENDING_AUDIENCE]);
    decode::<PendingClaims>(
        jwt,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|token| token.claims)
}

pub fn build_pending_cookie(jwt: &str) -> String {
    pending_cookie_string(jwt, PENDING_TTL_SECONDS)
}

pub fn clear_pending_cookie() -> String {
    pending_cookie_string("", 0)
}

fn pending_cookie_string(value: &str, max_age: u64) -> String {
    format!(
        "{PENDING_COOKIE_NAME}={value}; HttpOnly; Secure; SameSite=Lax; Path={PENDING_COOKIE_PATH}; Max-Age={max_age}"
    )
}

/// The challenge token in a `Cookie` header.
pub fn pending_token_from_cookie_header(cookie_header: &str) -> Option<&str> {
    cookie_header.split(';').find_map(|part| {
        let value = part.trim().strip_prefix("app_mfa_pending=")?;
        (!value.is_empty()).then_some(value)
    })
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
    fn password_sessions_have_no_second_factor_and_second_factor_sessions_do() {
        let (jwt, _, _) = issue_token(SECRET, 60, Uuid::new_v4()).unwrap();
        let claims = validate_token(SECRET, &jwt).unwrap();
        assert_eq!(claims.amr, vec!["pwd"]);
        assert!(!claims.has_second_factor());
        assert!(!claims.second_factor_fresh(Utc::now().timestamp(), 300));

        let (jwt, _, _) = issue_session(
            SECRET,
            60,
            Uuid::new_v4(),
            SessionAuth::second_factor(AMR_OTP),
        )
        .unwrap();
        let claims = validate_token(SECRET, &jwt).unwrap();
        assert_eq!(claims.amr, vec!["pwd", "otp"]);
        assert!(claims.has_second_factor());
        let now = Utc::now().timestamp();
        assert!(claims.second_factor_fresh(now, 300));
        assert!(
            !claims.second_factor_fresh(now + 301, 300),
            "stale after the window"
        );
        assert!(
            !claims.second_factor_fresh(now - 10, 300),
            "not from the future"
        );
    }

    #[test]
    fn a_token_from_before_mfa_still_validates_and_counts_as_no_second_factor() {
        // The claims a pre-MFA build wrote: no amr, no mfa_at.
        #[derive(Serialize)]
        struct Old {
            sub: Uuid,
            jti: Uuid,
            iat: i64,
            exp: i64,
        }
        let now = Utc::now().timestamp();
        let jwt = encode(
            &Header::new(Algorithm::HS256),
            &Old {
                sub: Uuid::new_v4(),
                jti: Uuid::new_v4(),
                iat: now,
                exp: now + 60,
            },
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        let claims = validate_token(SECRET, &jwt).unwrap();
        assert!(claims.amr.is_empty() && claims.mfa_at.is_none());
        assert!(!claims.has_second_factor());
    }

    #[test]
    fn a_challenge_token_is_never_a_session_and_a_session_never_a_challenge() {
        let user = Uuid::new_v4();
        let (challenge, claims) = issue_pending(SECRET, user).unwrap();
        assert_eq!(claims.exp - claims.iat, PENDING_TTL_SECONDS as i64);
        assert_eq!(validate_pending(SECRET, &challenge).unwrap().sub, user);
        // Put in the session cookie, it is refused: it carries an audience.
        assert!(validate_token(SECRET, &challenge).is_err());

        let (session, _, _) = issue_token(SECRET, 60, user).unwrap();
        assert!(validate_pending(SECRET, &session).is_err());
        // A challenge for another secret, or an expired one, is refused too.
        assert!(validate_pending(OTHER_SECRET, &challenge).is_err());
    }

    #[test]
    fn an_expired_challenge_is_refused() {
        let past = Utc::now().timestamp() - 400;
        let claims = PendingClaims {
            sub: Uuid::new_v4(),
            jti: Uuid::new_v4(),
            iat: past,
            exp: past + PENDING_TTL_SECONDS as i64,
            aud: PENDING_AUDIENCE.to_owned(),
        };
        let jwt = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert!(validate_pending(SECRET, &jwt).is_err());
    }

    #[test]
    fn pending_cookie_is_scoped_to_the_second_step_and_found_in_a_header() {
        assert_eq!(
            build_pending_cookie("jwt"),
            "app_mfa_pending=jwt; HttpOnly; Secure; SameSite=Lax; Path=/api/v1/auth/mfa; Max-Age=300"
        );
        assert_eq!(
            clear_pending_cookie(),
            "app_mfa_pending=; HttpOnly; Secure; SameSite=Lax; Path=/api/v1/auth/mfa; Max-Age=0"
        );
        assert_eq!(
            pending_token_from_cookie_header("a=b; app_mfa_pending=tok; app_session=s"),
            Some("tok")
        );
        assert_eq!(pending_token_from_cookie_header("app_session=s"), None);
        assert_eq!(pending_token_from_cookie_header("app_mfa_pending="), None);
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
