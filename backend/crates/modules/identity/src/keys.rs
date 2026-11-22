//! The Ed25519 keys that sign and verify access tokens (ADR-005 §2).
//!
//! Keys come from the `AUTH_SIGNING_KEYS` environment variable, a JSON array in the shape of a
//! JWKS with one extra field for the private half:
//!
//! ```json
//! [
//!   {"kid": "2026-11", "private_key": "<base64 PKCS#8 DER>"},
//!   {"kid": "2026-08", "public_key": "<base64url raw 32-byte public key>"}
//! ]
//! ```
//!
//! The first entry with a `private_key` signs. Every entry verifies, so a previous key stays valid
//! until it is removed from the list (rotation: add the new key first, move it to the front once
//! every verifier has it, drop the old one after the longest token has expired). A verify-only
//! deployment, such as the gateway, lists public keys only.
//!
//! Development and test runs with no configuration use a key generated at process start: sessions
//! don't survive a restart, which is fine there. Production and staging never do: with no usable
//! key nothing is signed or verified.

use std::{collections::HashMap, sync::OnceLock};

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use config::Environment;
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::Deserialize;

/// The environment variable holding the key list.
pub const KEYS_ENV: &str = "AUTH_SIGNING_KEYS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(pub String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeyError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKey {
    kid: String,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    public_key: Option<String>,
}

/// The keys a process signs and verifies with.
pub struct KeySet {
    signing: Option<(String, EncodingKey)>,
    verifying: HashMap<String, DecodingKey>,
    /// `(kid, base64url public key)` of every key, in configuration order.
    public: Vec<(String, String)>,
}

impl KeySet {
    /// No keys: nothing is signed and every token is refused.
    pub fn empty() -> Self {
        Self {
            signing: None,
            verifying: HashMap::new(),
            public: Vec::new(),
        }
    }

    /// Parses the JSON key list (see the module documentation).
    pub fn from_json(json: &str) -> Result<Self, KeyError> {
        let raw: Vec<RawKey> = serde_json::from_str(json)
            .map_err(|e| KeyError(format!("{KEYS_ENV} is not a valid key list: {e}")))?;
        if raw.is_empty() {
            return Err(KeyError(format!("{KEYS_ENV} lists no keys")));
        }
        let mut set = Self::empty();
        for key in raw {
            if key.kid.trim().is_empty() {
                return Err(KeyError("a signing key has an empty kid".into()));
            }
            if set.verifying.contains_key(&key.kid) {
                return Err(KeyError(format!("key id {} appears twice", key.kid)));
            }
            match (key.private_key, key.public_key) {
                (Some(private), None) => {
                    let der = STANDARD.decode(private.trim()).map_err(|_| {
                        KeyError(format!("key {}: private_key is not base64", key.kid))
                    })?;
                    let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&der).map_err(|_| {
                        KeyError(format!(
                            "key {}: private_key is not an Ed25519 PKCS#8 key",
                            key.kid
                        ))
                    })?;
                    let public = URL_SAFE_NO_PAD.encode(pair.public_key().as_ref());
                    if set.signing.is_none() {
                        set.signing = Some((key.kid.clone(), EncodingKey::from_ed_der(&der)));
                    }
                    set.add_public(key.kid, public)?;
                }
                (None, Some(public)) => set.add_public(key.kid, public.trim().to_owned())?,
                _ => {
                    return Err(KeyError(format!(
                        "key {}: give either private_key or public_key",
                        key.kid
                    )))
                }
            }
        }
        Ok(set)
    }

    fn add_public(&mut self, kid: String, public: String) -> Result<(), KeyError> {
        let decoded = URL_SAFE_NO_PAD
            .decode(&public)
            .map_err(|_| KeyError(format!("key {kid}: public_key is not base64url")))?;
        if decoded.len() != 32 {
            return Err(KeyError(format!(
                "key {kid}: public_key must be a 32-byte Ed25519 key"
            )));
        }
        let decoding = DecodingKey::from_ed_components(&public)
            .map_err(|_| KeyError(format!("key {kid}: public_key is not usable")))?;
        self.verifying.insert(kid.clone(), decoding);
        self.public.push((kid, public));
        Ok(())
    }

    /// A fresh key, for development and tests. Never used in production or staging.
    pub fn generate(kid: &str) -> Result<Self, KeyError> {
        Self::from_json(&generated_key_json(kid)?)
    }

    /// The key that signs new tokens and its `kid`.
    pub fn signing(&self) -> Option<(&str, &EncodingKey)> {
        self.signing.as_ref().map(|(kid, key)| (kid.as_str(), key))
    }

    /// The key a token's `kid` names.
    pub fn verifying(&self, kid: &str) -> Option<&DecodingKey> {
        self.verifying.get(kid)
    }

    /// How many keys verify.
    pub fn len(&self) -> usize {
        self.verifying.len()
    }

    pub fn is_empty(&self) -> bool {
        self.verifying.is_empty()
    }

    /// The public keys as a JWKS document (RFC 7517), for verifiers in other processes.
    pub fn jwks(&self) -> serde_json::Value {
        let keys: Vec<_> = self
            .public
            .iter()
            .map(|(kid, x)| {
                serde_json::json!({
                    "kty": "OKP", "crv": "Ed25519", "alg": "EdDSA", "use": "sig",
                    "kid": kid, "x": x
                })
            })
            .collect();
        serde_json::json!({ "keys": keys })
    }
}

