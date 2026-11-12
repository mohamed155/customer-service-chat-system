# Feature Specification: Multi-Factor Authentication (TOTP)

**Feature Branch**: `mohamedahmedramadan154/moh-56-mfa-backend-totp-enrolment`

**Created**: 2026-11-11

**Status**: In progress (parts 1 and 2 of 3 are the backend; part 3 is the dashboard)

**Input**: MOH-34, ADR-005 phase M1, threat model risk 4 (row 4.11 E).

## Context: Current Baseline

There is no second factor. A session is an 8-hour HS256 JWT in the `app_session` cookie. Platform
(operator) accounts can act across tenants, so one stolen password reaches every tenant.

## Decisions (product owner, 2026-11-11)

- Recovery codes plus an admin reset, not email recovery.
- MFA is **mandatory for everyone**: platform users and tenant users (ADR-005 had tenant users
  optional).
- The step-up window is **five minutes** (ADR-005 had ten).

## Parts

| Part | Delivers | Issue |
| -- | -- | -- |
| 1 | The TOTP engine, enrolment, the two-step sign-in, recovery codes, lockout, audit | MOH-56 |
| 2 | Enforcement for everyone, step-up for sensitive actions, recovery-code regeneration, admin reset | MOH-34 |
| 3 | The dashboard: enrol, sign in with a code, step-up prompt, recovery codes, reset buttons | MOH-57 |

Enforcement is behind `AUTH_MFA_REQUIRED` (on by default in production and staging, off in
development and tests). It must not be turned on in a deployed environment before part 3, or nobody
could sign in; no environment is deployed yet.

## Requirements (part 1)

- **FR-001** A signed-in user can enrol a TOTP factor (RFC 6238, HMAC-SHA-1, 6 digits, 30-second
  steps, one step either side accepted) and confirm it with a code. The secret is returned once,
  when enrolment starts, as base32 and an `otpauth://` URI, and never again.
- **FR-002** The secret is sealed with envelope encryption (ADR-014) under a key of its own
  (`user:<id>`); deleting the user destroys the key.
- **FR-003** Confirming returns ten single-use recovery codes, once. They are stored only as
  Argon2id hashes.
- **FR-004** Sign-in for a user with a confirmed factor is two steps. The password step returns
  `202` with a short-lived (5 minutes) `app_mfa_pending` challenge cookie and **no session**. The
  second step (`POST /auth/mfa/verify`) takes a code or a recovery code and issues the session,
  recording the second factor in the token (`amr`, `mfa_at`).
- **FR-005** A challenge token is never a session and a session is never a challenge: the challenge
  carries an audience the session check does not accept, and has its own cookie name and path.
- **FR-006** A code is accepted for a time step later than the last accepted one, so it works once,
  also under concurrent requests (one conditional `UPDATE`). A recovery code is burned the same way.
- **FR-007** Attempts are limited per client address, and wrong codes lock the account's second
  factor after five in a row, for fifteen minutes. A right code resets the count. A right code that was
  already used (a replay) is refused and audited (`reason: replayed_code`) but does not count, so a
  code someone saw cannot be used to lock the owner out.
- **FR-009** Starting enrolment needs the current password and is rate limited per client, so a
  stolen session alone cannot add an attacker's authenticator.
- **FR-010** Once a user has a confirmed factor, a session that skipped the second factor stops
  working (checked on every request), so a session stolen before enrolment does not survive it. The
  session that confirmed is replaced by one that records the factor.
- **FR-008** Challenge issued, enrolment, success, failure, recovery-code use and lockout are
  audited (`auth.mfa_*`), with no secret, code or hash in the row.

## Requirements (part 2)

- **FR-011** With `AUTH_MFA_REQUIRED` on, a session without a second factor is refused on every
  platform and tenant route with `403` and the code `mfa_enrolment_required`. The profile, the MFA
  routes and sign-out stay open so the user can set one up.
- **FR-012** Sensitive actions need a second factor proven in the last five minutes, else `401`
  with the code `mfa_step_up_required`: member role changes, AI credential changes (tenant and
  platform), integration connect, update and disconnect, tool create, update and delete, switching
  into a tenant, replacing recovery codes, and resetting someone's factor. `POST /auth/mfa/step-up`
  proves a code and replaces the session with a fresh one.
- **FR-013** A signed-in user can replace their recovery codes (needs step-up); the old ones stop
  working.
- **FR-014** A tenant owner (`owner.assign`) can reset a member's factor only if that member
  belongs to no other workspace and is not platform staff; a platform administrator
  (`platform.admin`) can reset anyone. Nobody resets their own. A reset removes the factor and the
  recovery codes, ends every session the user has (`users.sessions_valid_after`), and writes
  `auth.mfa_reset` naming who did it.

## Non-goals (part 1)

Enforcement, step-up and admin reset (part 2); the dashboard (part 3); WebAuthn (ADR-005 M2).

## Tenancy and security impact

Users have no tenant, so the tables are global. Every route is self-service: it acts only on the
signed-in user (or on the user named by a valid challenge). Wrong codes, used codes and users
without a factor all answer the same way. A sign-in that fails after the password step never
reveals whether the code, the recovery code or the account was the problem.

## Acceptance

See `tasks.md`. Tests: `crates/modules/identity` (TOTP and session unit tests, including the RFC
4226 and RFC 6238 vectors) and `crates/server/tests/mfa.rs` (20 tests) and `crates/server/tests/mfa_policy.rs` (9 tests) against PostgreSQL.

## Known gaps (recorded, not done in part 1)

- No email to the user when a factor is added or reset. A confirmed enrolment already needs the
  password and a code; a notification is a follow-up with the reset flow (MOH-34).
- The per-client limit is per minute window like login; there is no daily cap on repeated lockouts.
