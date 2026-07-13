-- Add status column to tenant_memberships for disable/re-enable support
ALTER TABLE tenant_memberships
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active',
    ADD CONSTRAINT tenant_memberships_status_check CHECK (status IN ('active', 'disabled'));
