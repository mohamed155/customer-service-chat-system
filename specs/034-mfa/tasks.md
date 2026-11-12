# Tasks: Multi-Factor Authentication (TOTP)

## Part 1: engine and sign-in flow (MOH-56)

- [x] T001 `identity::totp` with RFC 4226 and RFC 6238 vectors
- [x] T002 `SessionClaims` `amr` and `mfa_at`; the challenge token, cookie and separation tests
- [x] T003 Migration `0077` (`mfa_factors`, `mfa_recovery_codes`, key shredding on user delete)
- [x] T004 `identity::mfa`: enrolment, confirmation, verification, replay, lockout, recovery codes, audit
- [x] T005 Routes: enrol, confirm, status, verify; `LoginOutcome` and the 202 response; `Mfa` rate-limit scope
- [x] T006 Integration tests against PostgreSQL (18)
- [x] T006a Review fixes: enrolment needs the password; sessions that skipped the second factor stop working once the user enrols
- [x] T007 ADR-005 section 6 and current state, threat model, OpenAPI document and generated types

## Part 2: enforcement, step-up, regeneration, admin reset (MOH-34)

- [x] T010 `AUTH_MFA_REQUIRED` (on by default in production and staging) and the `enforce_second_factor` layer on platform and tenant routes
- [x] T011 `require_step_up` on role changes, credential and integration and tool changes, switching tenant; `POST /auth/mfa/step-up`
- [x] T012 `POST /auth/mfa/recovery-codes` (replace, needs step-up); `required` and `sessionSecondFactor` in the status
- [x] T013 Reset: tenant owner (single-workspace, non-staff members) and platform administrator; `users.sessions_valid_after`; `auth.mfa_reset`
- [x] T014 Integration tests (9) and docs

## Part 3: dashboard (MOH-57)

- [ ] Sign-in second step, enrolment, recovery codes shown once, step-up prompt and retry, regeneration, reset buttons
