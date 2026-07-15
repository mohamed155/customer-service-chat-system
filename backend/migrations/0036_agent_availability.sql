-- Migration 0036: Agent Availability — per-membership toggle state
--
-- Default away (FR-016). Row created lazily on first toggle;
-- absent row ≡ away. Presence is runtime-only (research R2),
-- not stored here.

CREATE TABLE agent_availability (
    tenant_id UUID NOT NULL,
    membership_id UUID NOT NULL,
    state TEXT NOT NULL DEFAULT 'away' CHECK (state IN ('available', 'away')),
    state_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, membership_id),
    FOREIGN KEY (tenant_id, membership_id)
        REFERENCES tenant_memberships (tenant_id, id) ON DELETE CASCADE
);

-- set_updated_at trigger
CREATE OR REPLACE FUNCTION set_agent_availability_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_agent_availability_updated_at
    BEFORE UPDATE ON agent_availability
    FOR EACH ROW
    EXECUTE FUNCTION set_agent_availability_updated_at();
