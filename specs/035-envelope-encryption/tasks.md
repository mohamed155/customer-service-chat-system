# Tasks: Envelope Encryption for Tenant Secrets

- [x] T001 `crates/shared/secrets`: `legacy`, `Kms`/`LocalKms`, `KekStore`, `SecretBox`, `Vault`, with unit tests
- [x] T002 Migration `0076`: `secret_keks`, per-secret metadata columns, platform functions, shredding triggers; `secret_keks` in `GLOBAL_TABLES`
- [x] T003 `ai-providers` and `integrations` crypto modules become wrappers over `secrets::legacy`
- [x] T004 Seal and open AI credentials (tenant and platform) through the `Vault`
- [x] T005 Seal and open tool credentials through the `Vault`; the engine passes the vault to tool execution
- [x] T006 Seal and open integration secrets and webhook tokens through the `Vault`; WhatsApp workers and webhook use it
- [x] T007 Config: `APP_SECRETS_ROOT_KEY`, `APP_SECRETS_ROOT_KEY_PREVIOUS`
- [x] T008 `server secrets reencrypt | rewrap | rotate-kek | rewrap-keks`
- [x] T009 PostgreSQL tests: job for every kind, a row that does not open, KEK rotation and re-wrap, root-key rotation, shredding, scope isolation, the credential route, earlier rows
- [x] T010 Docs: ADR-014 current state, runbook, configuration, threat model
