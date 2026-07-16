-- Migration 0042: extend the messages kind vocabulary with 'ai' (LLM-generated
-- agent reply) and 'system' (platform-authored automatic message, e.g. the
-- unconfigured-tenant auto-acknowledgment). Both carry NULL membership ids,
-- like 'customer'.

ALTER TABLE messages DROP CONSTRAINT messages_kind_check;
ALTER TABLE messages ADD CONSTRAINT messages_kind_check CHECK (
    kind IN ('customer', 'reply', 'note', 'ai', 'system')
);

ALTER TABLE messages DROP CONSTRAINT messages_kind_consistency;
ALTER TABLE messages ADD CONSTRAINT messages_kind_consistency CHECK (
    (kind = 'customer' AND sender_membership_id IS NULL)
    OR (kind = 'reply' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
    OR (kind = 'note' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
    OR (kind IN ('ai', 'system') AND sender_membership_id IS NULL AND logged_by_membership_id IS NULL)
);
