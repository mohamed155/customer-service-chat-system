-- Validate the expanded counter constraint in its own migration transaction so the table's
-- validation lock is not held while other account-lockout DDL runs.
ALTER TABLE users VALIDATE CONSTRAINT users_failed_login_count_nonnegative;
