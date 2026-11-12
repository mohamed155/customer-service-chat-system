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

## Part 2: dashboard (MOH-57)

- [ ] Enrol page with QR code, confirm, recovery codes; second step on sign-in; status

## Part 3: enforcement, step-up, admin reset (MOH-34)

- [ ] Require a second factor for every account; step-up (5 minutes) for sensitive actions; admin reset with audit
