use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
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
    /// authenticator code, `rcv` for a recovery code.
    #[serde(default)]
    pub amr: Vec<String>,
    /// When the second factor was last proven (Unix seconds), for step-up checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa_at: Option<i64>,
    /// The id of the session's `auth_sessions` row.
    pub sid: Uuid,
    /// Issuer, audience and kind (`tenant_user` or `platform_user`) of an EdDSA access token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typ: Option<String>,
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

/// The `iss` and `aud` of every access token. Both are validated.
pub const ISSUER: &str = "helix";
pub const AUDIENCE: &str = "helix-dashboard";
/// The `typ` claim: who the token is for.
pub const TYP_TENANT_USER: &str = "tenant_user";
pub const TYP_PLATFORM_USER: &str = "platform_user";

/// Access token lifetime when `AUTH_ACCESS_TTL_SECONDS` is not set: ten minutes.
pub const DEFAULT_ACCESS_TTL_SECONDS: u64 = 600;
const MIN_ACCESS_TTL_SECONDS: u64 = 300;
const MAX_ACCESS_TTL_SECONDS: u64 = 900;

/// The access token lifetime: `AUTH_ACCESS_TTL_SECONDS`, from five to fifteen minutes (ADR-005 §2).
/// A value outside that range, or one that is not a number, is ignored with a warning.
pub fn access_ttl_seconds() -> u64 {
    static TTL: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *TTL.get_or_init(|| match std::env::var("AUTH_ACCESS_TTL_SECONDS") {
        Err(_) => DEFAULT_ACCESS_TTL_SECONDS,
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(v) if (MIN_ACCESS_TTL_SECONDS..=MAX_ACCESS_TTL_SECONDS).contains(&v) => v,
            _ => {
                tracing::warn!(
                    "AUTH_ACCESS_TTL_SECONDS must be between {MIN_ACCESS_TTL_SECONDS} and {MAX_ACCESS_TTL_SECONDS}; using {DEFAULT_ACCESS_TTL_SECONDS}"
                );
                DEFAULT_ACCESS_TTL_SECONDS
            }
        },
    })
}

/// Signs an access token for session `sid`, with the key that signs now. `platform` selects the
/// `typ` claim.
pub fn issue_access_token(
    keys: &crate::keys::KeySet,
    ttl: u64,
    user_id: Uuid,
    sid: Uuid,
    platform: bool,
    auth: SessionAuth,
) -> Result<(String, Uuid, DateTime<Utc>)> {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(ttl as i64);
    // uuid-v4-ok: access token ID: random so it reveals no time and can't be guessed from the clock
    let jti = Uuid::new_v4();
    let claims = SessionClaims {
        sub: user_id,
        jti,
        iat: issued_at.timestamp(),
        exp: expires_at.timestamp(),
        amr: auth.amr,
        mfa_at: auth.mfa_at,
        sid,
        iss: Some(ISSUER.to_owned()),
        aud: Some(AUDIENCE.to_owned()),
        typ: Some(
            if platform {
                TYP_PLATFORM_USER
            } else {
                TYP_TENANT_USER
            }
            .to_owned(),
        ),
    };
    Ok((sign(keys, &claims)?, jti, expires_at))
}

/// Verifies an EdDSA access token against `keys`. Only `alg = EdDSA` with a known `kid` is
/// accepted, so `none` and the HMAC algorithms can't be used to forge one (algorithm confusion).
/// `iss`, `aud`, `exp`, `sid` and `typ` are all required and checked, with no leeway.
pub fn validate_access_token(keys: &crate::keys::KeySet, jwt: &str) -> Result<SessionClaims> {
    use jsonwebtoken::errors::{Error, ErrorKind};

    let claims: SessionClaims = verify(keys, jwt, AUDIENCE)?;
    if !matches!(
        claims.typ.as_deref(),
        Some(TYP_TENANT_USER | TYP_PLATFORM_USER)
    ) {
        return Err(Error::from(ErrorKind::InvalidToken));
    }
    Ok(claims)
}

fn sign(keys: &crate::keys::KeySet, claims: &impl Serialize) -> Result<String> {
    let (kid, key) = keys.signing().ok_or_else(|| {
        jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidKeyFormat)
    })?;
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(kid.to_owned());
    encode(&header, claims, key)
}

