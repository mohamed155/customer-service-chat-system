CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug CITEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT tenants_name_length CHECK (length(name) BETWEEN 1 AND 200),
    CONSTRAINT tenants_slug_format CHECK (
        slug::text ~ '^[a-z0-9](-?[a-z0-9])*$' AND
        length(slug::text) <= 63
    ),
    CONSTRAINT tenants_status_check CHECK (status IN ('active', 'suspended'))
);

CREATE UNIQUE INDEX tenants_slug_active_uniq
    ON tenants (slug)
    WHERE deleted_at IS NULL;

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
