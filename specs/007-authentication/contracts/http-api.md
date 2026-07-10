# HTTP API Contract: Authentication

Extends the 001 REST contract (`specs/001-ai-customer-service-platform/contracts/rest-api.md`) and the 006 contract (`specs/006-multi-tenancy-foundation/contracts/http-api.md`). All responses use the kernel envelope; all requests/responses are JSON.

## A. Session mechanics & recorded divergences

- The dashboard session is a **JWT carried in an httpOnly cookie** (`app_session`), not the Authorization-header "opaque session token" described in 001's prose. Divergence driven by the spec clarification (script-inaccessible credential) and the feature description (JWT); the Authorization header remains reserved for future API-key and widget-token callers per 001.
- The user description's `GET /auth/me` is **satisfied by the existing `GET /api/v1/me`** (006) — no alias is added; one path per resource (001 consistency rule).
- Cookie contract (set on login):

  ```text
  Set-Cookie: app_session=<jwt>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800
  ```

  Cleared on logout with the same attributes and `Max-Age=0`. The token never appears in any response body. Clients MUST send requests with credentials (browser attaches the cookie automatically; SPA uses `withCredentials`).
- CORS: allowed origins are the configured explicit list, now with `Access-Control-Allow-Credentials: true`.

## B. `POST /api/v1/auth/login` — public

Request body:

```json
{ "email": "user@example.com", "password": "..." }
```

| Case | Status | Body |
|------|--------|------|
| Valid credentials, active user | 200 | `MeResponse` (identical shape to `GET /me`) + `Set-Cookie` above |
| Missing/blank email or password, non-JSON body | 400 | `validation_failed` envelope |
| Wrong password | 401 | `unauthenticated`, message `"Invalid email or password"` |
| Unknown email | 401 | **byte-identical** to wrong-password (modulo `request_id`) |
| Deactivated (soft-deleted) user | 401 | byte-identical to the above |
| User with no credential set (`password_hash IS NULL`) | 401 | byte-identical to the above |

- The four 401 bodies are pinned byte-identical by tests (anti-enumeration, FR-003); verification work is performed on every failure path (dummy-verify) to avoid a timing oracle.
- Login while already authenticated: allowed; issues a fresh session cookie (previous token remains subject to its own expiry/revocation lifecycle).
- Audit: success → `auth.login_succeeded`; failure → `auth.login_failed` with `{ email, reason }`.
- `X-Tenant-ID`, if supplied, is ignored (platform-scoped operation per 006 FR-004).

## C. `POST /api/v1/auth/logout` — any authenticated principal

No request body.

| Case | Status | Body |
|------|--------|------|
| Valid session cookie | 204 | empty + clearing `Set-Cookie`; the token's `jti` is inserted into `revoked_sessions` |
| Dev-header principal (no cookie session) | 204 | clearing `Set-Cookie` only (nothing to revoke) |
| No principal | 401 | `unauthenticated` envelope |

- After logout, replaying the previous cookie on any protected route → 401 (revocation is server-side, test-pinned).
- Audit: `auth.logged_out` with `{ jti }`.
- Idempotency: logging out an already-revoked session is impossible in practice (the cookie no longer authenticates → 401), which is acceptable per 001's idempotency-where-semantics-allow rule.

## D. Session validation on protected routes (all `/api/v1` except login)

Evaluation order in `principal_middleware` (all environments):

1. `app_session` cookie present → verify JWT signature and `exp` → single query: user exists with `deleted_at IS NULL` **and** `jti` not in `revoked_sessions` → attach `Principal` + `SessionClaims`.
2. Cookie absent/invalid **and** environment is development/test → existing `X-Dev-User-Id` path (006, unchanged).
3. Otherwise no principal → protected handlers reject with 401 `unauthenticated` (existing extractor behavior).

| Presented credential | Result |
|----------------------|--------|
| Valid, unexpired, unrevoked cookie; active user | Principal attached |
| Expired token | 401 |
| Tampered/garbage token (bad signature, malformed) | 401 |
| Revoked `jti` (post-logout replay) | 401 |
| Valid token, user since soft-deleted | 401 |
| No cookie, prod/staging, any `X-Dev-User-Id` | 401 (dev header hard-disabled — 006 FR-019 preserved) |

All 401s use the kernel `unauthenticated` envelope; no distinction is leaked between expiry/tamper/revocation.

## E. CSRF protection (FR-005a)

- Cookie is `SameSite=Lax` (browser-level cross-site suppression).
- `csrf_origin_middleware` on `/api/v1`: for state-changing methods (anything except GET/HEAD/OPTIONS), if an `Origin` header is present and not in the configured allowlist → 403 `unauthorized` before any handler. Same-origin requests (no `Origin` or matching) and non-browser clients are unaffected.
- Test-pinned: POST with a foreign `Origin` → 403 even with a valid session cookie.

## F. Frontend contract

- All `apiBaseUrl` requests are sent with credentials (cookie attached by the browser); application code never reads, stores, or forwards a token.
- Bootstrap: app initializer calls `GET /me`; 401 resolves to signed-out state (no error surface).
- `authGuard` (canMatch, shell routes): signed-out → redirect `/auth/login?returnUrl=<attempted URL>`. `guestGuard` (auth routes): signed-in → redirect into the app.
- Login page: submits to §B; on 200 loads the current user and navigates to `returnUrl` (validated as an internal path) or the default route; on 401 shows the generic invalid-credentials message; pending state disables resubmission.
- 401 mid-session on any `apiBaseUrl` response (except the login call): clear current user + tenant context, navigate to login with a session-expired notice (FR-014); never render stale authenticated UI.
- Logout control invokes §C, clears user + tenant context, navigates to login.