fn verify<T: serde::de::DeserializeOwned>(
    keys: &crate::keys::KeySet,
    jwt: &str,
    audience: &str,
) -> Result<T> {
    use jsonwebtoken::errors::{Error, ErrorKind};

    let header = jsonwebtoken::decode_header(jwt)?;
    if header.alg != Algorithm::EdDSA {
        return Err(Error::from(ErrorKind::InvalidAlgorithm));
    }
    let key = header
        .kid
        .as_deref()
        .and_then(|kid| keys.verifying(kid))
        .ok_or_else(|| Error::from(ErrorKind::InvalidKeyFormat))?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.leeway = 0;
    validation.reject_tokens_expiring_in_less_than = 1;
    validation.set_audience(&[audience]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
    decode(jwt, key, &validation).map(|token| token.claims)
}

// The sign-in challenge proves the password while the second step is pending.

/// How long the second step may take after the password step.
pub const PENDING_TTL_SECONDS: u64 = 300;
/// Challenges carry a different audience and kind from access tokens.
const PENDING_AUDIENCE: &str = "mfa_pending";
const TYP_PENDING: &str = "mfa_pending";
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
    pub iss: String,
    pub typ: String,
}

pub fn issue_pending(keys: &crate::keys::KeySet, user_id: Uuid) -> Result<(String, PendingClaims)> {
    let issued_at = Utc::now();
    let claims = PendingClaims {
        sub: user_id,
        // uuid-v4-ok: challenge token ID: random so it reveals no time and can't be guessed
        jti: Uuid::new_v4(),
        iat: issued_at.timestamp(),
        exp: (issued_at + Duration::seconds(PENDING_TTL_SECONDS as i64)).timestamp(),
        aud: PENDING_AUDIENCE.to_owned(),
        iss: ISSUER.to_owned(),
        typ: TYP_PENDING.to_owned(),
    };
    Ok((sign(keys, &claims)?, claims))
}

pub fn validate_pending(keys: &crate::keys::KeySet, jwt: &str) -> Result<PendingClaims> {
    use jsonwebtoken::errors::{Error, ErrorKind};

    let claims: PendingClaims = verify(keys, jwt, PENDING_AUDIENCE)?;
    if claims.typ != TYP_PENDING {
        return Err(Error::from(ErrorKind::InvalidToken));
    }
    Ok(claims)
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

/// The access token cookie. The `__Host-` prefix pins it to the exact host: a sibling subdomain
/// can't set or overwrite it (ADR-005 §4).
pub const SESSION_COOKIE_NAME: &str = "__Host-app_session";
/// The refresh token cookie, sent only to the refresh endpoint.
pub const REFRESH_COOKIE_NAME: &str = "__Secure-app_refresh";
pub const REFRESH_COOKIE_PATH: &str = "/api/v1/auth/refresh";

pub fn build_session_cookie(jwt: &str, ttl: u64) -> String {
    session_cookie_string(jwt, ttl)
}

pub fn clear_session_cookie() -> String {
    session_cookie_string("", 0)
}

fn session_cookie_string(value: &str, max_age: u64) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}"
    )
}

pub fn build_refresh_cookie(token: &str, max_age: u64) -> String {
    refresh_cookie_string(token, max_age)
}

pub fn clear_refresh_cookie() -> String {
    refresh_cookie_string("", 0)
}

fn refresh_cookie_string(value: &str, max_age: u64) -> String {
    format!(
        "{REFRESH_COOKIE_NAME}={value}; HttpOnly; Secure; SameSite=Strict; Path={REFRESH_COOKIE_PATH}; Max-Age={max_age}"
    )
}

fn cookie_value<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    cookie_header.split(';').find_map(|part| {
        let value = part.trim().strip_prefix(name)?.strip_prefix('=')?;
        (!value.is_empty()).then_some(value)
    })
}

/// The access token in a `Cookie` header.
pub fn access_token_from_cookie_header(cookie_header: &str) -> Option<&str> {
    cookie_value(cookie_header, SESSION_COOKIE_NAME)
}

