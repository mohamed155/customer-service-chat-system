CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email CITEXT NOT NULL,
    display_name TEXT NOT NULL,
    platform_role TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT users_email_format CHECK (position('@' in email::text) > 1),
    CONSTRAINT users_display_name_length CHECK (length(display_name) BETWEEN 1 AND 200),
    CONSTRAINT users_platform_role_check CHECK (
        platform_role IS NULL OR
        platform_role IN ('super_admin', 'developer', 'sales', 'support', 'finance')
    )
);

CREATE UNIQUE INDEX users_email_active_uniq
    ON users (email)
    WHERE deleted_at IS NULL;

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
