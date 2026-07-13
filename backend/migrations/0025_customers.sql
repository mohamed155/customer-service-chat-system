CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE customers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    display_name TEXT NOT NULL,
    email CITEXT NULL,
    phone TEXT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT customers_display_name_length CHECK (length(display_name) BETWEEN 1 AND 200),
    CONSTRAINT customers_metadata_object CHECK (jsonb_typeof(metadata) = 'object')
);

CREATE INDEX customers_tenant_cursor_idx
    ON customers (tenant_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX customers_display_name_trgm_idx
    ON customers USING GIN (display_name gin_trgm_ops);

CREATE INDEX customers_email_trgm_idx
    ON customers USING GIN ((email::text) gin_trgm_ops);

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON customers
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TABLE customer_channel_identifiers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE RESTRICT,
    channel TEXT NOT NULL,
    identifier TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT customer_channel_identifiers_channel_check CHECK (
        channel IN ('email', 'phone', 'web_chat', 'whatsapp', 'telegram')
    ),
    CONSTRAINT customer_channel_identifiers_identifier_length CHECK (
        length(identifier) BETWEEN 1 AND 320
    )
);

CREATE UNIQUE INDEX customer_channel_identifiers_unique_idx
    ON customer_channel_identifiers (tenant_id, channel, identifier);

CREATE INDEX customer_channel_identifiers_customer_idx
    ON customer_channel_identifiers (customer_id);
