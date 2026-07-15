use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use ai_providers::SecretKey;
use base64::Engine;

pub struct MasterKey([u8; 32]);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey([REDACTED])")
    }
}

impl MasterKey {
    pub fn from_base64(s: &str) -> Result<Self, String> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| format!("base64 decode failed: {e}"))?;
        if decoded.len() != 32 {
            return Err(
                "APP_AI_KEY_ENCRYPTION_KEY must be a base64-encoded string of exactly 32 bytes"
                    .into(),
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        Ok(Self(key))
    }
}

pub fn seal(
    master: &MasterKey,
    scope_aad: &str,
    plaintext: &str,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher =
        Aes256Gcm::new_from_slice(&master.0).map_err(|e| format!("AES-256-GCM init: {e}"))?;
    let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
    let payload = Payload {
        msg: plaintext.as_bytes(),
        aad: scope_aad.as_bytes(),
    };
    let ciphertext = cipher
        .encrypt(&nonce, payload)
        .map_err(|e| format!("encryption failed: {e}"))?;
    Ok((ciphertext, nonce.to_vec()))
}

pub fn open(
    master: &MasterKey,
    scope_aad: &str,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<SecretKey, String> {
    let cipher =
        Aes256Gcm::new_from_slice(&master.0).map_err(|e| format!("AES-256-GCM init: {e}"))?;
    if nonce.len() != 12 {
        return Err(format!(
            "invalid nonce length: expected 12, got {}",
            nonce.len()
        ));
    }
    let nonce = Nonce::from_slice(nonce);
    let payload = Payload {
        msg: ciphertext,
        aad: scope_aad.as_bytes(),
    };
    let plaintext = cipher
        .decrypt(nonce, payload)
        .map_err(|_| "decryption failed (tampered ciphertext or wrong key)".to_string())?;
    let key_str = String::from_utf8(plaintext)
        .map_err(|_| "decrypted data is not valid UTF-8".to_string())?;
    Ok(SecretKey::new(key_str))
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
    use base64::Engine;

    fn test_master_key() -> MasterKey {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=")
            .unwrap();
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        MasterKey(key)
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
