ALTER TABLE tenant_invitations
    ADD COLUMN email_delivery_status TEXT NOT NULL DEFAULT 'unconfigured',
    ADD CONSTRAINT tenant_invitations_email_delivery_status_check
        CHECK (email_delivery_status IN ('unconfigured', 'queued', 'sent', 'failed'));
