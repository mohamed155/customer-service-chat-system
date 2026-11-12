//! Envelope encryption for tenant secrets (ADR-014, MOH-33).
//!
//! One implementation of AES-256-GCM sealing for every module that stores a secret it has to
//! read back (AI provider keys, tool credentials, integration tokens):
//!
//! - [`SecretBox`] seals each secret under its own random data key (DEK). The DEK is wrapped by a
//!   key-encryption key (KEK) per tenant (`tenant:<uuid>`) or for the platform (`platform`), and
//!   each KEK is wrapped by the root key held behind the [`Kms`] port ([`LocalKms`] for
//!   development, CI and single-host installs; a cloud KMS adapter implements the same trait).
//! - Rotating a KEK or the root key re-wraps keys, never the secrets ([`SecretBox::rewrap`]).
//! - Destroying a tenant's KEK makes every secret under it unrecoverable
//!   ([`SecretBox::destroy_kek`], crypto-shredding).
//! - [`legacy`] is the earlier scheme (one static key, no DEK) that existing rows still use until
//!   the re-encryption job has moved them. It is the single implementation behind the
//!   `crypto` modules of `ai` and `integrations`.
//!
//! Plaintext and unwrapped keys live in `Zeroizing` buffers and are never logged.

mod error;
pub mod kms;
pub mod legacy;
mod secretbox;
pub mod store;
pub mod vault;

pub use error::SecretError;
pub use kms::{Kms, LocalKms, WrappedKey};
pub use secretbox::{Sealed, SecretBox, ALGORITHM};
pub use store::{InMemoryKekStore, KekRecord, KekState, KekStore, PgKekStore};
pub use vault::{kek_id, Envelope, StoredSecret, Vault};
