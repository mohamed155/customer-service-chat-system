-- Migration 0078: invalidate a user's sessions on demand (MOH-34).
--
-- A session is a signed token whose id is recorded in `revoked_sessions` only when that one session
-- logs out. When an administrator resets a user's second factor, every session that user already
-- has must stop working too, and the server does not keep a list of them. Instead the user row
-- records a moment, and a session issued before it is refused.

ALTER TABLE users ADD COLUMN sessions_valid_after timestamptz;

COMMENT ON COLUMN users.sessions_valid_after IS
    'Sessions issued before this moment are refused (set when an administrator resets MFA).';
