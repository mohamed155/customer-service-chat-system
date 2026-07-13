CREATE INDEX customers_phone_trgm_idx ON customers USING GIN (phone gin_trgm_ops) WHERE phone IS NOT NULL;

CREATE INDEX customer_channel_identifiers_identifier_trgm_idx ON customer_channel_identifiers USING GIN (identifier gin_trgm_ops);
