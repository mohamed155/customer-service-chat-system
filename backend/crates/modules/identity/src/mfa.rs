//! Multi-factor authentication: TOTP enrolment, the second sign-in step, recovery codes, lockout.
//!
//! ADR-005 phase M1. A user has at most one TOTP factor. The secret is sealed with envelope
//! encryption under a key of its own (`user:<id>`, ADR-014) and is shown once, when enrolment
//! starts. A code is accepted for a time step later than the last one accepted, so it works once.
//! Wrong codes count against the factor; five in a row lock it for fifteen minutes. A recovery
//! code is a single-use stand-in for a code, kept only as an Argon2id hash.
//!
//! Every outcome that matters writes an `auth.mfa_*` audit row (no secret, code or hash in it).

use config::AppConfig;
use rand::RngExt;
use secrets::{Sealed, SecretBox, Vault};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{audit, password, session, totp};

/// Consecutive wrong codes before the factor locks.
pub const MAX_FAILURES: i32 = 5;
/// How long a locked factor refuses every code.
pub const LOCK_MINUTES: i64 = 15;
/// Recovery codes issued with each enrolment.
pub const RECOVERY_CODE_COUNT: usize = 10;
/// The name authenticator apps show for this service.
pub const ISSUER: &str = "Helix Support AI";

/// The keys that seal TOTP secrets, built from the configuration.
pub struct MfaKeys(Vault);

impl MfaKeys {
    pub fn from_config(pool: &PgPool, config: &AppConfig) -> Result<Self, String> {
        let vault = Vault::from_config(
            pool,
            None,
            config.secrets_root_key(),
            config.secrets_root_key_previous.as_deref(),
        )?;
        if vault.secret_box().is_none() {
            return Err("no root key is configured for sealing MFA secrets".into());
        }
        Ok(Self(vault))
    }

    fn secret_box(&self) -> &SecretBox {
        self.0
            .secret_box()
            .expect("checked when the keys were built")
    }
}

fn kek_id(user_id: Uuid) -> String {
    format!("user:{user_id}")
}

/// The scope a secret is sealed for, so a secret copied to another user's row does not open.
fn aad(user_id: Uuid) -> String {
    format!("mfa-totp|{user_id}")
}

#[derive(Debug, PartialEq, Eq)]
pub enum MfaError {
    /// The user already has a confirmed factor.
    AlreadyEnrolled,
    /// No factor to confirm (enrolment was not started, or it is confirmed already).
    NotEnrolling,
    /// The code or recovery code is wrong, was used, or the user has no factor.
    Invalid,
    /// The factor is locked; the number of seconds until it unlocks.
    Locked(i64),
    Internal(String),
}

impl From<sqlx::Error> for MfaError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

/// What a successful second step used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Otp,
    RecoveryCode,
}

impl Method {
    pub fn amr(self) -> &'static str {
        match self {
            Method::Otp => session::AMR_OTP,
            Method::RecoveryCode => session::AMR_RECOVERY,
        }
    }
}

/// What the user types in the second step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    Code(String),
    RecoveryCode(String),
}

pub struct Enrolment {
    /// The secret as base32, for typing into an app. Shown once.
    pub secret: String,
    /// The `otpauth://` URI a QR code carries. Shown once.
    pub otpauth_uri: String,
}

/// Whether the user has a confirmed factor, so sign-in needs a second step.
pub async fn is_enrolled(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM mfa_factors WHERE user_id = $1 AND confirmed_at IS NOT NULL)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

/// How many recovery codes the user still has.
pub async fn recovery_codes_remaining(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM mfa_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}

