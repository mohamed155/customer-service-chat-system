ALTER TABLE users
    ADD COLUMN password_hash TEXT NULL,
    ADD CONSTRAINT users_password_hash_argon2_check CHECK (
        password_hash IS NULL OR
        password_hash LIKE '$argon2%'
    );

CREATE TABLE revoked_sessions (
    jti UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
