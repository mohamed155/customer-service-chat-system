-- Migration 0039: AI credentials — encrypted API keys per provider
--
-- One credential row per (tenant_id, provider) scope, soft-delete to rotate.
-- Ciphertext and nonce are stored as BYTEA; key_hint aids admin identification.

CREATE TABLE ai_credentials (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL CHECK (provider IN ('openai','anthropic','gemini')),
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    key_hint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX ai_credentials_tenant_provider_live_uq
    ON ai_credentials (tenant_id, provider)
    WHERE tenant_id IS NOT NULL AND deleted_at IS NULL;

CREATE UNIQUE INDEX ai_credentials_platform_provider_live_uq
    ON ai_credentials (provider)
    WHERE tenant_id IS NULL AND deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_ai_credentials_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_ai_credentials_updated_at
    BEFORE UPDATE ON ai_credentials
    FOR EACH ROW
    EXECUTE FUNCTION set_ai_credentials_updated_at();
