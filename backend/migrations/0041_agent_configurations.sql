-- Migration 0041: AI agent configurations — the tenant's configurable AI
-- agent. Multi-agent-shaped (named, one designated default) with a single
-- extra partial-unique index enforcing the v1 "exactly one agent" rule;
-- dropping that one index is the entire multi-agent unlock (see research.md R2).

CREATE TABLE agent_configurations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (char_length(trim(name)) BETWEEN 1 AND 80),
    is_default BOOLEAN NOT NULL DEFAULT true,
    avatar_kind TEXT NOT NULL DEFAULT 'preset' CHECK (avatar_kind IN ('preset', 'upload')),
    avatar_preset TEXT NULL,
    tone TEXT NOT NULL DEFAULT 'professional'
        CHECK (tone IN ('professional', 'friendly', 'casual', 'formal', 'empathetic')),
    system_prompt TEXT NOT NULL DEFAULT '' CHECK (char_length(system_prompt) <= 8000),
    business_rules JSONB NOT NULL DEFAULT '[]',
    escalation_rules JSONB NOT NULL DEFAULT '[]',
    enabled_channels JSONB NOT NULL DEFAULT '["web_chat"]',
    provider TEXT NULL CHECK (provider IN ('openai', 'anthropic', 'gemini')),
    model TEXT NULL CHECK (model IS NULL OR char_length(trim(model)) >= 1),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL,

    CONSTRAINT agent_configurations_provider_model_pair CHECK (
        (provider IS NULL AND model IS NULL) OR (provider IS NOT NULL AND model IS NOT NULL)
    )
);

CREATE UNIQUE INDEX agent_configurations_tenant_single_live_uq
    ON agent_configurations (tenant_id)
    WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX agent_configurations_tenant_default_uq
    ON agent_configurations (tenant_id)
    WHERE is_default AND deleted_at IS NULL;

CREATE UNIQUE INDEX agent_configurations_tenant_name_uq
    ON agent_configurations (tenant_id, lower(name))
    WHERE deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_agent_configurations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_agent_configurations_updated_at
    BEFORE UPDATE ON agent_configurations
    FOR EACH ROW
    EXECUTE FUNCTION set_agent_configurations_updated_at();

CREATE TABLE agent_avatar_uploads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    agent_id UUID NOT NULL REFERENCES agent_configurations(id) ON DELETE CASCADE,
    content_type TEXT NOT NULL CHECK (content_type IN ('image/png', 'image/jpeg', 'image/webp')),
    bytes BYTEA NOT NULL CHECK (octet_length(bytes) <= 262144),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX agent_avatar_uploads_agent_live_uq
    ON agent_avatar_uploads (agent_id)
    WHERE deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_agent_avatar_uploads_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_agent_avatar_uploads_updated_at
    BEFORE UPDATE ON agent_avatar_uploads
    FOR EACH ROW
    EXECUTE FUNCTION set_agent_avatar_uploads_updated_at();
