//! Sessions and rotating refresh tokens (ADR-005 §3).
//!
//! A sign-in creates a session (`auth_sessions`) and a first refresh token. The access token the
//! browser sends on every request lives ten minutes and carries the session id as `sid`. When it
//! expires the dashboard exchanges the refresh token for a new pair; the old refresh token is then
//! used up. Presenting a used refresh token again is a replay, so the whole session is revoked.
//!
//! Revoking a session (`auth_sessions.revoked_at`) is how every kind of sign-out works: logout,
//! reuse detection, a password reset, a removed member. The middleware checks the row on every
//! request, so a revoked session stops at once, not when its access token expires.

use axum::http::{header, HeaderMap};
use chrono::{DateTime, Utc};
use config::Environment;
use kernel::ApiError;
use rand::RngExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    audit,
    keys::{self, KeySet},
    session::{self, SessionAuth},
};

pub use crate::infrastructure::session_status::active_session_ids;

/// Prefix of every refresh token, so a leaked one is recognisable in a log or a scanner.
pub const REFRESH_PREFIX: &str = "rt_";

/// A refresh token that was used less than this many seconds ago and was answered by a successor
/// is the second tab of the same refresh, not a replay (ADR-005 §3, concurrency grace).
pub const REUSE_GRACE_SECONDS: i64 = 10;

/// How long a session lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifetimes {
    /// A session nobody refreshes for this long is over; each refresh extends it.
    pub idle_seconds: i64,
    /// A session never lasts longer than this, however often it is refreshed.
    pub absolute_seconds: i64,
}

/// Tenant users: 12 hours idle, 7 days at most.
pub const TENANT_LIFETIMES: Lifetimes = Lifetimes {
    idle_seconds: 12 * 3600,
    absolute_seconds: 7 * 24 * 3600,
};

/// Platform users can reach every tenant, so their sessions are short: 1 hour idle, 12 hours at most.
pub const PLATFORM_LIFETIMES: Lifetimes = Lifetimes {
    idle_seconds: 3600,
    absolute_seconds: 12 * 3600,
};

pub fn lifetimes(platform: bool) -> Lifetimes {
    if platform {
        PLATFORM_LIFETIMES
    } else {
        TENANT_LIFETIMES
    }
}

/// Why a session ended, recorded in `auth_sessions.revoke_reason`.
pub mod reason {
    pub const LOGOUT: &str = "logout";
    pub const REFRESH_REUSE: &str = "refresh_reuse";
    /// A new session replaced it (the second factor was set up).
    pub const REPLACED: &str = "replaced";
    pub const PASSWORD_RESET: &str = "password_reset";
    pub const MEMBER_REMOVED: &str = "member_removed";
    pub const MFA_RESET: &str = "mfa_reset";
}

#[derive(Debug)]
pub enum SessionError {
    Database(sqlx::Error),
    Token(jsonwebtoken::errors::Error),
    /// The user no longer exists or was deleted.
    NoSuchUser,
    ChallengeUsed,
    StaleProof,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Token(e) => write!(f, "token error: {e}"),
            Self::NoSuchUser => f.write_str("no such user"),
            Self::ChallengeUsed => f.write_str("MFA challenge already used"),
            Self::StaleProof => f.write_str("session proof was revoked"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<sqlx::Error> for SessionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<jsonwebtoken::errors::Error> for SessionError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        Self::Token(error)
    }
}

/// What is recorded about the device a session was started from. Informational only: nothing
/// decides anything from it.
#[derive(Debug, Clone, Default)]
pub struct ClientMeta {
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

impl ClientMeta {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let text = |name: header::HeaderName, max: usize| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.chars().take(max).collect::<String>())
        };
        Self {
            user_agent: text(header::USER_AGENT, 256),
            ip: headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|v| v.trim().chars().take(64).collect::<String>())
                .filter(|v| !v.is_empty()),
        }
    }
}

