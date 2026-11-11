//! How a tenant tool's credential is stored.
//!
//! The `credential_ciphertext` column keeps `base64(ciphertext)||base64(nonce)` (the format since
//! spec 022); the four `credential_*` columns beside it hold the envelope-encryption metadata
//! (MOH-33) and are all `NULL` for a credential sealed before that, which is still readable.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use secrets::StoredSecret;

/// The text for the `credential_ciphertext` column.
pub fn encode(stored: &StoredSecret) -> String {
    format!(
        "{}||{}",
        STANDARD.encode(&stored.ciphertext),
        STANDARD.encode(&stored.nonce)
    )
}

/// Rebuild a stored credential from the column text and the four metadata columns.
pub fn decode(
    text: &str,
    wrapped_dek: Option<Vec<u8>>,
    kek_id: Option<String>,
    kek_version: Option<i32>,
    algorithm: Option<String>,
) -> Result<StoredSecret, String> {
    let (ciphertext, nonce) = text
        .split_once("||")
        .ok_or_else(|| "invalid credential storage format".to_string())?;
    let ciphertext = STANDARD
        .decode(ciphertext)
        .map_err(|_| "failed to decode credential ciphertext".to_string())?;
    let nonce = STANDARD
        .decode(nonce)
        .map_err(|_| "failed to decode credential nonce".to_string())?;
    StoredSecret::from_columns(
        ciphertext,
        nonce,
        wrapped_dek,
        kek_id,
        kek_version,
        algorithm,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_an_earlier_credential() {
        let stored =
            StoredSecret::from_columns(vec![1, 2, 3], vec![9; 12], None, None, None, None).unwrap();
        let text = encode(&stored);
        assert_eq!(decode(&text, None, None, None, None).unwrap(), stored);
    }

    #[test]
    fn round_trips_an_envelope_credential() {
        let stored = StoredSecret::from_columns(
            vec![1, 2, 3],
            vec![9; 12],
            Some(vec![7; 60]),
            Some("tenant:x".into()),
            Some(2),
            Some("aes-256-gcm".into()),
        )
        .unwrap();
        let back = decode(
            &encode(&stored),
            stored.wrapped_dek().map(<[u8]>::to_vec),
            stored.kek_id().map(str::to_owned),
            stored.kek_version(),
            stored.algorithm().map(str::to_owned),
        )
        .unwrap();
        assert_eq!(back, stored);
    }

    #[test]
    fn malformed_text_and_partial_metadata_are_refused() {
        assert!(decode("no-separator", None, None, None, None).is_err());
        assert!(decode("!!||!!", None, None, None, None).is_err());
        assert!(decode("AQID||CQkJCQkJCQkJCQkJ", Some(vec![1]), None, None, None).is_err());
    }
}
