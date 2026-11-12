//! The earlier scheme: AES-256-GCM directly under one static key, bound to its scope by AAD.
//!
//! Rows written before envelope encryption (MOH-33) use it, and stay readable until the
//! re-encryption job has moved them to [`SecretBox`](crate::SecretBox). It is also what the test
//! fixtures use to write such rows. Nothing writes new secrets this way in production code.

use aes_gcm::aead::Nonce as AeadNonce;
use aes_gcm::{
    aead::{Aead, Generate, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use zeroize::Zeroizing;

use crate::SecretError;

/// A 32-byte key that never prints and is wiped on drop.
pub struct MasterKey(Zeroizing<[u8; 32]>);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey([REDACTED])")
    }
}

impl MasterKey {
    /// Decode a base64 string that holds exactly 32 bytes. The error names no value.
    pub fn from_base64(s: &str) -> Result<Self, String> {
        let decoded = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| format!("base64 decode failed: {e}"))?,
        );
        Self::from_bytes(&decoded)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 32 {
            return Err("an encryption key must be base64 for exactly 32 bytes".into());
        }
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    /// A short id for this key that is safe to store and log: the first 8 bytes of a salted
    /// SHA-256 of it, in hex. It names which root key wrapped a key without revealing it.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"secrets-root-key-id|");
        hasher.update(self.0.as_slice());
        let digest = hasher.finalize();
        let mut id = String::from("local:");
        for byte in &digest[..8] {
            id.push_str(&format!("{byte:02x}"));
        }
        id
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Clone for MasterKey {
    fn clone(&self) -> Self {
        Self(Zeroizing::new(*self.0))
    }
}

/// Encrypt `plaintext` for `scope_aad`. Returns `(ciphertext, nonce)`.
pub fn seal(
    master: &MasterKey,
    scope_aad: &str,
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), SecretError> {
    let (ciphertext, nonce) = seal_bytes(master.as_bytes(), scope_aad.as_bytes(), plaintext)?;
    Ok((ciphertext, nonce))
}

/// Decrypt what [`seal`] produced. Fails if the scope, key, nonce or ciphertext differ.
pub fn open(
    master: &MasterKey,
    scope_aad: &str,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SecretError> {
    open_bytes(master.as_bytes(), scope_aad.as_bytes(), ciphertext, nonce)
}

pub(crate) fn seal_bytes(
    key: &[u8; 32],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), SecretError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SecretError::Crypto("cipher init"))?;
    let nonce = AeadNonce::<Aes256Gcm>::generate();
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| SecretError::Crypto("encryption failed"))?;
    Ok((ciphertext, nonce.to_vec()))
}

pub(crate) fn open_bytes(
    key: &[u8; 32],
    aad: &[u8],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SecretError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SecretError::Crypto("cipher init"))?;
    let nonce = Nonce::try_from(nonce).map_err(|_| SecretError::Invalid("nonce length"))?;
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| {
            SecretError::Crypto("decryption failed (changed ciphertext, other scope or wrong key)")
        })?;
    Ok(Zeroizing::new(plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> MasterKey {
        MasterKey::from_bytes(&[7u8; 32]).unwrap()
    }

    #[test]
    fn round_trip() {
        let (ct, nonce) = seal(&key(), "tenant-a|openai", b"sk-abc").unwrap();
        assert_eq!(
            &open(&key(), "tenant-a|openai", &ct, &nonce).unwrap()[..],
            b"sk-abc"
        );
    }

    #[test]
    fn another_scope_cannot_open_it() {
        let (ct, nonce) = seal(&key(), "tenant-a|openai", b"sk-abc").unwrap();
        assert!(open(&key(), "tenant-b|openai", &ct, &nonce).is_err());
    }

    #[test]
    fn a_changed_ciphertext_or_nonce_fails() {
        let (mut ct, nonce) = seal(&key(), "s", b"sk-abc").unwrap();
        ct[0] ^= 1;
        assert!(open(&key(), "s", &ct, &nonce).is_err());
        let (ct, mut nonce) = seal(&key(), "s", b"sk-abc").unwrap();
        nonce[0] ^= 1;
        assert!(open(&key(), "s", &ct, &nonce).is_err());
        assert!(open(&key(), "s", &ct, &nonce[..11]).is_err());
    }

    #[test]
    fn fingerprint_names_a_key_without_revealing_it() {
        let a = MasterKey::from_bytes(&[1; 32]).unwrap();
        let b = MasterKey::from_bytes(&[2; 32]).unwrap();
        assert_eq!(
            a.fingerprint(),
            MasterKey::from_bytes(&[1; 32]).unwrap().fingerprint()
        );
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert!(a.fingerprint().starts_with("local:") && a.fingerprint().len() == 22);
    }

    #[test]
    fn nonces_are_fresh() {
        let (_, a) = seal(&key(), "s", b"x").unwrap();
        let (_, b) = seal(&key(), "s", b"x").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn key_must_be_32_bytes_and_never_prints() {
        assert!(MasterKey::from_base64("c2hvcnQ=").is_err());
        assert!(MasterKey::from_base64("not base64!").is_err());
        assert_eq!(format!("{:?}", key()), "MasterKey([REDACTED])");
    }
}