/// A session that was just started or refreshed: the two tokens to hand to the browser.
#[derive(Debug, Clone)]
pub struct Issued {
    pub sid: Uuid,
    pub access_token: String,
    pub access_ttl: u64,
    pub refresh_token: String,
    /// Seconds until the session's absolute limit: the `Max-Age` of the refresh cookie.
    pub refresh_max_age: u64,
}

impl Issued {
    /// The access and refresh cookies to send to the browser.
    pub fn set_cookies(&self) -> Vec<String> {
        vec![
            session::build_session_cookie(&self.access_token, self.access_ttl),
            session::build_refresh_cookie(&self.refresh_token, self.refresh_max_age),
        ]
    }
}

/// Clear the access and refresh cookies.
pub fn clear_cookies() -> Vec<String> {
    vec![
        session::clear_session_cookie(),
        session::clear_refresh_cookie(),
    ]
}

fn new_refresh_token() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes[..]);
    format!("{REFRESH_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn auth_from_row(amr: Vec<String>, mfa_at: Option<DateTime<Utc>>) -> SessionAuth {
    SessionAuth {
        amr,
        mfa_at: mfa_at.map(|t| t.timestamp()),
    }
}

/// Starts a session for `user_id` and issues its first access and refresh tokens.
pub async fn start(
    pool: &PgPool,
    keys: &KeySet,
    user_id: Uuid,
    auth: SessionAuth,
    meta: &ClientMeta,
) -> Result<Issued, SessionError> {
    start_session(pool, keys, user_id, auth, meta, SessionProof::NewAccount).await
}

/// A pending challenge still represents the current password and has never created a session.
pub async fn challenge_available(
    pool: &PgPool,
    challenge: &session::PendingClaims,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM users
             WHERE id = $1 AND deleted_at IS NULL AND password_reset_at IS NOT DISTINCT FROM $2
         ) AND NOT EXISTS (SELECT 1 FROM auth_sessions WHERE mfa_challenge_id = $3)",
    )
    .bind(challenge.sub)
    .bind(challenge.password_reset_at)
    .bind(challenge.jti)
    .fetch_one(pool)
    .await
}

/// A completed challenge can start only one session. The unique index enforces this even
/// when concurrent requests both passed the early check.
pub async fn start_from_challenge(
    pool: &PgPool,
    environment: &Environment,
    challenge: &session::PendingClaims,
    auth: SessionAuth,
    headers: &HeaderMap,
) -> Result<Vec<String>, ApiError> {
    start_session(
        pool,
        keys::keys_for(environment),
        challenge.sub,
        auth,
        &ClientMeta::from_headers(headers),
        SessionProof::Challenge(challenge),
    )
    .await
    .map(|issued| issued.set_cookies())
    .map_err(|error| match error {
        SessionError::ChallengeUsed | SessionError::StaleProof => {
            ApiError::unauthenticated("Sign in again")
        }
        other => ApiError::internal_error_from("Session token issuance failed", other),
    })
}

enum SessionProof<'a> {
    NewAccount,
    Password(Option<DateTime<Utc>>),
    Challenge(&'a session::PendingClaims),
    ExistingSession(Uuid),
}

pub(crate) async fn start_for_password(
    pool: &PgPool,
    environment: &Environment,
    user_id: Uuid,
    password_reset_at: Option<DateTime<Utc>>,
    headers: &HeaderMap,
) -> Result<Vec<String>, SessionError> {
    start_session(
        pool,
        keys::keys_for(environment),
        user_id,
        SessionAuth::password(),
        &ClientMeta::from_headers(headers),
        SessionProof::Password(password_reset_at),
    )
    .await
    .map(|issued| issued.set_cookies())
}

