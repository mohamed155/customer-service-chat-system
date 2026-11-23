-- MOH-93. Apply after PR #146 is fully rolled out and its 8-hour transition window has elapsed.
-- The old API uses the denylist for both legacy sessions and completed MFA challenges.

ALTER TABLE auth_sessions ADD COLUMN mfa_challenge_id uuid;

CREATE UNIQUE INDEX auth_sessions_mfa_challenge_id
    ON auth_sessions (mfa_challenge_id) WHERE mfa_challenge_id IS NOT NULL;

COMMENT ON COLUMN auth_sessions.mfa_challenge_id IS
    'The one-time MFA sign-in challenge that created this session, when applicable.';

DROP TABLE revoked_sessions;
ALTER TABLE users DROP COLUMN sessions_valid_after;
