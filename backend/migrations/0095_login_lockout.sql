-- Per-account login throttling is global identity state: no tenant is known at sign-in.
ALTER TABLE users
    ADD COLUMN failed_login_count integer NOT NULL DEFAULT 0,
    ADD COLUMN last_failed_login_at timestamptz,
    ADD COLUMN locked_until timestamptz,
    ADD CONSTRAINT users_failed_login_count_nonnegative
        CHECK (failed_login_count >= 0) NOT VALID;

COMMENT ON COLUMN users.failed_login_count IS
    'Consecutive password sign-in failures reserved across all client IPs; reset after success or password reset.';
COMMENT ON COLUMN users.last_failed_login_at IS
    'Time the latest password sign-in failure slot was reserved, used to expire old failure streaks.';
COMMENT ON COLUMN users.locked_until IS
    'Account password sign-in is refused until this timestamp; client responses remain generic.';
