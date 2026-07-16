-- Migration 0044: claimable index for the agent responder's outbox event type.
-- Mirrors the shape of 0037's outbox_escalations_claimable_idx so that the
-- responder's claim query can seek rather than sequentially scan outbox_events
-- (which grows with total platform message volume).

CREATE INDEX IF NOT EXISTS outbox_customer_message_claimable_idx
    ON outbox_events (event_type, created_at ASC)
    WHERE event_type = 'conversation.customer_message' AND claimed_at IS NULL;
