-- Migration 0033: Conversation core — status, assignee FK, inbox/assignee indexes
--
-- 1. Convert escalated→open before CHECK swap
-- 2. Extend status CHECK to include pending/resolved
-- 3. Add assigned_membership_id with composite FK to tenant_memberships
-- 4. Add inbox query index and assignee query index

UPDATE conversations SET status = 'open' WHERE status = 'escalated';

ALTER TABLE conversations
    DROP CONSTRAINT IF EXISTS conversations_status_check,
    ADD CONSTRAINT conversations_status_check
        CHECK (status IN ('open', 'pending', 'resolved', 'closed'));

ALTER TABLE conversations
    ADD COLUMN assigned_membership_id UUID NULL;

-- UNIQUE constraint on (tenant_id, id) is required for composite FK target
DROP INDEX IF EXISTS tenant_memberships_tenant_id_id_uq;
ALTER TABLE tenant_memberships ADD CONSTRAINT tenant_memberships_tenant_id_id_uq UNIQUE (tenant_id, id);

ALTER TABLE conversations
    ADD CONSTRAINT conversations_assignee_tenant_fkey
    FOREIGN KEY (tenant_id, assigned_membership_id)
    REFERENCES tenant_memberships (tenant_id, id)
    NOT VALID;

ALTER TABLE conversations
    VALIDATE CONSTRAINT conversations_assignee_tenant_fkey;

CREATE INDEX conversations_inbox_idx
    ON conversations (tenant_id, status, last_activity_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX conversations_assignee_idx
    ON conversations (tenant_id, assigned_membership_id, last_activity_at DESC)
    WHERE deleted_at IS NULL;
