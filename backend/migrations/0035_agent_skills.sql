-- Migration 0035: Agent Skills catalog and agent-skill assignments
--
-- 1. `skills` — tenant-defined skill catalog (FR-018)
-- 2. `agent_skills` — per-agent skill assignments (join table)

-- -------------------------------------------------------------------
-- skills
-- -------------------------------------------------------------------
CREATE TABLE skills (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (char_length(trim(name)) BETWEEN 1 AND 50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Composite FK target for agent_skills
ALTER TABLE skills ADD CONSTRAINT skills_tenant_id_id_uq UNIQUE (tenant_id, id);

-- Case-insensitive per-tenant uniqueness
CREATE UNIQUE INDEX skills_tenant_lower_name_uniq ON skills (tenant_id, lower(name));

-- set_updated_at trigger
CREATE OR REPLACE FUNCTION set_skills_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_skills_updated_at
    BEFORE UPDATE ON skills
    FOR EACH ROW
    EXECUTE FUNCTION set_skills_updated_at();

-- -------------------------------------------------------------------
-- agent_skills
-- -------------------------------------------------------------------
CREATE TABLE agent_skills (
    tenant_id UUID NOT NULL,
    membership_id UUID NOT NULL,
    skill_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, membership_id, skill_id),
    FOREIGN KEY (tenant_id, membership_id)
        REFERENCES tenant_memberships (tenant_id, id),
    FOREIGN KEY (tenant_id, skill_id)
        REFERENCES skills (tenant_id, id) ON DELETE CASCADE
);

-- Index for candidate-selection match counting
CREATE INDEX agent_skills_tenant_skill_idx ON agent_skills (tenant_id, skill_id);
