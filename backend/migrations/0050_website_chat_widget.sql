-- Migration 0050: Website Chat Widget — widget_instances, widget_sessions,
-- and conversations.widget_instance_id. See specs/023-website-chat-widget/
-- for the full design.

-- Table: widget_instances — tenant-owned embeddable widget definitions (US5).
CREATE TABLE widget_instances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    public_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    display_name TEXT NOT NULL DEFAULT 'Support' CHECK (char_length(display_name) BETWEEN 1 AND 80),
    primary_color TEXT NOT NULL DEFAULT '#4F46E5' CHECK (primary_color ~ '^#[0-9a-fA-F]{6}$'),
    welcome_message TEXT NOT NULL DEFAULT 'Hi! How can we help?' CHECK (char_length(welcome_message) <= 500),
    position TEXT NOT NULL DEFAULT 'bottom-right' CHECK (position IN ('bottom-right', 'bottom-left')),
    theme TEXT NOT NULL DEFAULT 'light' CHECK (theme IN ('light', 'dark')),
    enabled BOOLEAN NOT NULL DEFAULT true,
    allowed_domains TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX widget_instances_tenant_active_idx
    ON widget_instances (tenant_id)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX widget_instances_public_id_active_uq
    ON widget_instances (public_id)
    WHERE deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_widget_instances_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_widget_instances_updated_at
    BEFORE UPDATE ON widget_instances
    FOR EACH ROW
    EXECUTE FUNCTION set_widget_instances_updated_at();

-- Table: widget_sessions — anonymous visitor sessions (US4). No PII.
CREATE TABLE widget_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    widget_instance_id UUID NOT NULL REFERENCES widget_instances(id) ON DELETE RESTRICT,
    token_hash BYTEA NOT NULL,
    customer_id UUID NULL REFERENCES customers(id) ON DELETE SET NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX widget_sessions_token_hash_uq
    ON widget_sessions (token_hash);

CREATE INDEX widget_sessions_tenant_instance_idx
    ON widget_sessions (tenant_id, widget_instance_id);

CREATE INDEX widget_sessions_expires_at_idx
    ON widget_sessions (expires_at);

-- Alter conversations to attribute widget origin (FR-032).
ALTER TABLE conversations
    ADD COLUMN widget_instance_id UUID NULL
    REFERENCES widget_instances(id) ON DELETE SET NULL;

CREATE INDEX conversations_widget_instance_idx
    ON conversations (tenant_id, widget_instance_id)
    WHERE widget_instance_id IS NOT NULL;