/// Start (or restart) enrolment: a new secret, unconfirmed until [`confirm_enrolment`].
pub async fn begin_enrolment(
    pool: &PgPool,
    keys: &MfaKeys,
    user_id: Uuid,
    account_label: &str,
) -> Result<Enrolment, MfaError> {
    if is_enrolled(pool, user_id).await? {
        return Err(MfaError::AlreadyEnrolled);
    }
    let secret = totp::generate_secret();
    let sealed: Sealed = keys
        .secret_box()
        .seal(&kek_id(user_id), &aad(user_id), &secret)
        .await
        .map_err(|e| MfaError::Internal(e.to_string()))?;

    let mut tx = pool.begin().await?;
    // A started but unconfirmed enrolment is replaced, so the user can start again.
    sqlx::query("DELETE FROM mfa_factors WHERE user_id = $1 AND confirmed_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let inserted = sqlx::query(
        "INSERT INTO mfa_factors \
         (user_id, kind, ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm) \
         VALUES ($1, 'totp', $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .bind(&sealed.wrapped_dek)
    .bind(&sealed.kek_id)
    .bind(sealed.kek_version)
    .bind(&sealed.algorithm)
    .execute(&mut *tx)
    .await;
    match inserted {
        Ok(_) => {}
        // A confirmed factor appeared meanwhile (two enrolments at once).
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(MfaError::AlreadyEnrolled);
        }
        Err(e) => return Err(e.into()),
    }
    tx.commit().await?;

    Ok(Enrolment {
        secret: totp::base32_encode(&secret),
        otpauth_uri: totp::otpauth_uri(ISSUER, account_label, &secret),
    })
}

struct Factor {
    id: Uuid,
    sealed: Sealed,
    confirmed: bool,
    locked_for_seconds: i64,
}

async fn load_factor(pool: &PgPool, user_id: Uuid) -> Result<Option<Factor>, MfaError> {
    type Row = (
        Uuid,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        String,
        i32,
        String,
        bool,
        i64,
    );
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm, \
                confirmed_at IS NOT NULL, \
                GREATEST(0, CEIL(EXTRACT(EPOCH FROM (COALESCE(locked_until, now()) - now()))))::bigint \
         FROM mfa_factors WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(
            id,
            ciphertext,
            nonce,
            wrapped_dek,
            kek_id,
            kek_version,
            algorithm,
            confirmed,
            locked,
        )| {
            Factor {
                id,
                sealed: Sealed {
                    ciphertext,
                    nonce,
                    wrapped_dek,
                    kek_id,
                    kek_version,
                    algorithm,
                },
                confirmed,
                locked_for_seconds: locked,
            }
        },
    ))
}

/// The opened TOTP secret, in a buffer that is wiped on drop.
async fn open_secret(
    keys: &MfaKeys,
    user_id: Uuid,
    factor: &Factor,
) -> Result<zeroize::Zeroizing<Vec<u8>>, MfaError> {
    keys.secret_box()
        .open(&aad(user_id), &factor.sealed)
        .await
        .map_err(|e| MfaError::Internal(e.to_string()))
}

