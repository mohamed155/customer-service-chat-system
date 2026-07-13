-- Persist automatic invitation expiry without representing it as admin revocation.

ALTER TABLE tenant_invitations
    DROP CONSTRAINT tenant_invitations_status_check,
    ADD CONSTRAINT tenant_invitations_status_check
        CHECK (status IN ('pending', 'accepted', 'revoked', 'expired'));
