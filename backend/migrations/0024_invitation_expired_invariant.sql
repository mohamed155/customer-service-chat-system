-- An invitation may enter the persisted expired state only after its validity window.

ALTER TABLE tenant_invitations
    ADD CONSTRAINT tenant_invitations_expired_shape
        CHECK (status <> 'expired' OR expires_at <= updated_at);