pub(crate) async fn start_replacing_session(
    pool: &PgPool,
    environment: &Environment,
    user_id: Uuid,
    auth: SessionAuth,
    headers: &HeaderMap,
    old_sid: Option<Uuid>,
) -> Result<Vec<String>, ApiError> {
    let proof = old_sid.map_or(SessionProof::NewAccount, SessionProof::ExistingSession);
    start_session(
        pool,
        keys::keys_for(environment),
        user_id,
        auth,
        &ClientMeta::from_headers(headers),
        proof,
    )
    .await
    .map(|issued| issued.set_cookies())
    .map_err(|error| match error {
        SessionError::StaleProof => ApiError::unauthenticated("Sign in again"),
        other => ApiError::internal_error_from("Session issuance failed", other),
    })
}

async fn start_session(
    pool: &PgPool,
    keys: &KeySet,
    user_id: Uuid,
    auth: SessionAuth,
    meta: &ClientMeta,
    proof: SessionProof<'_>,
) -> Result<Issued, SessionError> {
    let mut tx = pool.begin().await?;
    // The password reset transaction locks the same user before revoking sessions. A proof that
    // predates the reset either creates a session before revocation or is refused after it.
    let user: Option<(bool, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT platform_role IS NOT NULL, password_reset_at FROM users
         WHERE id = $1 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (platform, password_reset_at) = user.ok_or(SessionError::NoSuchUser)?;
    let challenge_id = match proof {
        SessionProof::Password(proven_at) if password_reset_at != proven_at => {
            return Err(SessionError::StaleProof);
        }
        SessionProof::Challenge(challenge) => {
            if password_reset_at != challenge.password_reset_at {
                return Err(SessionError::StaleProof);
            }
            Some(challenge.jti)
        }
        SessionProof::ExistingSession(sid) => {
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM auth_sessions WHERE id = $1 AND user_id = $2
                 AND revoked_at IS NULL AND idle_expires_at > now() AND absolute_expires_at > now())",
            )
            .bind(sid)
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
            if !active {
                return Err(SessionError::StaleProof);
            }
            None
        }
        _ => None,
    };
    let life = lifetimes(platform);

    let (sid, absolute_left): (Uuid, i64) = sqlx::query_as(
        r#"
        INSERT INTO auth_sessions
            (user_id, amr, mfa_at, idle_expires_at, absolute_expires_at, user_agent, ip, mfa_challenge_id)
        VALUES ($1, $2, $3, now() + make_interval(secs => $4), now() + make_interval(secs => $5), $6, $7, $8)
        ON CONFLICT (mfa_challenge_id) WHERE mfa_challenge_id IS NOT NULL DO NOTHING
        RETURNING id, $5::bigint
        "#,
    )
    .bind(user_id)
    .bind(&auth.amr)
    .bind(auth.mfa_at.and_then(|t| DateTime::from_timestamp(t, 0)))
    .bind(life.idle_seconds as f64)
    .bind(life.absolute_seconds as f64)
    .bind(meta.user_agent.as_deref())
    .bind(meta.ip.as_deref())
    .bind(challenge_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(SessionError::ChallengeUsed)?;

    let refresh_token = new_refresh_token();
    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (session_id, token_hash, expires_at)
        VALUES ($1, $2, now() + make_interval(secs => $3))
        "#,
    )
    .bind(sid)
    .bind(hash_token(&refresh_token))
    .bind(life.idle_seconds as f64)
    .execute(&mut *tx)
    .await?;

    let ttl = session::access_ttl_seconds();
    let (access_token, _, _) =
        session::issue_access_token(keys, ttl, user_id, sid, platform, auth)?;
    tx.commit().await?;

    Ok(Issued {
        sid,
        access_token,
        access_ttl: ttl,
        refresh_token,
        refresh_max_age: absolute_left.max(0) as u64,
    })
}

