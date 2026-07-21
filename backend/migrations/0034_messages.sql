-- Migration 0034: Create messages table
--
-- Messages are the append-only log of conversation activity. They support
-- three kinds: customer (from the customer), reply (from an agent), and note
-- (internal agent note not visible to the customer).
--
-- This migration also adds UNIQUE constraints on conversations(tenant_id, id)
-- and tenant_memberships(tenant_id, id) to serve as composite FK targets.

-- 1. UNIQUE constraints required for composite FK targets

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'conversations_tenant_id_id_uq'
    ) THEN
        ALTER TABLE conversations
            ADD CONSTRAINT conversations_tenant_id_id_uq UNIQUE (tenant_id, id);
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'tenant_memberships_tenant_id_id_uq'
    ) THEN
        ALTER TABLE tenant_memberships
            ADD CONSTRAINT tenant_memberships_tenant_id_id_uq UNIQUE (tenant_id, id);
    END IF;
END $$;

-- 2. Messages table (append-only, no updated_at)

CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    conversation_id UUID NOT NULL,
    kind TEXT NOT NULL,
    sender_membership_id UUID NULL,
    logged_by_membership_id UUID NULL,
    body TEXT NOT NULL,
    seq BIGINT GENERATED ALWAYS AS IDENTITY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Composite FK to conversations
    CONSTRAINT messages_conversation_fkey
        FOREIGN KEY (tenant_id, conversation_id)
        REFERENCES conversations (tenant_id, id),

    -- Composite FKs to tenant_memberships
    CONSTRAINT messages_sender_fkey
        FOREIGN KEY (tenant_id, sender_membership_id)
        REFERENCES tenant_memberships (tenant_id, id),

    CONSTRAINT messages_logged_by_fkey
        FOREIGN KEY (tenant_id, logged_by_membership_id)
        REFERENCES tenant_memberships (tenant_id, id),

    CONSTRAINT messages_kind_check CHECK (
        kind IN ('customer', 'reply', 'note')
    ),

    CONSTRAINT messages_body_length CHECK (
        char_length(body) BETWEEN 1 AND 10000
    ),

    CONSTRAINT messages_kind_consistency CHECK (
        (kind = 'customer' AND sender_membership_id IS NULL)
        OR (kind = 'reply' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
        OR (kind = 'note' AND sender_membership_id IS NOT NULL AND logged_by_membership_id IS NULL)
    )
);

-- 3. Timeline index for chronological conversation queries

CREATE INDEX messages_timeline_idx
    ON messages (tenant_id, conversation_id, created_at DESC, seq DESC);
