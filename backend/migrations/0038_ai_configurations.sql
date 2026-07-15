-- Migration 0038: AI configurations — provider, model, fallback chain
--
-- Tenant-scoped or platform-wide AI configuration rows. Only one live row
-- per scope (tenant or platform). Soft-delete to replace.

CREATE TABLE ai_configurations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL CHECK (provider IN ('openai','anthropic','gemini')),
    model TEXT NOT NULL CHECK (char_length(trim(model)) >= 1),
    max_output_tokens INTEGER NULL CHECK (max_output_tokens > 0),
    temperature REAL NULL CHECK (temperature >= 0 AND temperature <= 2),
    fallbacks JSONB NOT NULL DEFAULT '[]',
    capture_content BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX ai_configurations_tenant_live_uq
    ON ai_configurations (tenant_id)
    WHERE tenant_id IS NOT NULL AND deleted_at IS NULL;

CREATE UNIQUE INDEX ai_configurations_platform_live_uq
    ON ai_configurations ((true))
    WHERE tenant_id IS NULL AND deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_ai_configurations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_ai_configurations_updated_at
    BEFORE UPDATE ON ai_configurations
    FOR EACH ROW
    EXECUTE FUNCTION set_ai_configurations_updated_at();
