-- Migration 0051: Customer feedback (spec 024).
--
-- Part 1 fixes a pre-existing defect that blocks this feature: migration 0050
-- added widget conversations, and widgets/public_routes.rs inserts them with
-- channel = 'widget', but 0026's CHECK never listed 'widget', so every widget
-- conversation INSERT violates the constraint.
ALTER TABLE conversations
    DROP CONSTRAINT IF EXISTS conversations_channel_check,
    ADD CONSTRAINT conversations_channel_check
        CHECK (channel IN ('email', 'phone', 'web_chat', 'whatsapp', 'telegram', 'widget'));

-- Part 2: append-only, immutable feedback fact table. No updated_at trigger
-- and no deleted_at: rows are never updated or deleted (FR-012, FR-013).
CREATE TABLE conversation_feedback (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    conversation_id UUID NOT NULL,
    widget_session_id UUID NULL REFERENCES widget_sessions(id) ON DELETE SET NULL,
    channel TEXT NOT NULL,
    agent_configuration_id UUID NULL REFERENCES agent_configurations(id) ON DELETE RESTRICT,
    assigned_membership_id UUID NULL,
    rating SMALLINT NOT NULL,
    comment TEXT NULL,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT conversation_feedback_rating_check
        CHECK (rating BETWEEN 1 AND 5),
    CONSTRAINT conversation_feedback_comment_len_check
        CHECK (comment IS NULL OR char_length(comment) <= 2000),
    CONSTRAINT conversation_feedback_conversation_fkey
        FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT conversation_feedback_membership_fkey
        FOREIGN KEY (tenant_id, assigned_membership_id)
        REFERENCES tenant_memberships (tenant_id, id)
);

-- Duplicate prevention (FR-003): one feedback row per conversation, ever.
CREATE UNIQUE INDEX conversation_feedback_conversation_uq
    ON conversation_feedback (tenant_id, conversation_id);

CREATE INDEX conversation_feedback_tenant_time_idx
    ON conversation_feedback (tenant_id, submitted_at DESC);

CREATE INDEX conversation_feedback_tenant_agent_idx
    ON conversation_feedback (tenant_id, agent_configuration_id)
    WHERE agent_configuration_id IS NOT NULL;

CREATE INDEX conversation_feedback_tenant_member_idx
    ON conversation_feedback (tenant_id, assigned_membership_id)
    WHERE assigned_membership_id IS NOT NULL;
