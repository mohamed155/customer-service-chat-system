-- Migration 0045: Agent prompt management — versioned, append-only system
-- prompt content, superseding agent_configurations.system_prompt. Every
-- save creates an immutable version; history is browsable; restore is
-- roll-forward. See specs/018-prompt-management/ for the full design.

-- Table: agent_prompts — the tenant's managed prompt object (one per tenant
-- in v1, discriminated by prompt_kind for future multi-prompt support).
CREATE TABLE agent_prompts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    prompt_kind TEXT NOT NULL DEFAULT 'system' CHECK (prompt_kind IN ('system')),
    active_version INTEGER NOT NULL CHECK (active_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ NULL
);

CREATE UNIQUE INDEX agent_prompts_tenant_kind_uq
    ON agent_prompts (tenant_id, prompt_kind)
    WHERE deleted_at IS NULL;

CREATE OR REPLACE FUNCTION set_agent_prompts_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_agent_prompts_updated_at
    BEFORE UPDATE ON agent_prompts
    FOR EACH ROW
    EXECUTE FUNCTION set_agent_prompts_updated_at();

-- Table: agent_prompt_versions — immutable, append-only content snapshots.
CREATE TABLE agent_prompt_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    prompt_id UUID NOT NULL REFERENCES agent_prompts(id) ON DELETE RESTRICT,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    content TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 8000),
    change_note TEXT NULL CHECK (change_note IS NULL OR char_length(change_note) <= 500),
    restored_from INTEGER NULL,
    created_by_user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_by_display TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX agent_prompt_versions_prompt_version_uq
    ON agent_prompt_versions (prompt_id, version_number);

CREATE INDEX agent_prompt_versions_tenant_prompt_created_idx
    ON agent_prompt_versions (tenant_id, prompt_id, version_number DESC);

CREATE TRIGGER agent_prompt_versions_append_only
    BEFORE UPDATE OR DELETE ON agent_prompt_versions
    FOR EACH ROW
    EXECUTE FUNCTION forbid_mutation();

-- Backfill: migrate existing live system prompts into the versioned model.
INSERT INTO agent_prompts (tenant_id, prompt_kind, active_version)
SELECT tenant_id, 'system', 1
FROM agent_configurations
WHERE deleted_at IS NULL AND trim(system_prompt) <> '';

INSERT INTO agent_prompt_versions (tenant_id, prompt_id, version_number, content, created_by_user_id, created_by_display)
SELECT ac.tenant_id, ap.id, 1, ac.system_prompt, NULL, 'Migration backfill'
FROM agent_configurations ac
JOIN agent_prompts ap ON ap.tenant_id = ac.tenant_id AND ap.prompt_kind = 'system'
WHERE ac.deleted_at IS NULL AND trim(ac.system_prompt) <> '';

ALTER TABLE agent_configurations DROP COLUMN system_prompt;
