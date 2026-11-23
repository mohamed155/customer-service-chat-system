-- Password recovery is a global identity operation, before a tenant is known.
-- A credential timestamp also invalidates pending MFA challenges and in-flight password proofs.
ALTER TABLE users ADD COLUMN password_reset_at timestamptz;

CREATE TABLE password_reset_requests (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    email text NOT NULL,
    ip inet,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    request_trace_id text NOT NULL,
    available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    attempts integer NOT NULL DEFAULT 0
);

CREATE INDEX password_reset_requests_available ON password_reset_requests (available_at, id);
CREATE INDEX password_reset_requests_email_order
    ON password_reset_requests ((email::citext), created_at, id);

CREATE TABLE password_reset_tokens (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Independent of queue retention. Orders issuance and retries across workers.
    request_id uuid NOT NULL,
    requested_at timestamptz NOT NULL,
    token_hash text NOT NULL UNIQUE CHECK (length(token_hash) = 64),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    ip inet,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '30 minutes')
);

CREATE INDEX password_reset_tokens_user_request ON password_reset_tokens (user_id, requested_at, request_id);
CREATE UNIQUE INDEX password_reset_tokens_one_unused
    ON password_reset_tokens (user_id) WHERE used_at IS NULL;
CREATE INDEX password_reset_tokens_expires ON password_reset_tokens (expires_at);

-- Grants to app_runtime are inherited from the app_owner defaults set in migration 0066.
