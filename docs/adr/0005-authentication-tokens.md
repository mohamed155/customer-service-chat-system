# ADR-005: Authentication — password hashing, tokens, cookies, CSRF, MFA

- **Status:** Accepted (MOH-16 merged to master, 2026-11-07)
- **Date:** 2026-11-06
- **Deciders:** Platform engineering
- **Related:** [System architecture §6.1](../architecture/system-architecture.md#61-authentication),
  ADR-006 (authorization model), ADR-009 (realtime transport), ADR-014 (envelope encryption)

## Context

The platform serves two populations of human users through one API:

- **Tenant users** (Owner, Admin, Manager, Agent, Viewer) working in the dashboard.
- **Platform users** (Super Admin, Developer, Support, Sales, Finance) who can see across tenants.
  A compromised platform account can reach every tenant, so its bar must be higher.

Machine callers (API keys) and end customers (widget visitors) are out of scope here. ADR-006
covers API keys, and the widget has its own session scheme (§6.3 of the architecture doc).

The decision has to balance these forces:

- **Revocation vs. statelessness.** Stateless JWTs avoid a database hit per request, but they
  can't be revoked before they expire. A long-lived stateless token is effectively unrevocable.
- **Theft resistance.** The dashboard is a browser SPA. Anything readable from JavaScript
  (`localStorage`, non-HttpOnly cookies) can be stolen by one XSS bug.
- **CSRF.** Cookie-based auth brings CSRF back as a risk, and it needs an explicit defence.
- **Key management.** A shared HMAC secret has to be present wherever tokens are verified, so
  every verifier can also mint tokens. Asymmetric keys separate signing from verification.
- **Brute force and enumeration.** Login must not reveal whether an account exists, through
  either the response body or its timing.

What is built today (feature 007-authentication, hardened in 033) is a single **8-hour HS256 JWT**
in an `app_session` cookie, with a `revoked_sessions` denylist checked on every request. That
was the right first step, but it doesn't meet the bar above. See
[Current state and migration](#current-state-and-migration).

## Decision

### 1. Password hashing: Argon2id with pinned parameters

- Hash with **Argon2id**, version 0x13, and a 16-byte random salt per hash. Store the PHC string
  (`$argon2id$v=19$m=…,t=…,p=…$salt$hash`), so the parameters travel with each hash.
- **Pin the parameters explicitly** in one place (`identity::password`). Don't rely on
  `Argon2::default()`. Baseline: `m = 65536 KiB (64 MiB), t = 3, p = 1`, then tuned so that one
  hash takes about **250 ms** on production hardware. That must never drop below the OWASP floor
  of `m = 19456, t = 2, p = 1`.
- **Rehash on login.** When a stored hash's parameters are weaker than the current ones, re-hash
  the password after a successful verification. This lets the parameters move up without forcing
  password resets.
- **Enumeration resistance.** Unknown accounts verify against a dummy hash built from **the same
  pinned parameters**, so both paths take the same time. A unit test asserts that the dummy hash
  parameters equal the configured parameters.
- **Throttling.** Keep the Redis-backed `(email, client_ip)` login throttle from 033. Add a
  per-account counter with exponential lockout after repeated failures. Every failure returns the
  same generic `401` with no indication of which factor failed.

### 2. Access token: short-lived EdDSA JWT

- **Algorithm:** **EdDSA (Ed25519)**. Verifiers accept only `alg = EdDSA`, and `none`/HS\*
  are rejected outright, which closes off algorithm-confusion attacks.
- **Lifetime:** **10 minutes** (configurable from 5 to 15). Zero leeway, as today.
- **Header:** `kid` names the signing key. Public keys are published at
  `GET /.well-known/jwks.json`.
- **Claims:**

  | Claim | Meaning |
  | -- | -- |
  | `iss`, `aud` | Issuer and audience. Both are validated. |
  | `sub` | User ID |
  | `sid` | Session ID: the refresh-token family (see 3) |
  | `jti` | Token ID |
  | `typ` | `tenant_user` or `platform_user` |
  | `amr` | Authentication methods used, e.g. `["pwd"]` or `["pwd","otp"]` |
  | `imp` | Optional impersonation-session ID (see ADR-006) |
  | `iat`, `exp` | Standard |

- **No tenant roles or permissions in the token.** Memberships and roles change. They are
  resolved per request (and cached; see ADR-006). The `X-Tenant-ID` header selects the tenant.
  The token never asserts it.
- **Key rotation:** keep two keys active, *current* (signs and verifies) and *previous* (verifies
  only). Rotate every 90 days or immediately on suspected compromise. Private keys are stored
  under envelope encryption (ADR-014) and are only loaded by the API unit.
- **Revocation:** with a 10-minute lifetime, revocation works at the **session** level. Each
  request checks the `sid` against a session-revocation set in Redis that holds an entry until
  the access token's `exp`, and the database stays the source of truth. The per-`jti` denylist
  goes away.

### 3. Refresh token: opaque, rotating, with reuse detection

- **Format:** 256 bits from a CSPRNG, base64url, with the prefix `rt_`. Only
  `SHA-256(token)` is stored. The server never stores the plaintext.
- **Model:** `auth_sessions (id = sid, user_id, created_at, last_seen_at, idle_expires_at,
  absolute_expires_at, revoked_at, revoke_reason, user_agent, ip)` and
  `refresh_tokens (id, session_id, token_hash, parent_id, issued_at, used_at, expires_at)`.
- **Rotation:** `POST /auth/refresh` consumes the presented token in one statement
  (`UPDATE … SET used_at = now() WHERE token_hash = $1 AND used_at IS NULL RETURNING …`) and
  returns a new refresh token and a new access token.
- **Reuse detection:** if the presented token is already marked used, it was replayed, either by
  an attacker or by a client that was compromised. The server **revokes the whole session
  family** (`auth_sessions.revoked_at`, reason `refresh_reuse`), writes an
  `auth.refresh_reuse_detected` audit event, and returns `401`.
- **Concurrency grace:** two browser tabs can refresh at the same moment. If the reused token was
  rotated **less than 10 seconds ago** and the request comes from the same session, the server
  doesn't revoke. It issues a fresh **access token only** and leaves the refresh cookie alone.
  Both tabs share one cookie jar, so the winning response has already set the successor refresh
  token, and the server never needs its plaintext. Outside that window, reuse is treated as
  theft.
- **Lifetimes:** 12-hour idle timeout (each rotation extends it) and a 7-day absolute cap for
  tenant users. Platform users get 1 hour idle and 12 hours absolute.
- **Logout** revokes the session family. "Sign out everywhere" and admin-forced sign-out revoke
  every session for the user. Password change and MFA reset revoke every session except the
  current one.

### 4. Browser transport: HttpOnly cookies

Tokens never touch JavaScript, `localStorage` or `sessionStorage`.

| Cookie | Content | Attributes |
| -- | -- | -- |
| `__Host-app_session` | access JWT | `HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=<access ttl>` |
| `__Secure-app_refresh` | refresh token | `HttpOnly; Secure; SameSite=Strict; Path=/auth/refresh; Max-Age=<absolute ttl>` |

- The `__Host-` prefix pins the access cookie to the exact host with no `Domain` attribute, so a
  sibling subdomain can't set or overwrite it.
- The refresh cookie is scoped to the refresh path and set to `SameSite=Strict`, so the browser
  only sends it to the one endpoint that consumes it.
- The SPA treats `401 unauthenticated` as "refresh and retry once". The frontend's credentials
  interceptor queues requests while one refresh is in flight, so a burst of concurrent `401`s
  doesn't set off a burst of rotations.
- Non-browser clients (CLI, tests) can send `Authorization: Bearer <access>`. The CSRF rules in
  §5 only apply to cookie-authenticated requests.

### 5. CSRF strategy

`SameSite=Lax` already blocks cross-site cookie sending on `POST`/`PUT`/`PATCH`/`DELETE`.
Two more layers sit on top of it, and both are required:

1. **Strict origin check.** For every unsafe method on a cookie-authenticated request, the
   `Origin` header must be **present** and must exactly match an entry in
   `CORS_ALLOWED_ORIGINS`. When `Origin` is missing, the server falls back to the `Referer` origin.
   **When both are missing, the request is rejected** with `403`.
2. **Custom-header requirement.** Unsafe cookie-authenticated requests must carry
   `X-Requested-With: helix`, or `X-Tenant-ID` on tenant routes. A plain HTML form can't set
   custom headers, and a cross-origin `fetch` that sets one triggers a CORS preflight, which our
   allowlist rejects. `x-requested-with` has to be added to the dashboard CORS layer's
   allowed headers. Without that, the dashboard's own cross-origin calls fail preflight.

The architecture doc mentions double-submit tokens. This design doesn't need them, because the
two checks above cover the same attacks without server-side token state.

**Login CSRF.** `POST /auth/login` and `POST /auth/refresh` have no authenticated principal when
they arrive, so they get the same Origin and custom-header checks applied **unconditionally**.
The checks don't depend on a principal being present.

Long-lived SSE streams (`GET /tenant/events`, ADR-009) are safe methods and need no CSRF check.
However, they **outlive the access token**. When a stream is opened, the server records the
session's `sid`, and the stream is closed as soon as that session is revoked. Otherwise the
stream can stay open up to its absolute session cap. The client reconnects after each refresh,
so that the per-request authorization runs again.

### 6. MFA plan

| Phase | Scope |
| -- | -- |
| **M1: TOTP** | RFC 6238, SHA-1, 6 digits, 30 s step, ±1 step window, with replay protection (the last accepted step is stored). The TOTP secret is stored under envelope encryption (ADR-014). Users get 10 single-use recovery codes, stored as Argon2id hashes. |
| **M1: policy** | Mandatory for **everyone**, platform and tenant users (product owner decision, 2026-11-11; this replaces "optional for tenant users"). No account keeps a session whose `amr` lacks a second factor. A user who cannot produce a code uses a recovery code; an administrator can reset the factor, which is audited. |
| **M1: step-up** | Sensitive actions (`owner.assign`, `tenant.delete`, API-key creation, impersonation start, role changes, secret changes) require `amr` to include a second factor obtained within the last **5 minutes** (decision of 2026-11-11, was 10). Otherwise the server returns `401` with code `mfa_step_up_required`. |
| **M2: WebAuthn** | Passkeys and security keys as both a second factor and a passwordless first factor. Platform users get pushed to WebAuthn first. |
| **Later: SSO** | Enterprise OIDC/SAML, where the IdP's MFA assertion (`amr`/`acr`) is trusted per tenant configuration. |

Login with MFA takes two steps. The password step returns a short-lived (5-minute) `mfa_pending`
challenge cookie and no session. Completing the factor step creates the session and issues the
tokens.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Keep a long-lived HS256 JWT plus `jti` denylist** (current) | Every verifier holds the signing secret, so there's no separation between verifying and minting. An 8-hour token is only revocable through a database check on every request, which cancels the stateless benefit. |
| **Server-side opaque sessions only** (no JWT) | Simple and instantly revocable. But it needs a session-store lookup on every request in every unit, and it gives the SSE/WebSocket and `gateway` units (ADR-010) no self-contained credential. A short JWT plus session check costs about the same and keeps that option open. |
| **RS256 / ES256** | Both are acceptable. RS256 keys and signatures are much larger. ES256 depends on per-signature randomness, and a nonce failure leaks the key. Ed25519 is deterministic, fast and small. ES256 is the fallback if a consumer can't do EdDSA. |
| **Tokens in `localStorage` + `Authorization` header** | Any XSS bug means token theft. Cookies keep the token away from script. |
| **Double-submit CSRF token** | Works, but adds per-session token plumbing to the SPA. The strict Origin check plus custom header gives equivalent protection when `SameSite` and CORS are already in place. |
| **Non-rotating refresh tokens** | A stolen refresh token stays valid until it expires, and nothing reveals the theft. Rotation with reuse detection turns replay into a signal. |
| **bcrypt / scrypt** | bcrypt silently truncates input at 72 bytes and isn't memory-hard. scrypt is acceptable, but Argon2id is the current OWASP and RFC 9106 recommendation. |
| **SMS OTP** | Exposed to SIM-swap attacks and costly. Not offered. |

## Consequences

**Positive**

- A stolen access token stays useful for at most 10 minutes. A stolen refresh token gets
  noticed the first time the real client refreshes.
- Verifiers hold only public keys. The future `gateway` unit can verify tokens without being able
  to mint them.
- Platform accounts, the highest-value targets, always need a second factor.
- Hash parameters can move up through rehash-on-login, with no forced resets.

**Negative and costs**

- Two new tables, a refresh endpoint, a JWKS endpoint, and key-rotation runbook steps.
- Ed25519 private keys need secure storage and a rotation procedure (documented in `docs/runbooks/operations.md`
  when implemented).
- The SPA must deal with refresh races, and the 10-second reuse grace window has to be tested
  carefully, since it trades a little theft detection for usability.
- SSE clients reconnect after refresh, which costs some realtime churn.
- Argon2id at 64 MiB per hash limits how many logins can run at once. Login concurrency per API
  instance has to be capped (semaphore) so a login burst can't exhaust memory.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| A1 | JWTs are signed **HS256** with the shared `AUTH_JWT_SECRET`, with no `kid`, `iss`, `aud`, `sid` or `typ` claims. | `backend/crates/modules/identity/src/session.rs` (`issue_token`, `validate_token`) | Add an EdDSA signer, JWKS and key config. Accept both algorithms for one access-TTL window during cutover, then drop HS256. |
| A2 | There's a single **8-hour** access token (`AUTH_SESSION_TTL_SECONDS`, default 28800). There's no refresh token, session family or reuse detection. Revocation uses the per-`jti` table `revoked_sessions`. | `identity/src/routes.rs` (`authenticate_login`, `logout`), `identity/src/lib.rs` (`resolve_cookie_principal`) | Add the `auth_sessions` and `refresh_tokens` migrations, `POST /auth/refresh`, and a refresh queue in the frontend credentials interceptor. Retire `revoked_sessions` after cutover. |
| A3 | ~~**Argon2 parameter mismatch.**~~ **Resolved (MOH-35).** The Argon2id parameters are defined once as constants (`MEMORY_KIB`, `TIME_COST`, `PARALLELISM`) in `identity::password` and used to build every real hash and the dummy hash for unknown emails, so both paths do the same work. A unit test fails if the dummy hash's parameters differ from a real hash's; an integration test fails if unknown-email logins are not within 0.6 to 1.6 times the time of known-email ones (before the fix the ratio was about 3.4). A successful login replaces a hash made with other parameters (`needs_rehash`), with a compare-and-set update so a concurrent password change is never overwritten. The constants are the OWASP minimum (`m = 19456, t = 2, p = 1`), which is what every existing hash already uses, so nothing changes for stored hashes. There is no password-reset flow in the codebase yet, so the reset half of the original note has nothing to check. | `backend/crates/modules/identity/src/password.rs`, `backend/crates/modules/identity/src/routes.rs` (`rehash_if_outdated`), `backend/crates/server/tests/auth.rs` | Done. Raising the parameters to the §1 baseline (`m = 65536, t = 3`, tuned to about 250 ms) is now a change to three constants plus a benchmark on production hardware. Until rehash-on-login has moved every hash, an account whose hash is weaker than the current parameters verifies faster than an unknown email, so do that change together with a one-off check of how many legacy hashes remain. |
| A4 | ~~The CSRF origin check lets requests without `Origin` through.~~ **Resolved (MOH-36).** A state-changing request (not `GET`, `HEAD`, `OPTIONS`) authenticated by the session cookie is rejected with 403 unless it names an allowed origin (`Origin`, else the origin part of `Referer`) and carries `X-Requested-With` or `X-Tenant-ID`; a missing `Origin` and `Referer`, a foreign one, a look-alike host and a missing header are all refused. `POST /auth/login` and `POST /auth/signup`, which start a session before any principal exists, get the same check unconditionally (login CSRF). `x-requested-with` is in the CORS allow-list, and the dashboard's credentialed requests send it from `authTokenInterceptor`. The widget is unaffected: it sends no cookie (its own session token in `Authorization`, `credentials` off) and its routes sit outside this check. The development-only `X-Dev-User-Id` principal carries no cookie, so it keeps the old rule (an `Origin`, when present, must be allowed). | `backend/crates/server/src/router.rs` (`csrf_origin_middleware`, `csrf_session_start_middleware`, `require_csrf_proof`, `cors_layer`), `backend/crates/server/tests/auth.rs`, `frontend/apps/dashboard/src/app/core/http/auth-token.interceptor.ts` | Done. There is no `/auth/refresh` route yet; when ADR-005 §3 adds it, put it behind `csrf_session_start_middleware`. Non-browser clients of the cookie API must now send an allowed `Origin` and a custom header. |
| A5 | The cookie is named `app_session` without the `__Host-` prefix. | `identity/src/session.rs` (`session_cookie_string`) | Rename to `__Host-app_session` at the same time as A2 (it forces one re-login). |
| A6 | There's no MFA. | `backend/crates/modules/identity/src/{totp,mfa,mfa_routes,session}.rs`, `specs/034-mfa/` | **Part 1 done (MOH-56):** TOTP enrolment and confirmation, ten single-use recovery codes (Argon2id hashes), a two-step sign-in (`202` plus a 5-minute `app_mfa_pending` challenge, then `POST /auth/mfa/verify`), replay protection, per-client rate limit, lockout after five wrong codes for fifteen minutes, `auth.mfa_*` audit rows, the secret sealed under ADR-014. The session records `amr` and `mfa_at`. Starting enrolment needs the current password, and a session that skipped the second factor stops working once the user has enrolled. **Part 2 done (MOH-34):** `AUTH_MFA_REQUIRED` (default on in production and staging) refuses sessions without a second factor (`403 mfa_enrolment_required`); sensitive actions need a code proven in the last five minutes (`401 mfa_step_up_required`; `POST /auth/mfa/step-up`); recovery codes can be replaced; an owner (single-workspace, non-staff members) or a platform administrator can reset a factor, which ends the user's sessions and is audited. **Still open:** the dashboard (MOH-57). M2 (WebAuthn) after that. |
| A7 | There's no per-account lockout. Only the `(email, ip)` window limit exists. | `backend/crates/server/src/rate_limit.rs` (`login_rate_limit`) | Add a per-account failure counter with exponential backoff. |
| A8 | SSE streams aren't tied to session revocation. | `escalations` / `notifications` SSE handlers | Record the `sid` on subscribe and close the stream on revocation (with ADR-009). |
