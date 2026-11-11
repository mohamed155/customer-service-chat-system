# Implementation Plan: Envelope Encryption for Tenant Secrets

**Spec**: [spec.md](spec.md) | **ADR**: [ADR-014](../../docs/adr/0014-envelope-encryption-tenant-secrets.md)

## Design

```text
secret --AES-256-GCM(DEK, aad)--> ciphertext + nonce
DEK    --AES-256-GCM(KEK, "dek|kek_id|version|aad")--> wrapped_dek
KEK    --Kms.wrap(root key, "kek|kek_id|version")--> secret_keks.wrapped_kek
```

- `secrets::legacy`: the earlier direct scheme, the single implementation behind the `crypto`
  modules of `ai-providers` and `integrations` (both are now thin wrappers).
- `secrets::Kms` + `LocalKms`: the root key held in memory, with an optional previous key that only
  unwraps (root-key rotation). Key ids are a salted fingerprint of the key, so nothing is versioned
  by hand. A cloud KMS implements the same trait and keeps the root key inside the service.
- `secrets::KekStore`: `PgKekStore` over `secret_keks` and `InMemoryKekStore` for tests. One active
  version per KEK id (partial unique index); rotation retires and activates in one transaction.
- `secrets::SecretBox`: `seal`, `open`, `rewrap`, `rotate_kek`, `rewrap_keks`, `destroy_kek`, with a
  five-minute cache of unwrapped KEKs (a destroyed KEK is gone everywhere within that time).
- `secrets::Vault`: a `SecretBox` plus the earlier key; `open` picks the right path from the row's
  metadata, `reencrypt` verifies before it returns, `rewrap` moves a DEK to the active KEK.
- Migration `0076`: `secret_keks`, four metadata columns on `ai_credentials`, `integration_secrets`,
  `integration_connections` (webhook token) and `tenant_tools` (credential), the changed platform
  credential functions, and the triggers that destroy a tenant's KEK on soft or hard delete.
- `server::secret_maintenance` and `server secrets <action>`: the job and the rotations.

## Root key

`APP_SECRETS_ROOT_KEY`, falling back to `APP_AI_KEY_ENCRYPTION_KEY`, then
`APP_INTEGRATION_SECRETS_KEY`. A `gateway` process holds only the integration key, so a split
deployment must give every process the same `APP_SECRETS_ROOT_KEY`. The earlier keys stay as the
read path for rows the job has not moved.

## Constitution Check

- II (secrets encrypted with envelope encryption): this feature.
- VI (tests): unit tests for the crate, PostgreSQL tests for the job, rotation and shredding.
- Module boundaries: modules call the `secrets` crate; none calls AES directly.
- No placeholders: the cloud KMS adapter is a stated non-goal, not a stub.
