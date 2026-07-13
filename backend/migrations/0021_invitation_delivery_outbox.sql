CREATE INDEX outbox_invitation_delivery_pending_idx
    ON outbox_events (created_at, id)
    WHERE event_type = 'invitation.email_delivery' AND processed_at IS NULL;
