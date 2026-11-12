//! Direct (non-envelope) sealing of integration secrets: a thin layer over [`secrets::legacy`].
//!
//! New secrets are sealed by the shared [`secrets::SecretBox`] (ADR-014); this module is what
//! earlier rows and the test fixtures use. The AES code lives in the `secrets` crate only.

pub use secrets::legacy::MasterKey;

/// The keys of a running process for integration secrets: the earlier integration key (rows not
/// yet moved to envelope encryption) and the root key for everything sealed since (ADR-014).
/// Fails when none is configured.
pub fn vault_from_config(
    pool: &sqlx::PgPool,
    config: &config::AppConfig,
) -> Result<secrets::Vault, String> {
    let vault = secrets::Vault::from_config(
        pool,
        config.integration_secrets_key.as_deref(),
        config.secrets_root_key(),
        config.secrets_root_key_previous.as_deref(),
    )?;
    if vault.has_keys() {
        Ok(vault)
    } else {
        Err("Integration secrets key is not configured".to_string())
    }
}

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
) -> Result<String, String> {
    let plaintext =
        secrets::legacy::open(master, scope_aad, ciphertext, nonce).map_err(|e| e.to_string())?;
    text(&plaintext)
}

/// A secret from decrypted bytes. Fails if they are not UTF-8.
pub fn text(plaintext: &[u8]) -> Result<String, String> {
    String::from_utf8(plaintext.to_vec())
        .map_err(|_| "decrypted data is not valid UTF-8".to_string())
}

pub fn aad(tenant_id: uuid::Uuid, slug: &str, field_key: &str) -> String {
    format!("integration|{tenant_id}|{slug}|{field_key}")
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
    use uuid::Uuid;

    fn test_master_key() -> MasterKey {
        MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
    }

    #[test]
    fn test_roundtrip() {
        let master = test_master_key();
        let tenant_uuid = Uuid::new_v4();
        let scope = aad(tenant_uuid, "openai", "api_key");
        let (ciphertext, nonce) = seal(&master, &scope, "sk-abc123").unwrap();
        let opened = open(&master, &scope, &ciphertext, &nonce).unwrap();
        assert_eq!(opened, "sk-abc123");
    }

    #[test]
    fn test_aad_mismatch_fails() {
        let master = test_master_key();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let scope_a = aad(tenant_a, "openai", "api_key");
        let scope_b = aad(tenant_b, "openai", "api_key");
        let (ciphertext, nonce) = seal(&master, &scope_a, "sk-secret").unwrap();
        assert!(open(&master, &scope_b, &ciphertext, &nonce).is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let master = test_master_key();
        let tenant_uuid = Uuid::new_v4();
        let scope = aad(tenant_uuid, "openai", "api_key");
        let (mut ciphertext, nonce) = seal(&master, &scope, "sk-secret").unwrap();
        ciphertext[0] ^= 0xff;
        assert!(open(&master, &scope, &ciphertext, &nonce).is_err());
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