/// The refresh token in a `Cookie` header.
pub fn refresh_token_from_cookie_header(cookie_header: &str) -> Option<&str> {
    cookie_value(cookie_header, REFRESH_COOKIE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn password_sessions_have_no_second_factor_and_second_factor_sessions_do() {
        let keys = KeySet::generate("k1").unwrap();
        let (jwt, _, _) = access(&keys, 60, false);
        let claims = validate_access_token(&keys, &jwt).unwrap();
        assert_eq!(claims.amr, vec!["pwd"]);
        assert!(!claims.has_second_factor());
        assert!(!claims.second_factor_fresh(Utc::now().timestamp(), 300));

        let (jwt, _, _) = issue_access_token(
            &keys,
            60,
            Uuid::new_v4(),
            Uuid::new_v4(),
            false,
            SessionAuth::second_factor(AMR_OTP),
        )
        .unwrap();
        let claims = validate_access_token(&keys, &jwt).unwrap();
        assert_eq!(claims.amr, vec!["pwd", "otp"]);
        assert!(claims.has_second_factor());
        let now = Utc::now().timestamp();
        assert!(claims.second_factor_fresh(now, 300));
        assert!(!claims.second_factor_fresh(now + 301, 300));
        assert!(!claims.second_factor_fresh(now - 10, 300));
    }

    #[test]
    fn a_pending_token_has_its_own_audience_kind_and_lifetime() {
        let keys = KeySet::generate("k1").unwrap();
        let user = Uuid::new_v4();
        let (jwt, issued) = issue_pending(&keys, user).unwrap();
        let header = jsonwebtoken::decode_header(&jwt).unwrap();
        assert_eq!(header.alg, Algorithm::EdDSA);
        assert_eq!(header.kid.as_deref(), Some("k1"));
        let claims = validate_pending(&keys, &jwt).unwrap();
        assert_eq!(claims, issued);
        assert_eq!(claims.sub, user);
        assert_eq!(claims.exp - claims.iat, 300);
        assert_eq!(claims.iss, "helix");
        assert_eq!(claims.aud, "mfa_pending");
        assert_eq!(claims.typ, "mfa_pending");
        assert!(validate_access_token(&keys, &jwt).is_err());
        assert!(validate_pending(&keys, &access(&keys, 60, false).0).is_err());
    }

    #[test]
    fn pending_validation_rejects_expired_or_mismatched_claims() {
        let keys = KeySet::generate("k1").unwrap();
        let (_, claims) = issue_pending(&keys, Uuid::new_v4()).unwrap();
        for invalid in [
            PendingClaims {
                exp: claims.iat - 1,
                ..claims.clone()
            },
            PendingClaims {
                typ: TYP_TENANT_USER.into(),
                ..claims.clone()
            },
            PendingClaims {
                aud: AUDIENCE.into(),
                ..claims.clone()
            },
            PendingClaims {
                iss: "another-issuer".into(),
                ..claims.clone()
            },
        ] {
            assert!(validate_pending(&keys, &sign(&keys, &invalid).unwrap()).is_err());
        }
    }

    #[test]
    fn pending_validation_rejects_tampering_and_untrusted_signing_keys() {
        let keys = KeySet::generate("k1").unwrap();
        let (jwt, _) = issue_pending(&keys, Uuid::new_v4()).unwrap();
        let mut parts: Vec<_> = jwt.split('.').collect();
        parts[1] = "e30";
        assert!(validate_pending(&keys, &parts.join(".")).is_err());
        let other = KeySet::generate("k1").unwrap();
        assert!(validate_pending(&other, &jwt).is_err());
        assert!(issue_pending(&KeySet::empty(), Uuid::new_v4()).is_err());
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
            "__Host-app_session=jwt-value; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800"
        );
    }

    #[test]
    fn clear_session_cookie_attributes_are_exact() {
        assert_eq!(
            clear_session_cookie(),
            "__Host-app_session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
        );
    }

    // -- EdDSA access tokens ---------------------------------------------------------------

    use crate::keys::KeySet;

    fn access(keys: &KeySet, ttl: u64, platform: bool) -> (String, Uuid, Uuid) {
        let sid = Uuid::new_v4();
        let user = Uuid::new_v4();
        let (jwt, _, _) =
            issue_access_token(keys, ttl, user, sid, platform, SessionAuth::password()).unwrap();
        (jwt, user, sid)
    }

    #[test]
    fn an_access_token_carries_the_documented_claims_and_header() {
        let keys = KeySet::generate("k1").unwrap();
        let (jwt, user, sid) = access(&keys, 600, false);

        let header = jsonwebtoken::decode_header(&jwt).unwrap();
        assert_eq!(header.alg, Algorithm::EdDSA);
        assert_eq!(header.kid.as_deref(), Some("k1"));

        let claims = validate_access_token(&keys, &jwt).unwrap();
        assert_eq!(claims.sub, user);
        assert_eq!(claims.sid, sid);
        assert_eq!(claims.iss.as_deref(), Some(ISSUER));
        assert_eq!(claims.aud.as_deref(), Some(AUDIENCE));
        assert_eq!(claims.typ.as_deref(), Some(TYP_TENANT_USER));
        assert_eq!(claims.exp - claims.iat, 600);
        assert_eq!(claims.amr, vec!["pwd"]);

        let (platform_jwt, _, _) = access(&keys, 600, true);
        assert_eq!(
            validate_access_token(&keys, &platform_jwt)
                .unwrap()
                .typ
                .as_deref(),
            Some(TYP_PLATFORM_USER)
        );
    }

    #[test]
    fn an_access_token_is_refused_when_expired_tampered_or_signed_by_an_unknown_key() {
        let keys = KeySet::generate("k1").unwrap();
        assert!(validate_access_token(&keys, &access(&keys, 0, false).0).is_err());

        let (jwt, _, _) = access(&keys, 600, false);
        let mut parts: Vec<&str> = jwt.split('.').collect();
        parts[1] = if parts[1] == "e30" { "e31" } else { "e30" };
        assert!(validate_access_token(&keys, &parts.join(".")).is_err());

        // A key set that has never heard of k1 refuses it.
        let stranger = KeySet::generate("k2").unwrap();
        assert!(validate_access_token(&stranger, &jwt).is_err());
        // The same kid with a different key: the signature does not verify.
        let impostor = KeySet::generate("k1").unwrap();
        assert!(validate_access_token(&impostor, &jwt).is_err());
    }

    #[test]
    fn rotation_a_token_of_the_previous_key_verifies_until_that_key_is_dropped() {
        let old = KeySet::generate("old").unwrap();
        let (jwt, _, _) = access(&old, 600, false);
        let (pending, _) = issue_pending(&old, Uuid::new_v4()).unwrap();

        // The set after rotation: a new key signs, the old public key still verifies.
        let old_public = old.jwks()["keys"][0]["x"].as_str().unwrap().to_owned();
        let new_private = {
            let v: serde_json::Value =
                serde_json::from_str(&crate::keys::generated_key_json("new").unwrap()).unwrap();
            v[0]["private_key"].as_str().unwrap().to_owned()
        };
        let rotated = KeySet::from_json(
            &serde_json::json!([
                { "kid": "new", "private_key": new_private },
                { "kid": "old", "public_key": old_public },
            ])
            .to_string(),
        )
        .unwrap();
        assert_eq!(rotated.signing().unwrap().0, "new");
        assert!(
            validate_access_token(&rotated, &jwt).is_ok(),
            "old token still verifies"
        );
        assert!(validate_pending(&rotated, &pending).is_ok());
        let (fresh, _, _) = access(&rotated, 600, false);
        assert_eq!(
            jsonwebtoken::decode_header(&fresh).unwrap().kid.as_deref(),
            Some("new")
        );
        assert!(validate_access_token(&rotated, &fresh).is_ok());

        // Once the old key is dropped, its tokens stop.
        let only_new = KeySet::from_json(
            &serde_json::json!([{ "kid": "new", "private_key": new_private }]).to_string(),
        )
        .unwrap();
        assert!(validate_access_token(&only_new, &jwt).is_err());
        assert!(validate_pending(&only_new, &pending).is_err());
    }

    #[test]
    fn the_hmac_algorithms_and_none_cannot_pass_as_an_access_token() {
        let keys = KeySet::generate("k1").unwrap();
        let user = Uuid::new_v4();

        // An HS256 token, signed with the *public* key as the secret (the classic algorithm
        // confusion attack), with the right kid.
        let public = keys.jwks()["keys"][0]["x"].as_str().unwrap().to_owned();
        let claims = SessionClaims {
            sub: user,
            jti: Uuid::new_v4(),
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + 600,
            amr: vec![],
            mfa_at: None,
            sid: Uuid::new_v4(),
            iss: Some(ISSUER.into()),
            aud: Some(AUDIENCE.into()),
            typ: Some(TYP_TENANT_USER.into()),
        };
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("k1".into());
        let forged = encode(
            &header,
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(public.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(validate_access_token(&keys, &forged).is_err());
        // `alg: none`.
        let none = format!(
            "{}.{}.",
            base64_url(br#"{"alg":"none","typ":"JWT","kid":"k1"}"#),
            base64_url(serde_json::to_string(&claims).unwrap().as_bytes())
        );
        assert!(validate_access_token(&keys, &none).is_err());
    }

    fn base64_url(bytes: &[u8]) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(bytes)
    }

    #[test]
    fn a_token_without_a_session_id_or_with_the_wrong_audience_is_not_an_access_token() {
        let keys = KeySet::generate("k1").unwrap();
        let (kid, key) = keys.signing().unwrap();
        let base = SessionClaims {
            sub: Uuid::new_v4(),
            jti: Uuid::new_v4(),
            iat: Utc::now().timestamp(),
            exp: Utc::now().timestamp() + 600,
            amr: vec![],
            mfa_at: None,
            sid: Uuid::new_v4(),
            iss: Some(ISSUER.into()),
            aud: Some(AUDIENCE.into()),
            typ: Some(TYP_TENANT_USER.into()),
        };
        let sign = |claims: &SessionClaims| {
            let mut header = Header::new(Algorithm::EdDSA);
            header.kid = Some(kid.to_owned());
            encode(&header, claims, key).unwrap()
        };
        assert!(validate_access_token(&keys, &sign(&base)).is_ok());

        let mut no_sid = serde_json::to_value(&base).unwrap();
        no_sid.as_object_mut().unwrap().remove("sid");
        assert!(validate_access_token(&keys, &super::sign(&keys, &no_sid).unwrap()).is_err());
        let mut other_aud = base.clone();
        other_aud.aud = Some("someone-else".into());
        assert!(validate_access_token(&keys, &sign(&other_aud)).is_err());
        let mut other_iss = base.clone();
        other_iss.iss = Some("someone-else".into());
        assert!(validate_access_token(&keys, &sign(&other_iss)).is_err());
        let mut no_typ = base.clone();
        no_typ.typ = None;
        assert!(validate_access_token(&keys, &sign(&no_typ)).is_err());
        let mut odd_typ = base;
        odd_typ.typ = Some("admin".into());
        assert!(validate_access_token(&keys, &sign(&odd_typ)).is_err());
    }

    #[test]
    fn an_empty_key_set_signs_and_verifies_nothing() {
        let keys = KeySet::empty();
        assert!(issue_access_token(
            &keys,
            600,
            Uuid::new_v4(),
            Uuid::new_v4(),
            false,
            SessionAuth::password()
        )
        .is_err());
        let other = KeySet::generate("k1").unwrap();
        let (jwt, _, _) = access(&other, 600, false);
        assert!(validate_access_token(&keys, &jwt).is_err());
    }

    #[test]
    fn only_current_cookie_names_are_read() {
        assert_eq!(
            access_token_from_cookie_header("a=b; __Host-app_session=new; app_session=old"),
            Some("new")
        );
        assert_eq!(access_token_from_cookie_header("app_session=old"), None);
        assert_eq!(access_token_from_cookie_header("xapp_session=nope"), None);
        assert_eq!(access_token_from_cookie_header("__Host-app_session="), None);
        assert_eq!(
            refresh_token_from_cookie_header("__Secure-app_refresh=rt_abc; x=y"),
            Some("rt_abc")
        );
        assert_eq!(refresh_token_from_cookie_header("app_refresh=rt_abc"), None);
    }

    #[test]
    fn the_refresh_cookie_is_strict_scoped_to_the_refresh_path() {
        assert_eq!(
            build_refresh_cookie("rt_x", 604800),
            "__Secure-app_refresh=rt_x; HttpOnly; Secure; SameSite=Strict; Path=/api/v1/auth/refresh; Max-Age=604800"
        );
        assert_eq!(
            clear_refresh_cookie(),
            "__Secure-app_refresh=; HttpOnly; Secure; SameSite=Strict; Path=/api/v1/auth/refresh; Max-Age=0"
        );
    }
}