/// Confirm enrolment with a code from the app. On success the factor is active and the user gets
/// their recovery codes, which are never shown again.
pub async fn confirm_enrolment(
    pool: &PgPool,
    keys: &MfaKeys,
    user_id: Uuid,
    code: &str,
    now_unix: u64,
) -> Result<Vec<String>, MfaError> {
    let factor = load_factor(pool, user_id)
        .await?
        .filter(|f| !f.confirmed)
        .ok_or(MfaError::NotEnrolling)?;
    if factor.locked_for_seconds > 0 {
        return Err(MfaError::Locked(factor.locked_for_seconds));
    }
    let secret = open_secret(keys, user_id, &factor).await?;
    let Some(step) = totp::matching_step(&secret, code, now_unix) else {
        record_failure(pool, user_id, factor.id, "enrol").await?;
        return Err(MfaError::Invalid);
    };

    let (codes, hashes) = new_recovery_codes().await?;
    let mut tx = pool.begin().await?;
    let confirmed = sqlx::query(
        "UPDATE mfa_factors SET confirmed_at = now(), last_used_step = $2, failed_attempts = 0, \
                locked_until = NULL \
         WHERE id = $1 AND confirmed_at IS NULL",
    )
    .bind(factor.id)
    .bind(step as i64)
    .execute(&mut *tx)
    .await?;
    if confirmed.rows_affected() != 1 {
        return Err(MfaError::NotEnrolling);
    }
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for hash in &hashes {
        sqlx::query("INSERT INTO mfa_recovery_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let resource = user_id.to_string();
    audit::record(
        pool,
        "auth.mfa_enrolled",
        Some(user_id),
        Some(&resource),
        &serde_json::json!({ "method": "totp", "recovery_codes": codes.len() }),
    )
    .await;
    Ok(codes)
}

/// The second sign-in step. Checks the lockout, the code (or recovery code), replay, and records
/// the outcome.
pub async fn verify(
    pool: &PgPool,
    keys: &MfaKeys,
    user_id: Uuid,
    credential: &Credential,
    now_unix: u64,
) -> Result<Method, MfaError> {
    let Some(factor) = load_factor(pool, user_id).await?.filter(|f| f.confirmed) else {
        return Err(MfaError::Invalid);
    };
    if factor.locked_for_seconds > 0 {
        return Err(MfaError::Locked(factor.locked_for_seconds));
    }

    let mut replayed = false;
    let method = match credential {
        Credential::Code(code) => {
            let secret = open_secret(keys, user_id, &factor).await?;
            match totp::matching_step(&secret, code, now_unix) {
                Some(step) if accept_step(pool, factor.id, step).await? => Some(Method::Otp),
                // A right code that was already used. It is refused and audited, but it does not
                // count toward the lockout: a code someone saw over a shoulder must not be a way
                // to lock the owner out.
                Some(_) => {
                    replayed = true;
                    None
                }
                None => None,
            }
        }
        Credential::RecoveryCode(code) => {
            if use_recovery_code(pool, user_id, code).await? {
                Some(Method::RecoveryCode)
            } else {
                None
            }
        }
    };

    let resource = user_id.to_string();
    match method {
        Some(method) => {
            sqlx::query(
                "UPDATE mfa_factors SET failed_attempts = 0, locked_until = NULL WHERE id = $1",
            )
            .bind(factor.id)
            .execute(pool)
            .await?;
            let (action, name) = match method {
                Method::Otp => ("auth.mfa_verified", "totp"),
                Method::RecoveryCode => ("auth.mfa_recovery_used", "recovery_code"),
            };
            let remaining = recovery_codes_remaining(pool, user_id).await.unwrap_or(0);
            audit::record(
                pool,
                action,
                Some(user_id),
                Some(&resource),
                &serde_json::json!({ "method": name, "recovery_codes_remaining": remaining }),
            )
            .await;
            Ok(method)
        }
        None if replayed => {
            audit::record(
                pool,
                "auth.mfa_failed",
                Some(user_id),
                Some(&resource),
                &serde_json::json!({ "stage": "signin", "reason": "replayed_code" }),
            )
            .await;
            Err(MfaError::Invalid)
        }
        None => {
            record_failure(pool, user_id, factor.id, "signin").await?;
            Err(MfaError::Invalid)
        }
    }
}

/// Accept a time step only if it is later than the last accepted one. One statement, so two
/// requests with the same code cannot both win.
async fn accept_step(pool: &PgPool, factor_id: Uuid, step: u64) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE mfa_factors SET last_used_step = $2 \
         WHERE id = $1 AND (last_used_step IS NULL OR last_used_step < $2)",
    )
    .bind(factor_id)
    .bind(step as i64)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Count a wrong attempt; lock the factor on the fifth in a row. Audited.
async fn record_failure(
    pool: &PgPool,
    user_id: Uuid,
    factor_id: Uuid,
    stage: &str,
) -> Result<(), sqlx::Error> {
    let (attempts, locked): (i32, bool) = sqlx::query_as(
        "UPDATE mfa_factors SET \
            failed_attempts = CASE WHEN failed_attempts + 1 >= $2 THEN 0 ELSE failed_attempts + 1 END, \
            locked_until = CASE WHEN failed_attempts + 1 >= $2 \
                                THEN now() + make_interval(mins => $3::int) ELSE locked_until END \
         WHERE id = $1 \
         RETURNING failed_attempts, locked_until IS NOT NULL AND locked_until > now()",
    )
    .bind(factor_id)
    .bind(MAX_FAILURES)
    .bind(LOCK_MINUTES as i32)
    .fetch_one(pool)
    .await?;
    let resource = user_id.to_string();
    audit::record(
        pool,
        "auth.mfa_failed",
        Some(user_id),
        Some(&resource),
        &serde_json::json!({ "stage": stage, "attempts_since_success": attempts }),
    )
    .await;
    if locked {
        audit::record(
            pool,
            "auth.mfa_locked",
            Some(user_id),
            Some(&resource),
            &serde_json::json!({ "minutes": LOCK_MINUTES }),
        )
        .await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Recovery codes
// ---------------------------------------------------------------------------

/// `XXXXX-XXXXX` from the base32 alphabet (50 bits), the form that is easy to read and type.
fn generate_recovery_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    let mut code = String::with_capacity(11);
    for i in 0..10 {
        if i == 5 {
            code.push('-');
        }
        code.push(ALPHABET[rng.random_range(0..ALPHABET.len())] as char);
    }
    code
}

/// What a person types in, reduced to what the code is made of: upper case, no dashes or spaces.
fn normalize_recovery_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase()
}

async fn new_recovery_codes() -> Result<(Vec<String>, Vec<String>), MfaError> {
    let codes: Vec<String> = (0..RECOVERY_CODE_COUNT)
        .map(|_| generate_recovery_code())
        .collect();
    let to_hash: Vec<String> = codes.iter().map(|c| normalize_recovery_code(c)).collect();
    let hashes = tokio::task::spawn_blocking(move || {
        to_hash
            .iter()
            .map(|c| password::hash_password(c).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()
    })
    .await
    .map_err(|e| MfaError::Internal(e.to_string()))?
    .map_err(MfaError::Internal)?;
    Ok((codes, hashes))
}

/// Check a recovery code against the user's unused ones and burn the match. Every unused hash is
/// checked, so the time taken does not say which one matched.
async fn use_recovery_code(pool: &PgPool, user_id: Uuid, input: &str) -> Result<bool, MfaError> {
    let normalized = normalize_recovery_code(input);
    if normalized.len() != 10 {
        return Ok(false);
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, code_hash FROM mfa_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(false);
    }
    let matched = tokio::task::spawn_blocking(move || {
        let mut found = None;
        for (id, hash) in &rows {
            if matches!(password::verify_password(&normalized, hash), Ok(true)) {
                found = Some(*id);
            }
        }
        found
    })
    .await
    .map_err(|e| MfaError::Internal(e.to_string()))?;
    let Some(id) = matched else {
        return Ok(false);
    };
    // One statement: two requests with the same code cannot both burn it.
    let burned = sqlx::query(
        "UPDATE mfa_recovery_codes SET used_at = now() WHERE id = $1 AND used_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(burned.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_codes_have_the_documented_shape_and_are_random() {
        let a = generate_recovery_code();
        assert_eq!(a.len(), 11);
        assert_eq!(a.as_bytes()[5], b'-');
        assert!(a
            .chars()
            .filter(|c| *c != '-')
            .all(|c| "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(c)));
        assert_ne!(a, generate_recovery_code());
    }

    #[test]
    fn typed_recovery_codes_are_normalised() {
        assert_eq!(normalize_recovery_code("abcde-fghjk"), "ABCDEFGHJK");
        assert_eq!(normalize_recovery_code(" ABCDE FGHJK "), "ABCDEFGHJK");
        assert_eq!(normalize_recovery_code("ABCDE-FGHJK"), "ABCDEFGHJK");
    }

    #[test]
    fn the_secret_is_bound_to_its_user() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_ne!(aad(a), aad(b));
        assert_ne!(kek_id(a), kek_id(b));
        assert!(kek_id(a).starts_with("user:"));
    }

    #[test]
    fn methods_map_to_amr_values() {
        assert_eq!(Method::Otp.amr(), "otp");
        assert_eq!(Method::RecoveryCode.amr(), "rcv");
    }
}
