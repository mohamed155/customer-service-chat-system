# Implementation Plan: Multi-Factor Authentication (TOTP), part 1

**Spec**: [spec.md](spec.md) | **ADR**: [ADR-005 section 6](../../docs/adr/0005-authentication-tokens.md)

## Design

- `identity::totp`: HOTP/TOTP, base32, `otpauth://`, constant-time compare. Pure; time is a
  parameter, so the RFC vectors run as published.
- `identity::session`: `SessionClaims` gains `amr` and `mfa_at` (both `#[serde(default)]`, so tokens
  issued before keep validating and read as "no second factor"). `PendingClaims` is the challenge:
  HS256 with `aud = "mfa_pending"`, cookie `app_mfa_pending`, path `/api/v1/auth/mfa`, 5 minutes.
- `identity::mfa`: the service. Enrolment seals the secret with `secrets::SecretBox` under
  `user:<id>` and AAD `mfa-totp|<id>`. Replay protection and recovery-code burning are single
  conditional `UPDATE`s. Wrong attempts are counted in one statement that also sets the lock.
- Routes: `POST /auth/mfa/enrol`, `POST /auth/mfa/enrol/confirm`, `GET /auth/mfa/status`
  (identity, session required) and `POST /auth/mfa/verify` (server, challenge required, rate
  limited by the new `Mfa` scope).
- `authenticate_login` returns `LoginOutcome`: a session, or `MfaRequired` with the challenge
  cookie. The login route answers 202 for the latter.
- Migration `0077`: `mfa_factors`, `mfa_recovery_codes`, and triggers that destroy a user's key when
  the user is deleted. Both tables are global (`GLOBAL_TABLES`).

## Constitution Check

- II (secrets encrypted): the TOTP secret is sealed with envelope encryption.
- VI (tests): vectors, replay (sequential and concurrent), lockout, recovery-code reuse, expired and
  foreign challenges, the challenge/session separation, rate limit, audit, key shredding.
- Errors: Problem Details; wrong codes never say why.
