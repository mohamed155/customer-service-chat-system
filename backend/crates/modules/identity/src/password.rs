//! Password hashing: Argon2id with parameters pinned in one place (ADR-005 §1).
//!
//! Every hash this module produces, and the dummy hash a login for an unknown email verifies
//! against, comes from [`argon2`], so the two cost the same. A stored hash made with other
//! parameters still verifies (the parameters travel in the PHC string) and is replaced on the
//! next successful login ([`needs_rehash`]).

use argon2::{
    password_hash::{Error, PasswordHasher, PasswordVerifier},
    Algorithm, Argon2, Params, PasswordHash, Version,
};

/// Memory cost in KiB. These are the OWASP minimum for Argon2id (`m = 19456, t = 2, p = 1`) and the
/// values every existing hash already uses, so pinning them changes no stored hash. Raising them
/// toward the ADR-005 target (about 250 ms per hash) is a change to these three constants plus a
/// benchmark on production hardware; rehash-on-login moves existing hashes over.
pub const MEMORY_KIB: u32 = 19_456;
/// Number of passes.
pub const TIME_COST: u32 = 2;
/// Degree of parallelism.
pub const PARALLELISM: u32 = 1;

// Never below the OWASP minimum for Argon2id, whatever someone tunes these to.
const _: () = assert!(MEMORY_KIB >= 19_456 && TIME_COST >= 2 && PARALLELISM >= 1);

const DUMMY_PASSWORD_ATTEMPT: &str = "__unknown_user_dummy_password__";

fn argon2() -> Result<Argon2<'static>, Error> {
    let params =
        Params::new(MEMORY_KIB, TIME_COST, PARALLELISM, None).map_err(|_| Error::ParamsInvalid)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(password: &str) -> Result<String, Error> {
    Ok(argon2()?.hash_password(password.as_bytes())?.to_string())
}

/// Verifies against the parameters stored in `password_hash`, which may differ from the current
/// ones.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, Error> {
    let parsed_hash = PasswordHash::new(password_hash)?;

    match argon2()?.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(Error::PasswordInvalid) => Ok(false),
        Err(err) => Err(err),
    }
}

/// The work a login for an unknown email does in place of verifying a password. It hashes with the
/// current parameters, which costs the same as verifying a current hash.
pub fn verify_dummy() -> Result<bool, Error> {
    dummy_password_hash()?;
    Ok(false)
}

fn dummy_password_hash() -> Result<String, Error> {
    hash_password(DUMMY_PASSWORD_ATTEMPT)
}

/// True when `password_hash` was made with anything other than the current algorithm, version or
/// parameters, or can't be parsed. A successful login should then store a fresh hash.
pub fn needs_rehash(password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        return true;
    };
    if parsed.algorithm.as_str() != "argon2id" {
        return true;
    }
    let Ok(params) = Params::try_from(&parsed) else {
        return true;
    };
    parsed.version != Some(Version::V0x13.into())
        || params.m_cost() != MEMORY_KIB
        || params.t_cost() != TIME_COST
        || params.p_cost() != PARALLELISM
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_of(hash: &str) -> (u32, u32, u32) {
        let parsed = PasswordHash::new(hash).unwrap();
        let params = Params::try_from(&parsed).unwrap();
        (params.m_cost(), params.t_cost(), params.p_cost())
    }

    fn hash_with(m: u32, t: u32, p: u32, password: &str) -> String {
        let params = Params::new(m, t, p, None).unwrap();
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password(password.as_bytes())
            .unwrap()
            .to_string()
    }

    #[test]
    fn hash_and_verify_round_trip() {
        let hash = hash_password("correct horse battery staple").unwrap();

        assert!(verify_password("correct horse battery staple", &hash).unwrap());
    }

    #[test]
    fn wrong_password_returns_false() {
        let hash = hash_password("correct horse battery staple").unwrap();

        assert!(!verify_password("wrong password", &hash).unwrap());
    }

    #[test]
    fn dummy_verification_returns_false_without_panicking() {
        assert!(!verify_dummy().unwrap());
    }

    #[test]
    fn real_hashes_use_the_pinned_parameters() {
        let hash = hash_password("anything").unwrap();

        assert_eq!(
            params_of(&hash),
            (MEMORY_KIB, TIME_COST, PARALLELISM),
            "{hash}"
        );
        assert!(hash.starts_with("$argon2id$v=19$"), "{hash}");
    }

    /// The enumeration fix: a login for an unknown email must do the same work as one for a known
    /// email. This fails if the dummy hash is ever built with different parameters again.
    #[test]
    fn dummy_hash_parameters_equal_the_real_ones() {
        let real = hash_password("anything").unwrap();
        let dummy = dummy_password_hash().unwrap();

        assert_eq!(params_of(&dummy), params_of(&real));
        assert_eq!(params_of(&dummy), (MEMORY_KIB, TIME_COST, PARALLELISM));
    }

    #[test]
    fn current_hash_does_not_need_rehash() {
        assert!(!needs_rehash(&hash_password("anything").unwrap()));
    }

    #[test]
    fn hash_with_weaker_or_different_parameters_needs_rehash() {
        assert!(needs_rehash(&hash_with(8_192, 1, 1, "anything")));
        assert!(needs_rehash(&hash_with(
            MEMORY_KIB,
            TIME_COST + 1,
            PARALLELISM,
            "anything"
        )));
        assert!(needs_rehash(&hash_with(
            MEMORY_KIB * 2,
            TIME_COST,
            PARALLELISM,
            "anything"
        )));
    }

    #[test]
    fn unparseable_or_other_algorithm_hash_needs_rehash() {
        assert!(needs_rehash("not a phc string"));
        assert!(needs_rehash(""));
    }

    #[test]
    fn older_hash_still_verifies() {
        let old = hash_with(8_192, 1, 1, "old password");

        assert!(verify_password("old password", &old).unwrap());
        assert!(!verify_password("other", &old).unwrap());
    }
}