/// [`start`] for a request handler: returns the `Set-Cookie` values, or the error response.
pub async fn start_for_response(
    pool: &PgPool,
    environment: &Environment,
    user_id: Uuid,
    auth: SessionAuth,
    headers: &HeaderMap,
) -> Result<Vec<String>, ApiError> {
    let keys = keys::keys_for(environment);
    start(
        pool,
        keys,
        user_id,
        auth,
        &ClientMeta::from_headers(headers),
    )
    .await
    .map(|issued| issued.set_cookies())
    .map_err(|e| ApiError::internal_error_from("Session issuance failed", e))
}

/// What exchanging a refresh token came to.
#[derive(Debug)]
pub enum Refresh {
    /// A new access token and a new refresh token; the presented one is used up.
    Rotated(Issued),
    /// The presented token was rotated a moment ago by another request of the same browser (two
    /// tabs). Only a new access token is issued: the winning request already set the new refresh
    /// cookie, which both tabs share, and the server never keeps a token's plaintext.
    Concurrent {
        access_token: String,
        access_ttl: u64,
    },
    /// Unknown, expired, or the session is over. Nothing was changed.
    Rejected,
    /// A used token came back outside the grace window: the session was revoked.
    Reuse,
}

struct SessionRow {
    user_id: Uuid,
    amr: Vec<String>,
    mfa_at: Option<DateTime<Utc>>,
    platform: bool,
    active: bool,
}

const SESSION_SQL: &str = r#"
    SELECT s.user_id, s.amr, s.mfa_at, u.platform_role IS NOT NULL,
           (s.revoked_at IS NULL AND s.idle_expires_at > now() AND s.absolute_expires_at > now())
    FROM auth_sessions s
    JOIN users u ON u.id = s.user_id AND u.deleted_at IS NULL
    WHERE s.id = $1
"#;

type SessionRowTuple = (Uuid, Vec<String>, Option<DateTime<Utc>>, bool, bool);

fn session_row(row: Option<SessionRowTuple>) -> Option<SessionRow> {
    row.map(|(user_id, amr, mfa_at, platform, active)| SessionRow {
        user_id,
        amr,
        mfa_at,
        platform,
        active,
    })
}

/// The session, locked for the rest of the transaction.
async fn lock_session(tx: &mut sqlx::PgConnection, sid: Uuid) -> sqlx::Result<Option<SessionRow>> {
    let row: Option<SessionRowTuple> = sqlx::query_as(
        r#"
    SELECT s.user_id, s.amr, s.mfa_at, u.platform_role IS NOT NULL,
           (s.revoked_at IS NULL AND s.idle_expires_at > now() AND s.absolute_expires_at > now())
    FROM auth_sessions s
    JOIN users u ON u.id = s.user_id AND u.deleted_at IS NULL
    WHERE s.id = $1 FOR UPDATE OF s
"#,
    )
    .bind(sid)
    .fetch_optional(tx)
    .await?;
    Ok(session_row(row))
}

async fn load_session(pool: &PgPool, sid: Uuid) -> sqlx::Result<Option<SessionRow>> {
    let row: Option<SessionRowTuple> = sqlx::query_as(SESSION_SQL)
        .bind(sid)
        .fetch_optional(pool)
        .await?;
    Ok(session_row(row))
}

