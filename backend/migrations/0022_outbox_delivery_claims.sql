ALTER TABLE outbox_events
    ADD COLUMN claimed_at TIMESTAMPTZ NULL,
    ADD COLUMN claim_token UUID NULL,
    ADD COLUMN available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN last_error TEXT NULL,
    ADD COLUMN dead_lettered_at TIMESTAMPTZ NULL,
    ADD CONSTRAINT outbox_claim_shape CHECK ((claimed_at IS NULL) = (claim_token IS NULL));

DROP INDEX IF EXISTS outbox_invitation_delivery_pending_idx;

CREATE INDEX outbox_invitation_delivery_claimable_idx
    ON outbox_events (available_at, created_at, id)
    WHERE event_type = 'invitation.email_delivery'
      AND processed_at IS NULL
      AND dead_lettered_at IS NULL;
