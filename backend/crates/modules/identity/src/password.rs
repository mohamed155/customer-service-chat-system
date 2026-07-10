use argon2::{
    password_hash::{Error, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;

const DUMMY_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc";
const DUMMY_PASSWORD_ATTEMPT: &str = "__unknown_user_dummy_password__";

pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);

    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, Error> {
    let parsed_hash = PasswordHash::new(password_hash)?;

    match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(Error::Password) => Ok(false),
        Err(err) => Err(err),
    }
}

pub fn verify_dummy() -> Result<bool, Error> {
    verify_password(DUMMY_PASSWORD_ATTEMPT, DUMMY_PASSWORD_HASH)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
