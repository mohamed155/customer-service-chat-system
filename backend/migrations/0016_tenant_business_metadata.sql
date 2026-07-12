-- Add business metadata columns to tenants for platform tenant management.
-- These columns back the platform-tenant-management CRUD endpoints.

ALTER TABLE tenants
    ADD COLUMN plan TEXT NOT NULL DEFAULT 'trial',
    ADD COLUMN contact_name TEXT NULL,
    ADD COLUMN contact_email TEXT NULL;

-- Constraint: plan must be one of the four configured plans.
ALTER TABLE tenants
    ADD CONSTRAINT tenants_plan_check CHECK (plan IN ('trial', 'starter', 'professional', 'enterprise'));

-- Constraint: contact_name must be 1-200 characters when present.
-- (contact_email format is validated application-side, mirroring the pattern in migration 0003.)
ALTER TABLE tenants
    ADD CONSTRAINT tenants_contact_name_length CHECK (contact_name IS NULL OR length(contact_name) BETWEEN 1 AND 200);