/// Exchanges a refresh token for a new pair (`POST /auth/refresh`).
pub async fn refresh(
    pool: &PgPool,
    keys: &KeySet,
    presented: &str,
) -> Result<Refresh, SessionError> {
    let hash = hash_token(presented);
    let ttl = session::access_ttl_seconds();

    let mut tx = pool.begin().await?;

    // Use the token up in one statement: of two requests presenting the same token, one gets a row
    // back and the other waits for it, then gets none.
    let consumed: Option<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        UPDATE refresh_tokens SET used_at = now()
        WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
        RETURNING id, session_id
        "#,
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((token_id, sid)) = consumed {
        let Some(row) = lock_session(&mut tx, sid).await?.filter(|r| r.active) else {
            // The session is over; the token was not really spent.
            tx.rollback().await?;
            return Ok(Refresh::Rejected);
        };
        let life = lifetimes(row.platform);
        let (absolute_left,): (i64,) = sqlx::query_as(
            r#"
            UPDATE auth_sessions
               SET last_seen_at = now(),
                   idle_expires_at = LEAST(now() + make_interval(secs => $2), absolute_expires_at)
             WHERE id = $1
            RETURNING EXTRACT(EPOCH FROM (absolute_expires_at - now()))::bigint
            "#,
        )
        .bind(sid)
        .bind(life.idle_seconds as f64)
        .fetch_one(&mut *tx)
        .await?;

        let refresh_token = new_refresh_token();
        sqlx::query(
            r#"
            INSERT INTO refresh_tokens (session_id, token_hash, parent_id, expires_at)
            SELECT $1, $2, $3, idle_expires_at FROM auth_sessions WHERE id = $1
            "#,
        )
        .bind(sid)
        .bind(hash_token(&refresh_token))
        .bind(token_id)
        .execute(&mut *tx)
        .await?;

        let (access_token, _, _) = session::issue_access_token(
            keys,
            ttl,
            row.user_id,
            sid,
            row.platform,
            auth_from_row(row.amr, row.mfa_at),
        )?;
        tx.commit().await?;
        return Ok(Refresh::Rotated(Issued {
            sid,
            access_token,
            access_ttl: ttl,
            refresh_token,
            refresh_max_age: absolute_left.max(0) as u64,
        }));
    }

    // Not spendable: unknown, expired, or already used.
    let known: Option<(Uuid, Option<DateTime<Utc>>, bool, bool)> = sqlx::query_as(
        r#"
        SELECT t.session_id, t.used_at,
               COALESCE(t.used_at > now() - make_interval(secs => $2), false) AS recent,
               EXISTS (SELECT 1 FROM refresh_tokens c WHERE c.parent_id = t.id) AS answered
        FROM refresh_tokens t
        WHERE t.token_hash = $1
        "#,
    )
    .bind(&hash)
    .bind(REUSE_GRACE_SECONDS as f64)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let Some((sid, Some(_used_at), recent, answered)) = known else {
        // Unknown, or never used and just expired: nothing to defend.
        return Ok(Refresh::Rejected);
    };

    let Some(row) = load_session(pool, sid).await? else {
        return Ok(Refresh::Rejected);
    };
    if !row.active {
        return Ok(Refresh::Rejected);
    }

    if recent && answered {
        let (access_token, _, _) = session::issue_access_token(
            keys,
            ttl,
            row.user_id,
            sid,
            row.platform,
            auth_from_row(row.amr, row.mfa_at),
        )?;
        return Ok(Refresh::Concurrent {
            access_token,
            access_ttl: ttl,
        });
    }

    // A used token again, outside the grace window: someone has a copy.
    if revoke_session(pool, sid, reason::REFRESH_REUSE).await? {
        let resource = sid.to_string();
        audit::record(
            pool,
            "auth.refresh_reuse_detected",
            Some(row.user_id),
            Some(resource.as_str()),
            &json!({ "sid": sid }),
        )
        .await;
    }
    Ok(Refresh::Reuse)
}

/// How long an ended session and its refresh tokens are kept, to look into who signed in when. The
/// user agent and address on a session are what the client sent (the address comes from
/// `X-Forwarded-For`, which only a trusted proxy makes reliable), so they are hints, not evidence.
pub const ENDED_SESSION_RETENTION_DAYS: i32 = 30;

