-- Migration 0065: allow the 'widget' channel on customer channel identifiers.
--
-- Same defect 0051 fixed for conversations: widgets/queries.rs creates the
-- anonymous visitor customer with a 'widget' channel identifier, but 0025's
-- CHECK never listed 'widget', so every first widget conversation failed with
-- customer_channel_identifiers_channel_check and the public API returned 500.
ALTER TABLE customer_channel_identifiers
    DROP CONSTRAINT IF EXISTS customer_channel_identifiers_channel_check,
    ADD CONSTRAINT customer_channel_identifiers_channel_check
        CHECK (channel IN ('email', 'phone', 'web_chat', 'whatsapp', 'telegram', 'widget'));
