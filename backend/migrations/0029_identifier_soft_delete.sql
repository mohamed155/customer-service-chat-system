ALTER TABLE customer_channel_identifiers ADD COLUMN deleted_at TIMESTAMPTZ NULL;

DROP INDEX customer_channel_identifiers_unique_idx;

CREATE UNIQUE INDEX customer_channel_identifiers_live_unique_idx
    ON customer_channel_identifiers (tenant_id, channel, identifier)
    WHERE deleted_at IS NULL;
