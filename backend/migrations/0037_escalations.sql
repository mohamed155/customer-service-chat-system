-- Migration 0037: Escalations — handoff routing, queue, and presence
--
-- 1. Extends `conversations` with escalated_at flag + composite UNIQUE + indexes
-- 2. Creates `escalations` table (one active escalation per conversation)
-- 3. Adds outbox event type partial index for escalations consumer

-- ===================================================================
-- Part 1: conversations alterations
-- ===================================================================

-- Composite UNIQUE for escalations FK target (mirrors 0033's tenant_memberships_tenant_id_id_uq)
DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'conversations_tenant_id_id_uq'
    ) THEN
        ALTER TABLE conversations ADD CONSTRAINT conversations_tenant_id_id_uq UNIQUE (tenant_id, id);
    END IF;
END $$;

-- Escalated-at flag (cleared when escalation closes)
ALTER TABLE conversations ADD COLUMN escalated_at TIMESTAMPTZ NULL;

-- Load-count index: open/pending conversations by assignee
CREATE INDEX conversations_open_pending_load_idx
    ON conversations (tenant_id, assigned_membership_id)
    WHERE status IN ('open', 'pending') AND deleted_at IS NULL;

-- Escalated inbox index
CREATE INDEX conversations_escalated_inbox_idx
    ON conversations (tenant_id, last_activity_at DESC, id DESC)
    WHERE escalated_at IS NOT NULL AND deleted_at IS NULL;

-- ===================================================================
-- Part 2: escalations table
-- ===================================================================
CREATE TABLE escalations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    conversation_id UUID NOT NULL,
    reason TEXT NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 2000),
    required_skill_ids UUID[] NOT NULL DEFAULT '{}',
    required_skill_names TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('queued', 'assigned', 'closed')),
    routing_reason TEXT NULL CHECK (routing_reason IN (
        'skill_match', 'load_fallback', 'manual_claim', 'queue_auto', 'manual_reassignment'
    )),
    matched_skill_ids UUID[] NOT NULL DEFAULT '{}',
    matched_skill_names TEXT[] NOT NULL DEFAULT '{}',
    assigned_membership_id UUID NULL,
    escalated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    assigned_at TIMESTAMPTZ NULL,
    closed_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id),
    FOREIGN KEY (tenant_id, assigned_membership_id)
        REFERENCES tenant_memberships (tenant_id, id)
);

-- Consistency CHECK: assigned rows must have all assignment fields
ALTER TABLE escalations ADD CONSTRAINT escalations_consistency_check
    CHECK (
        (status = 'assigned' AND assigned_membership_id IS NOT NULL AND routing_reason IS NOT NULL AND assigned_at IS NOT NULL)
        OR (status = 'queued'  AND assigned_membership_id IS NULL AND routing_reason IS NULL AND assigned_at IS NULL)
        OR (status = 'closed')
    );

-- Partial unique index: one active escalation per conversation
CREATE UNIQUE INDEX escalations_one_active_uniq
    ON escalations (tenant_id, conversation_id)
    WHERE status IN ('queued', 'assigned');

-- Queue index: queued escalations ordered by escalated_at
CREATE INDEX escalations_queue_idx
    ON escalations (tenant_id, escalated_at ASC)
    WHERE status = 'queued';

-- Conversation-lookup index: latest escalation per conversation
CREATE INDEX escalations_conversation_lookup_idx
    ON escalations (tenant_id, conversation_id, created_at DESC);

-- Updated-at trigger
CREATE OR REPLACE FUNCTION set_escalations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_escalations_updated_at
    BEFORE UPDATE ON escalations
    FOR EACH ROW
    EXECUTE FUNCTION set_escalations_updated_at();

-- ===================================================================
-- Part 3: Outbox event type partial index for escalations consumer
-- ===================================================================
CREATE INDEX outbox_escalations_claimable_idx
    ON outbox_events (event_type, created_at ASC)
    WHERE event_type IN ('conversation.status_changed', 'conversation.assignment_changed')
      AND claimed_at IS NULL;
