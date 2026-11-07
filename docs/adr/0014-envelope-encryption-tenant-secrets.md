# ADR-014: Envelope encryption for tenant secrets

- **Status:** Accepted (MOH-18)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §12.2 and §19](../architecture/system-architecture.md#122-integrations-and-secrets),
  constitution Principle II ("secrets encrypted at rest with envelope encryption"), ADR-004
  (tenant scope), ADR-005 (signing keys), ADR-006 (API keys are hashed, not encrypted)

## Context

Tenants give us secrets we must be able to use, not just verify: AI provider API keys (bring your
own key), WhatsApp and Telegram tokens, OAuth refresh tokens for integrations, HTTP tool
credentials, outbound webhook signing secrets.

Passwords and our own API keys are **hashed**, not encrypted (ADR-005, ADR-006), because we only
need to verify them. This ADR covers secrets we must decrypt.

The decision has to balance these forces:

- **Database leaks.** A backup, replica or SQL injection that exposes rows must not expose usable
  secrets.
- **Key rotation.** Keys get rotated on schedule and after incidents. Rotating must not mean
  re-encrypting every secret in one risky migration, or keeping old keys forever.
- **Tenant deletion.** When a tenant leaves, their secrets must become unrecoverable, including in
  backups (crypto-shredding).
- **Blast radius.** One key compromise should expose as little as possible.
- **Operations.** Local development and CI need to work without a cloud KMS.

## Decision

### 1. Envelope encryption

- Each secret is encrypted with its own random **data encryption key (DEK)** using
  **AES-256-GCM** with a random 96-bit nonce.
- The DEK is encrypted ("wrapped") by a **key encryption key (KEK)**. Per tenant: each tenant has a
  tenant KEK, which is wrapped by the platform **root key** held in a KMS (AWS KMS, GCP KMS, Vault
  Transit) in staging and production, or a sealed local master key in development and CI.
- **Additional authenticated data (AAD)** binds each ciphertext to its context:
  `tenant_id`, `secret kind`, `record id` and key version. A ciphertext copied to another tenant's
  row or another field fails to decrypt.
- Stored per secret: `ciphertext`, `nonce`, `wrapped_dek`, `kek_id`, `kek_version`, `algorithm`.
  Plaintext and unwrapped DEKs exist only in memory, are zeroised after use (`zeroize`), and are
  never logged, returned by the API, or sent to the model (constitution III).

### 2. One shared implementation

A single `platform::crypto::SecretBox` port (seal / open / rewrap) used by every module that
stores secrets. Modules never call AES directly. The KMS sits behind an adapter (`Kms` trait)
with `LocalKms` for development and tests.

### 3. Rotation

- **Root key / KEK rotation:** create a new version; new wraps use it; a background job
  **re-wraps DEKs** (a small operation per secret) without touching the secret ciphertexts. Old
  versions stay available for decrypt until re-wrapping finishes, then are disabled.
- **DEK rotation** (re-encrypting the secret itself) happens when the secret changes, or on
  suspected compromise of that record.
- Rotation is audited and documented in the runbook.

### 4. Crypto-shredding on tenant deletion

Deleting a tenant destroys its tenant KEK. Every DEK under it, and so every secret, becomes
unrecoverable, including copies in backups, without rewriting backups.

### 5. Access

- Only the runtime units that use a secret (`worker`, `api` for tool tests) can call decrypt; the
  `gateway` has no decrypt permission (ADR-010).
- In production, KMS access is restricted by workload identity; every decrypt of a root key is
  logged by the KMS.
- Secrets are write-only through the API: the dashboard shows a masked hint (last 4 characters)
  and "replace", never the value.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **One static application key encrypting every secret directly** | A single key compromise exposes everything; rotation means re-encrypting every row at once; no per-tenant crypto-shredding. This is the current state (see below). |
| **Database-level encryption only** (TDE, disk encryption) | Protects stolen disks, not SQL injection, replicas or logical backups, where data comes out decrypted. |
| **`pgcrypto` in SQL** | Keys travel to the database in queries and can show up in logs and `pg_stat_statements`. |
| **External secrets manager for every tenant secret** (Vault KV, AWS Secrets Manager) | Strong, but per-secret API calls and cost at multi-tenant scale, and another dependency on the hot path. KMS-wrapped DEKs in our database scale better; Vault Transit remains a valid KMS adapter. |
| **Per-secret KMS calls without DEKs** | One KMS call per encrypt/decrypt adds latency and cost, and hits KMS rate limits. |

## Consequences

**Positive**

- A database leak alone exposes no usable secrets.
- Rotation is cheap and routine (re-wrap DEKs, not data).
- Tenant deletion is provable via crypto-shredding.
- Ciphertexts can't be moved between tenants or fields (AAD).

**Negative and costs**

- More moving parts: KMS integration, key versions, a re-wrap job, a cache for unwrapped tenant
  KEKs (short TTL) to avoid a KMS call per decrypt.
- Losing the root key loses every secret; KMS key deletion protection and backup procedures are
  mandatory.
- Migration from the current scheme needs a careful, verified re-encryption.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| E1 | **Direct encryption with static keys, not envelope encryption.** Secrets are sealed with AES-256-GCM directly under one environment-provided master key: `APP_AI_KEY_ENCRYPTION_KEY` for AI provider keys and tool credentials, `APP_INTEGRATION_SECRETS_KEY` for integration secrets. There's no DEK, no tenant KEK, no key version stored with the ciphertext, no KMS, and no rotation path. | `backend/crates/ai-providers/src/crypto.rs`, `backend/crates/modules/integrations/src/crypto.rs`, `docs/configuration.md` | Introduce `SecretBox` with `LocalKms`, store key metadata per secret, migrate existing ciphertexts with a verified re-encrypt job, then add a cloud KMS adapter for staging/production. |
| E2 | **The crypto code is duplicated** in two crates with near-identical `seal` / `open` functions. | same files | Replace both with the shared `SecretBox`. |
| E3 | AAD scoping is already in place (`seal(master, scope_aad, plaintext)`), which binds ciphertexts to their scope. Keep this. | same files | Carry the AAD format into `SecretBox`. |
| E4 | No crypto-shredding on tenant deletion; deleting a tenant leaves secrets decryptable from backups as long as the master key exists. | — | Comes with tenant KEKs (E1). |
