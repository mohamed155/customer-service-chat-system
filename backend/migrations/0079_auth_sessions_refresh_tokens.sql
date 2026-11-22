-- Migration 0079: sessions and rotating refresh tokens (MOH-93, ADR-005 §3, gaps A1, A2, A5).
--
-- A sign-in creates one `auth_sessions` row. Its id is the `sid` claim of every access token the
-- session ever gets, so revoking the row ends the session, whatever access tokens are still in
-- browsers. The access token lives ten minutes; the session lives as long as its refresh tokens
-- keep being exchanged.
--
-- `refresh_tokens` holds one row per refresh token ever issued for a session, chained through
-- `parent_id`. A token is used once. Presenting a used token again is a replay: the session is
-- revoked with the reason `refresh_reuse` (unless it is the second tab of the same refresh, see the
-- grace window in `identity::sessions`). Only the SHA-256 of a token is stored.
--
-- Both tables are about users, who have no tenant, so they are global tables (`GLOBAL_TABLES`).
-- The `revoked_sessions` table of the HS256 tokens stays until the cutover window closes.

CREATE TABLE auth_sessions (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    user_id             uuid        NOT NULL REFERENCES users (id),
    -- How the session was earned, copied into every access token it gets (RFC 8176 values).
    amr                 text[]      NOT NULL,
    -- When the second factor was last proven. A refresh does not renew it: step-up stays a
    -- decision of the moment the user proved it.
    mfa_at              timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    last_seen_at        timestamptz NOT NULL DEFAULT now(),
    -- Each refresh pushes this out, up to the absolute limit.
    idle_expires_at     timestamptz NOT NULL,
    absolute_expires_at timestamptz NOT NULL,
    revoked_at          timestamptz,
    revoke_reason       text,
    user_agent          text,
    ip                  text,
    CONSTRAINT auth_sessions_revoke_reason_with_time CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
    CONSTRAINT auth_sessions_idle_within_absolute CHECK (idle_expires_at <= absolute_expires_at)
);

-- "Revoke every session of this user" and the middleware's per-request lookup.
CREATE INDEX auth_sessions_user_active ON auth_sessions (user_id) WHERE revoked_at IS NULL;

CREATE TABLE refresh_tokens (
    id         uuid        PRIMARY KEY DEFAULT uuidv7(),
    session_id uuid        NOT NULL REFERENCES auth_sessions (id) ON DELETE CASCADE,
    -- SHA-256 of the token; the token itself is never stored.
    token_hash bytea       NOT NULL UNIQUE,
    -- The token this one replaced.
    parent_id  uuid        REFERENCES refresh_tokens (id) ON DELETE SET NULL,
    issued_at  timestamptz NOT NULL DEFAULT now(),
    used_at    timestamptz,
    expires_at timestamptz NOT NULL,
    CONSTRAINT refresh_tokens_hash_is_sha256 CHECK (octet_length(token_hash) = 32)
);

CREATE INDEX refresh_tokens_session ON refresh_tokens (session_id);
CREATE INDEX refresh_tokens_parent ON refresh_tokens (parent_id);

COMMENT ON TABLE auth_sessions IS
    'One row per sign-in. Its id is the sid claim of the access tokens; revoking it ends the session.';
COMMENT ON TABLE refresh_tokens IS
    'Opaque refresh tokens, stored hashed, used once each, chained by parent_id. Reuse revokes the session.';
