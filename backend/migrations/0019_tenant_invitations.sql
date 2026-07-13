-- Create tenant_invitations table for email-bound single-use invitations

CREATE TABLE tenant_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    email CITEXT NOT NULL,
    role TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    invited_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ NULL,
    accepted_user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    revoked_at TIMESTAMPTZ NULL,
    revoked_by UUID NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tenant_invitations_email_format CHECK (position('@' in email::text) > 1),
    CONSTRAINT tenant_invitations_role_check CHECK (
        role IN ('owner', 'admin', 'manager', 'agent', 'viewer')
    ),
    CONSTRAINT tenant_invitations_status_check CHECK (
        status IN ('pending', 'accepted', 'revoked')
    ),
    CONSTRAINT tenant_invitations_accepted_shape CHECK (
        (status = 'accepted') = (accepted_at IS NOT NULL AND accepted_user_id IS NOT NULL)
    ),
    CONSTRAINT tenant_invitations_revoked_shape CHECK (
        (status = 'revoked') = (revoked_at IS NOT NULL AND revoked_by IS NOT NULL)
    )
);

CREATE UNIQUE INDEX tenant_invitations_token_hash_uniq
    ON tenant_invitations (token_hash);

CREATE UNIQUE INDEX tenant_invitations_pending_email_uniq
    ON tenant_invitations (tenant_id, email)
    WHERE status = 'pending';

CREATE INDEX tenant_invitations_tenant_idx
    ON tenant_invitations (tenant_id, status);

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenant_invitations
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
