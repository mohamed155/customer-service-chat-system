# Data Model: Authentication

All storage follows feature 005's conventions (UUID PKs, timestamptz, migrations only). One new migration: `backend/migrations/0007_auth.sql`.

## 1. `users.password_hash` (column addition)

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| `password_hash` | `TEXT` | `NULL` | PHC-format Argon2id string (self-describing parameters). `NULL` = user has no credential yet → login fails identically to a wrong password (FR-003). Never selected into logs or API responses. |

Constraint: `CHECK (password_hash IS NULL OR password_hash LIKE '$argon2%')` — cheap guard against accidentally storing plaintext.

No index: lookups are by `email` (existing `users_email_active_uniq` partial index) — the hash is fetched on the already-indexed row.

## 2. `revoked_sessions` (new table)

Records sessions invalidated before their natural expiry (sign-out). Rows are dead weight after `expires_at`; a later housekeeping feature may prune them.

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| `jti` | `UUID` | `PRIMARY KEY` | The revoked token's unique id claim. |
| `user_id` | `UUID` | `NOT NULL REFERENCES users(id) ON DELETE RESTRICT` | For operator forensics ("which sessions did this user kill"). |
| `expires_at` | `TIMESTAMPTZ` | `NOT NULL` | Copy of the token's `exp`; after this instant the row is prunable (the token would fail expiry anyway). |
| `revoked_at` | `TIMESTAMPTZ` | `NOT NULL DEFAULT now()` | When sign-out happened. |

Not tenant-owned (sessions belong to users, who span tenants) — no `tenant_id`, consistent with `users` itself. PK on `jti` is the only lookup path (per-request `NOT EXISTS`).

## 3. Session token (runtime credential — not a table)

JWT, HS256, signed with `AUTH_JWT_SECRET`. Claims:

| Claim | Type | Meaning |
|-------|------|---------|
| `sub` | UUID string | `users.id` |
| `jti` | UUID string | Unique session id; the revocation handle |
| `iat` | unix seconds | Issued at |
| `exp` | unix seconds | `iat + AUTH_SESSION_TTL_SECONDS` (default 28800 = 8h, per spec clarification) |

No email, name, role, or tenant claims — the per-request query re-reads current user state (FR-006/spec edge case "role changes take effect next request"). Carried exclusively in the `app_session` cookie (`HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800`).

**Lifecycle**: issue (login) → validate per request (signature + `exp`, then one query: user row `deleted_at IS NULL` AND `NOT EXISTS revoked_sessions(jti)`) → end by expiry or revocation (logout inserts `jti`).

## 4. Request-pipeline artifacts (runtime, existing shapes)

- **`Principal`** (identity crate, unchanged shape): now produced from a valid session cookie in all environments, or from `X-Dev-User-Id` in dev/test only.
- **`SessionClaims`** (new request extension): `{ jti: Uuid, expires_at: DateTime }` attached alongside `Principal` when the principal came from a cookie — logout reads it to know what to revoke. Absent for dev-header principals (logout then just clears the cookie).

## 5. Audit actions (rows in existing `audit_logs`)

| action | actor_user_id | resource_type / resource_id | tenant_id | details |
|--------|--------------|------------------------------|-----------|---------|
| `auth.login_succeeded` | user | `user` / user id | NULL | `{}` |
| `auth.login_failed` | NULL | `user` / NULL | NULL | `{ "email": <attempted>, "reason": "invalid_credentials" }` |
| `auth.logged_out` | user | `user` / user id | NULL | `{ "jti": <revoked jti> }` |

Same fail-open insert discipline as 006 (`tracing::error!` and continue — an audit outage must not lock users out; append-only trigger already enforced by 0006).

## 6. Configuration (environment, not storage)

| Variable | Type | Default | Notes |
|----------|------|---------|-------|
| `AUTH_JWT_SECRET` | string | — (required) | ≥ 32 bytes; redacted in `AppConfig` Debug (Constitution III) |
| `AUTH_SESSION_TTL_SECONDS` | u64 | `28800` | 8h per spec clarification; overridable in tests |

## 7. Frontend state (no new mechanisms)

- **Auth state** = `CurrentUserService.currentUser` signal (existing) — `null` means signed out. No token exists client-side (httpOnly cookie); nothing new persists to localStorage.
- **Return destination**: carried as a `returnUrl` query param on the login route — transient, never stored.
- Tenant-context slice (006) is cleared on logout and on 401-mid-session alongside the user.
