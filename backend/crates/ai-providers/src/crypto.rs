//! Direct (non-envelope) sealing of provider keys: a thin layer over [`secrets::legacy`].
//!
//! New secrets are sealed by the shared [`secrets::SecretBox`] (ADR-014); this module is what
//! earlier rows and the test fixtures use. The AES code lives in the `secrets` crate only.

use crate::SecretKey;

pub use secrets::legacy::MasterKey;

pub fn seal(
    master: &MasterKey,
    scope_aad: &str,
    plaintext: &str,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    secrets::legacy::seal(master, scope_aad, plaintext.as_bytes()).map_err(|e| e.to_string())
}

pub fn open(
    master: &MasterKey,
    scope_aad: &str,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<SecretKey, String> {
    let plaintext =
        secrets::legacy::open(master, scope_aad, ciphertext, nonce).map_err(|e| e.to_string())?;
    secret_key(&plaintext)
}

/// A provider key from decrypted bytes. Fails if they are not UTF-8.
pub fn secret_key(plaintext: &[u8]) -> Result<SecretKey, String> {
    String::from_utf8(plaintext.to_vec())
        .map(SecretKey::new)
        .map_err(|_| "decrypted data is not valid UTF-8".to_string())
}

pub fn aad(tenant_id: Option<uuid::Uuid>, provider: &str) -> String {
    match tenant_id {
        Some(id) => format!("{id}|{provider}"),
        None => format!("platform|{provider}"),
    }
}

pub fn hint(plaintext: &str) -> String {
    if plaintext.len() >= 4 {
        plaintext[plaintext.len() - 4..].to_string()
    } else {
        plaintext.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_master_key() -> MasterKey {
        MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
    }

    #[test]
    fn test_roundtrip() {
        let master = test_master_key();
        let (ciphertext, nonce) = seal(&master, "tenant-uuid|openai", "sk-abc123").unwrap();
        let opened = open(&master, "tenant-uuid|openai", &ciphertext, &nonce).unwrap();
        assert_eq!(opened.expose(), "sk-abc123");
    }

    #[test]
    fn test_aad_mismatch_fails() {
        let master = test_master_key();
        let (ciphertext, nonce) = seal(&master, "tenant-a|openai", "sk-secret").unwrap();
        assert!(open(&master, "tenant-b|openai", &ciphertext, &nonce).is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let master = test_master_key();
        let (mut ciphertext, nonce) = seal(&master, "test|openai", "sk-secret").unwrap();
        ciphertext[0] ^= 0xff;
        assert!(open(&master, "test|openai", &ciphertext, &nonce).is_err());
    }

    #[test]
    fn test_hint_full() {
        assert_eq!(hint("sk-abc123XYZ"), "3XYZ");
    }

    #[test]
    fn test_hint_short() {
        assert_eq!(hint("ab"), "ab");
    }

    #[test]
    fn test_hint_exact_4() {
        assert_eq!(hint("1234"), "1234");
    }
}
