CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE RESTRICT,
    channel TEXT NOT NULL,
    status TEXT NOT NULL,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,
    CONSTRAINT conversations_channel_check CHECK (
        channel IN ('email', 'phone', 'web_chat', 'whatsapp', 'telegram')
    ),
    CONSTRAINT conversations_status_check CHECK (
        status IN ('open', 'escalated', 'closed')
    )
);

CREATE INDEX conversations_customer_recent_idx
    ON conversations (tenant_id, customer_id, last_activity_at DESC)
    WHERE deleted_at IS NULL;

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON conversations
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
