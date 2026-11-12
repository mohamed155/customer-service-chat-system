use std::fmt;

/// A failure to seal, open or manage keys. The messages never contain key material or plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// Encryption failed, or decryption failed because the ciphertext was changed, was sealed for
    /// another scope (AAD) or the key is wrong.
    Crypto(&'static str),
    /// The key-management service refused or could not do the request.
    Kms(String),
    /// The key store (database) failed.
    Store(String),
    /// The key-encryption key was destroyed (crypto-shredding): the secret is gone.
    KekDestroyed,
    /// The key-encryption key version named by a sealed secret is not in the store.
    KekMissing,
    /// A stored value has the wrong shape (length, encoding, algorithm).
    Invalid(&'static str),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crypto(what) => write!(f, "{what}"),
            Self::Kms(what) => write!(f, "key management failed: {what}"),
            Self::Store(what) => write!(f, "key store failed: {what}"),
            Self::KekDestroyed => f.write_str("the key for this secret was destroyed"),
            Self::KekMissing => f.write_str("the key for this secret was not found"),
            Self::Invalid(what) => write!(f, "invalid stored secret: {what}"),
        }
    }
}

impl std::error::Error for SecretError {}

impl From<sqlx::Error> for SecretError {
    fn from(error: sqlx::Error) -> Self {
        Self::Store(error.to_string())
    }
}
