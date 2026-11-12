# Feature Specification: Envelope Encryption for Tenant Secrets

**Feature Branch**: `mohamedahmedramadan154/moh-33-envelope-encryption-for-tenant`

**Created**: 2026-11-11

**Status**: Implemented

**Input**: MOH-33, ADR-014 gaps E1-E4, threat model risk 3.

## Context: Current Baseline

Tenant secrets the platform must read back (AI provider keys, tool credentials, integration
tokens) were sealed with AES-256-GCM directly under one static key per kind
(`APP_AI_KEY_ENCRYPTION_KEY`, `APP_INTEGRATION_SECRETS_KEY`), by two near-identical crypto modules.
One leaked key exposed every tenant's secrets, a key could not be rotated without re-encrypting
everything at once, and deleting a tenant left its secrets readable from backups.

## Scope

- One shared implementation (`crates/shared/secrets`): `SecretBox` (seal, open, rewrap), a `Kms`
  port with `LocalKms`, a KEK store, and a `Vault` that opens both the new and the earlier rows.
- Per-secret data key (DEK), wrapped by a per-tenant (or platform) key (KEK), wrapped by the root
  key. Stored beside each secret: wrapped DEK, `kek_id`, `kek_version`, algorithm.
- A verified re-encryption job for existing rows; KEK and root-key rotation by re-wrapping.
- Crypto-shredding when a tenant is deleted.
- The existing AAD scoping of every secret kind is kept.

## Non-goals

- A cloud KMS adapter. The `Kms` port is the seam; AWS KMS, GCP KMS or Vault Transit is one more
  implementation of it. Decided by the product owner on 2026-11-11: ship the port with `LocalKms`
  first, without cloud lock-in.
- Encrypting data that is hashed today (passwords, API keys, ADR-005 and ADR-006).
- Removing the earlier single-key read path. It stays until the job has reported nothing left on
  every environment, then a later change drops it.

## Requirements

- **FR-001** Every new secret is sealed under its own random DEK. The DEK is wrapped by the KEK of
  its tenant (`tenant:<uuid>`) or the platform (`platform`).
- **FR-002** `kek_id`, `kek_version`, algorithm and the wrapped DEK are stored with the secret.
- **FR-003** Open fails for any scope other than the one a secret was sealed for (tenant, kind,
  field), after any change to the stored parts, and after the KEK is destroyed.
- **FR-004** Rows sealed the earlier way keep opening. A job (`server secrets reencrypt`) moves
  them, checks that the new seal opens to the same plaintext before storing it, and stores it
  only while the row is unchanged. It can be stopped and run again.
- **FR-005** Rotating a KEK (`server secrets rotate-kek`) and then re-wrapping (`server secrets
  rewrap`) changes no secret ciphertext. Rotating the root key re-wraps KEKs (`server secrets
  rewrap-keks`), not secrets.
- **FR-006** Deleting a tenant (soft or hard) destroys its KEKs in the same transaction. Restoring
  the tenant does not restore the keys.
- **FR-007** Plaintext and unwrapped keys live in zeroising buffers and are never logged or
  returned by the API.

## Tenancy and security impact

`secret_keks` holds only wrapped keys and has no tenant column; it is a global table
(`GLOBAL_TABLES`) the runtime role can read, insert and update but not delete. A KEK read for the
wrong tenant still cannot open a secret, because the AAD binds the tenant. Platform credentials
are reached only through the audited `xtenant` functions, which write an audit row for every
change.

## Acceptance

See `tasks.md`. Tests: `crates/shared/secrets` (23 unit tests) and
`crates/server/tests/envelope_encryption.rs` (8 tests against PostgreSQL).