/// Deletes the sessions that ended (revoked, or past their absolute limit) more than
/// [`ENDED_SESSION_RETENTION_DAYS`] days ago, with their refresh tokens. A live session keeps every
/// refresh token it ever had: a replay of an old one is how theft is noticed. Returns how many
/// sessions were deleted.
pub async fn sweep_ended(pool: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query(
        r#"
        DELETE FROM auth_sessions
        WHERE COALESCE(revoked_at, absolute_expires_at) < now() - make_interval(days => $1)
          AND (revoked_at IS NOT NULL OR absolute_expires_at < now())
        "#,
    )
    .bind(ENDED_SESSION_RETENTION_DAYS)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Ends one session. Returns whether it was still live.
pub async fn revoke_session(pool: &PgPool, sid: Uuid, why: &str) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE auth_sessions SET revoked_at = now(), revoke_reason = $2 \
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(sid)
    .bind(why)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Ends every live session of `user_id`, except `keep` when given. Returns how many ended.
pub async fn revoke_user_sessions(
    pool: &PgPool,
    user_id: Uuid,
    why: &str,
    keep: Option<Uuid>,
) -> sqlx::Result<u64> {
    let mut conn = pool.acquire().await?;
    revoke_user_sessions_in(&mut conn, user_id, why, keep).await
}

/// [`revoke_user_sessions`] on a connection the caller owns, so it can be part of a transaction: a
/// change that must end the sessions (a member who lost their last workspace) commits together
/// with their revocation, or not at all.
pub async fn revoke_user_sessions_in(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    why: &str,
    keep: Option<Uuid>,
) -> sqlx::Result<u64> {
    Ok(sqlx::query(
        "UPDATE auth_sessions SET revoked_at = now(), revoke_reason = $2 \
         WHERE user_id = $1 AND revoked_at IS NULL AND ($3::uuid IS NULL OR id <> $3)",
    )
    .bind(user_id)
    .bind(why)
    .bind(keep)
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

/// Ends the session of the current access token.
pub async fn revoke_claims(
    pool: &PgPool,
    claims: &session::SessionClaims,
    why: &str,
) -> sqlx::Result<()> {
    revoke_session(pool, claims.sid, why).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_token_is_prefixed_long_and_hashed_to_32_bytes() {
        let a = new_refresh_token();
        let b = new_refresh_token();
        assert!(a.starts_with(REFRESH_PREFIX));
        assert_ne!(a, b);
        assert_eq!(a.len(), REFRESH_PREFIX.len() + 43, "256 bits in base64url");
        assert_eq!(hash_token(&a).len(), 32);
        assert_ne!(hash_token(&a), hash_token(&b));
    }

    #[test]
    fn platform_sessions_are_shorter_than_tenant_sessions() {
        const {
            assert!(PLATFORM_LIFETIMES.idle_seconds < TENANT_LIFETIMES.idle_seconds);
            assert!(PLATFORM_LIFETIMES.absolute_seconds < TENANT_LIFETIMES.absolute_seconds);
        }
        assert_eq!(lifetimes(true), PLATFORM_LIFETIMES);
        assert_eq!(lifetimes(false), TENANT_LIFETIMES);
    }

    #[test]
    fn the_cookies_of_a_started_session_are_exact() {
        let issued = Issued {
            sid: Uuid::nil(),
            access_token: "jwt".into(),
            access_ttl: 600,
            refresh_token: "rt_x".into(),
            refresh_max_age: 604800,
        };
        assert_eq!(
            issued.set_cookies(),
            vec![
                "__Host-app_session=jwt; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600",
                "__Secure-app_refresh=rt_x; HttpOnly; Secure; SameSite=Strict; Path=/api/v1/auth/refresh; Max-Age=604800",
            ]
        );
    }

    #[test]
    fn sign_out_clears_every_cookie_a_session_can_have() {
        let cleared = clear_cookies().join("\n");
        assert!(cleared.contains("__Host-app_session=;"));
        assert!(cleared.contains("__Secure-app_refresh=;"));
    }

    #[test]
    fn client_meta_is_cut_to_a_sane_length() {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, "a".repeat(1000).parse().unwrap());
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        let meta = ClientMeta::from_headers(&headers);
        assert_eq!(meta.user_agent.unwrap().len(), 256);
        assert_eq!(meta.ip.as_deref(), Some("203.0.113.9"));
    }
}
