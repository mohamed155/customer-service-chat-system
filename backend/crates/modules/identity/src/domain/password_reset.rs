use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::SysRng, TryRng};
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

pub(crate) fn new_token() -> Result<String, <SysRng as TryRng>::Error> {
    let mut bytes = [0; TOKEN_BYTES];
    SysRng.try_fill_bytes(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn token_hash(token: &str) -> Option<String> {
    // Check canonical encoding before touching the database. The same public error covers misses.
    let mut bytes = [0; TOKEN_BYTES];
    if token.len() != 43 || URL_SAFE_NO_PAD.decode_slice(token, &mut bytes).ok()? != TOKEN_BYTES {
        return None;
    }
    Some(sha256(token))
}

pub(crate) fn sha256(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn normalize_email(email: &str) -> Option<String> {
    let email = email.trim().to_lowercase();
    if email.len() > 254 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    if local.is_empty()
        || domain.contains('@')
        || domain.split('.').count() < 2
        || domain.split('.').any(str::is_empty)
    {
        return None;
    }
    Some(email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_tokens_are_canonical_256_bit_secrets_and_are_hashed() {
        let first = new_token().unwrap();
        let second = new_token().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len(), 43);
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), 32);
        let hash = token_hash(&first).unwrap();
        assert_eq!(hash, hex::encode(Sha256::digest(first.as_bytes())));
        assert_ne!(hash, token_hash(&second).unwrap());
        assert!(token_hash(&format!("{first}=")).is_none());
        assert!(token_hash(&"x".repeat(43)).is_none()); // Nonzero pad bits.
    }

    #[test]
    fn email_is_normalized_and_its_shape_is_checked() {
        assert_eq!(
            normalize_email("  User+Reset@EXAMPLE.com ").as_deref(),
            Some("user+reset@example.com")
        );
        for invalid in [
            "",
            "@example.com",
            "a@b",
            "a@b..net",
            "a b@c.com",
            "a@b@c.com",
        ] {
            assert!(normalize_email(invalid).is_none(), "{invalid}");
        }
    }
}
