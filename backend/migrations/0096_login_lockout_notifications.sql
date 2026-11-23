-- Owner notifications are sent by a worker and contain no password or recovery token.
CREATE TABLE login_lockout_notifications (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    locked_until timestamptz NOT NULL,
    failed_attempts integer NOT NULL CHECK (failed_attempts > 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    request_trace_id text NOT NULL,
    available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0)
);

CREATE INDEX login_lockout_notifications_available
    ON login_lockout_notifications (available_at, id);
CREATE INDEX login_lockout_notifications_user_created
    ON login_lockout_notifications (user_id, created_at);

-- app_runtime receives DML on new tables from the defaults established in migration 0066.