/// The JSON of a newly generated signing key, in the format `AUTH_SIGNING_KEYS` takes. Used by
/// tests and by `cargo run -p server -- keygen`.
pub fn generated_key_json(kid: &str) -> Result<String, KeyError> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| KeyError("the system random source failed".into()))?;
    Ok(
        serde_json::json!([{ "kid": kid, "private_key": STANDARD.encode(pkcs8.as_ref()) }])
            .to_string(),
    )
}

static CONFIGURED: OnceLock<Result<KeySet, KeyError>> = OnceLock::new();
static EPHEMERAL: OnceLock<KeySet> = OnceLock::new();

fn configured() -> &'static Result<KeySet, KeyError> {
    CONFIGURED.get_or_init(|| match std::env::var(KEYS_ENV) {
        Ok(json) if !json.trim().is_empty() => KeySet::from_json(&json),
        _ => Ok(KeySet::empty()),
    })
}

/// The keys this process uses in `environment`.
///
/// The configured list when there is one. With none: a key generated once per process in
/// development and test, and no key at all in production and staging (see [`validate_startup`]).
pub fn keys_for(environment: &Environment) -> &'static KeySet {
    static NONE: OnceLock<KeySet> = OnceLock::new();
    match configured() {
        Ok(set) if !set.is_empty() => set,
        Ok(_) => match *environment {
            Environment::Development | Environment::Test => EPHEMERAL.get_or_init(|| {
                KeySet::generate("dev-ephemeral").unwrap_or_else(|error| {
                    tracing::error!(%error, "could not generate a development signing key");
                    KeySet::empty()
                })
            }),
            Environment::Production | Environment::Staging => NONE.get_or_init(KeySet::empty),
        },
        Err(error) => {
            tracing::error!(%error, "the signing key configuration is invalid; no token is signed or verified");
            NONE.get_or_init(KeySet::empty)
        }
    }
}

/// Checks at startup that the process can do what its run mode needs: a process that issues
/// sessions (`needs_signing`) must have a signing key in production and staging; a verify-only
/// process needs at least a public key. A broken key list always stops the process.
pub fn validate_startup(environment: &Environment, needs_signing: bool) -> Result<(), KeyError> {
    let set = match configured() {
        Ok(set) => set,
        Err(error) => return Err(error.clone()),
    };
    if matches!(*environment, Environment::Production | Environment::Staging) {
        if set.is_empty() {
            return Err(KeyError(format!(
                "{KEYS_ENV} is required in production and staging"
            )));
        }
        if needs_signing && set.signing().is_none() {
            return Err(KeyError(format!(
                "{KEYS_ENV} needs a key with a private_key to sign sessions"
            )));
        }
        if set.len() < 2 {
            tracing::warn!(
                "only one signing key is configured; add the next key before rotating so no session ends"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_key_signs_and_publishes_its_public_half() {
        let set = KeySet::generate("k1").unwrap();
        let (kid, _) = set.signing().expect("signs");
        assert_eq!(kid, "k1");
        assert!(set.verifying("k1").is_some());
        let jwks = set.jwks();
        assert_eq!(jwks["keys"][0]["kid"], "k1");
        assert_eq!(jwks["keys"][0]["kty"], "OKP");
        assert_eq!(jwks["keys"][0]["crv"], "Ed25519");
        assert!(
            jwks["keys"][0].get("d").is_none(),
            "the private half is never published"
        );
    }

    #[test]
    fn the_first_private_key_signs_and_every_key_verifies() {
        let current = KeySet::generate("new").unwrap();
        let previous = KeySet::generate("old").unwrap();
        let json = serde_json::json!([
            { "kid": "new", "private_key": private_of(&generated_key_json("new").unwrap()) },
            { "kid": "old", "public_key": previous.public[0].1 },
        ])
        .to_string();
        let set = KeySet::from_json(&json).unwrap();
        assert_eq!(set.signing().unwrap().0, "new");
        assert_eq!(set.len(), 2);
        assert!(set.verifying("old").is_some());
        assert!(set.verifying("missing").is_none());
        drop(current);
    }

    fn private_of(json: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        v[0]["private_key"].as_str().unwrap().to_owned()
    }

    #[test]
    fn bad_key_lists_are_refused() {
        assert!(KeySet::from_json("not json").is_err());
        assert!(KeySet::from_json("[]").is_err());
        assert!(KeySet::from_json(r#"[{"kid":"a"}]"#).is_err());
        assert!(KeySet::from_json(r#"[{"kid":"a","private_key":"!!"}]"#).is_err());
        assert!(KeySet::from_json(r#"[{"kid":"a","public_key":"AAAA"}]"#).is_err());
        let one = generated_key_json("dup").unwrap();
        let key = private_of(&one);
        let dup = serde_json::json!([
            {"kid": "dup", "private_key": key},
            {"kid": "dup", "private_key": key}
        ])
        .to_string();
        assert!(KeySet::from_json(&dup).is_err());
    }

    #[test]
    fn production_without_keys_stops_startup_and_dev_does_not() {
        // No AUTH_SIGNING_KEYS in the test environment.
        if std::env::var(KEYS_ENV).is_ok() {
            return;
        }
        assert!(validate_startup(&Environment::Production, true).is_err());
        assert!(validate_startup(&Environment::Staging, false).is_err());
        assert!(validate_startup(&Environment::Development, true).is_ok());
        assert!(keys_for(&Environment::Production).is_empty());
        assert!(keys_for(&Environment::Test).signing().is_some());
    }
}
