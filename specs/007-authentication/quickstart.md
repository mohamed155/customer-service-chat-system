# Quickstart: Authentication

Validation guide proving the feature end-to-end. References: [contracts/http-api.md](contracts/http-api.md), [data-model.md](data-model.md).

## Prerequisites

- Docker Compose Postgres up; migrations applied through `0007_auth.sql` (`sqlx migrate run` from `backend/`).
- `AUTH_JWT_SECRET` set (≥ 32 chars) in the backend environment; `APP_ENVIRONMENT=development`.
- Backend running on `http://localhost:8080`; dashboard via `pnpm ng serve dashboard` on `http://localhost:4200`.

## 1. Seed a user with a known password

Generate an Argon2id hash for a test password (any PHC-producing tool works; the repo's identity crate exposes a small example/test helper — see tasks). Then:

```sql
INSERT INTO users (email, display_name, platform_role, password_hash)
VALUES ('admin@quickstart.test', 'Quickstart Admin', 'super_admin', '<phc-hash-of Passw0rd!>');
```

## 2. Backend matrix (curl; `-c`/`-b` manage the cookie jar)

```bash
# Login success → 200 MeResponse; inspect Set-Cookie flags (HttpOnly; Secure; SameSite=Lax; Max-Age=28800)
curl -si -c jar.txt -X POST http://localhost:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"admin@quickstart.test","password":"Passw0rd!"}'

# Wrong password vs unknown email → both 401, bodies identical except request_id
curl -s -X POST http://localhost:8080/api/v1/auth/login -H 'Content-Type: application/json' \
  -d '{"email":"admin@quickstart.test","password":"nope"}'
curl -s -X POST http://localhost:8080/api/v1/auth/login -H 'Content-Type: application/json' \
  -d '{"email":"ghost@quickstart.test","password":"nope"}'

# Cookie authenticates /me → 200 with platform role
curl -s -b jar.txt http://localhost:8080/api/v1/me

# CSRF: state-changing request with a foreign Origin → 403 even with the valid cookie
curl -si -b jar.txt -X POST http://localhost:8080/api/v1/auth/logout -H 'Origin: https://evil.example'

# Logout → 204 + clearing Set-Cookie; replaying the old cookie → 401
curl -si -b jar.txt -X POST http://localhost:8080/api/v1/auth/logout \
  -H 'Origin: http://localhost:4200'
curl -s -b jar.txt http://localhost:8080/api/v1/me   # expect 401 unauthenticated

# Audit trail: expect auth.login_succeeded, 2× auth.login_failed, auth.logged_out
psql "$DATABASE_URL" -c "SELECT action, actor_user_id, details FROM audit_logs
  WHERE action LIKE 'auth.%' ORDER BY created_at DESC LIMIT 5;"

# Revocation row present
psql "$DATABASE_URL" -c "SELECT jti, expires_at FROM revoked_sessions ORDER BY revoked_at DESC LIMIT 1;"
```

## 3. Production gating check (006 FR-019 preserved)

Restart the backend with `APP_ENVIRONMENT=production` (and prod-shaped CORS):

```bash
# Dev header must be dead; only real sessions authenticate
curl -s -H "X-Dev-User-Id: <any-user-uuid>" http://localhost:8080/api/v1/me   # expect 401
# Login still works and its cookie authenticates as in §2
```

## 4. Automated suites

```bash
# Backend — auth matrix (live-gated; skips politely without DATABASE_URL, runs in CI)
cd backend && cargo test -p server --test auth
# Full gates
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace

# Frontend
cd ../frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check
```

## 5. Frontend walkthrough (dev, `localhost:4200`)

1. Open any protected route (e.g. `/tenant/overview`) signed out → redirected to `/auth/login?returnUrl=/tenant/overview`.
2. Sign in with the seeded credentials → land on `/tenant/overview`; DevTools → Network shows requests carrying the cookie (no `Authorization` header, no token in JS-visible storage).
3. Reload → still signed in (cookie persisted; `/me` re-resolves the user).
4. Wrong password on the form → generic "Invalid email or password" message; the form stays usable.
5. Visit `/auth/login` while signed in → redirected into the app.
6. Sign out → back at login; opening a protected route redirects to login; audit shows `auth.logged_out`.
7. (Session-expired path) Delete the `app_session` cookie in DevTools, then click anything that calls the API → clean return to login with a session-expired notice, no raw errors.

## Expected outcomes recap

- SC-001: seeded user reaches the product on first login attempt.
- SC-002/FR-003: the three failure curls return byte-identical 401 bodies.
- SC-003: unauthenticated `/me` and post-logout replay → 401; guarded routes never render signed out.
- SC-004: reload keeps the session; expiry/sign-out paths land on login cleanly.
- SC-005: four `auth.*` audit rows from §2.
- SC-006: `cargo test -p server --test auth` green in CI's Postgres job.
