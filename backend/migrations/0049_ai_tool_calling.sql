-- Migration: AI Tool Calling
-- Adds tenant_tools, tenant_tool_policies, tool_requests tables
-- and extends ai_generations.outcome CHECK constraint.

-- 1. tenant_tools — tenant-defined external tool registrations
CREATE TABLE tenant_tools (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid NOT NULL REFERENCES tenants(id),
    name            text NOT NULL,
    description     text NOT NULL,
    input_schema    jsonb NOT NULL,
    endpoint_url    text NOT NULL,
    credential_ciphertext text,
    classification  text NOT NULL DEFAULT 'approval' CHECK (classification IN ('auto', 'approval')),
    enabled         boolean NOT NULL DEFAULT true,
    created_by_membership_id uuid NOT NULL REFERENCES tenant_memberships(id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    deleted_at      timestamptz
);

CREATE UNIQUE INDEX idx_tenant_tools_live_name
    ON tenant_tools(tenant_id, name) WHERE deleted_at IS NULL;

CREATE INDEX idx_tenant_tools_tenant_live
    ON tenant_tools(tenant_id) WHERE deleted_at IS NULL;

-- 2. tenant_tool_policies — per-tenant policy over built-in tools
CREATE TABLE tenant_tool_policies (
    id                      uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               uuid NOT NULL REFERENCES tenants(id),
    tool_name               text NOT NULL,
    enabled                 boolean NOT NULL DEFAULT false,
    require_approval        boolean NOT NULL DEFAULT false,
    updated_by_membership_id uuid NOT NULL REFERENCES tenant_memberships(id),
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_tenant_tool_policies_tenant_tool
    ON tenant_tool_policies(tenant_id, tool_name);

-- 3. tool_requests — per-request lifecycle record = audit trail
CREATE TABLE tool_requests (
    id                       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                uuid NOT NULL REFERENCES tenants(id),
    conversation_id          uuid NOT NULL REFERENCES conversations(id),
    generation_id            uuid NOT NULL REFERENCES ai_generations(id),
    tool_name                text NOT NULL,
    tool_source              text NOT NULL CHECK (tool_source IN ('builtin', 'tenant')),
    tenant_tool_id           uuid REFERENCES tenant_tools(id),
    arguments                jsonb NOT NULL,
    status                   text NOT NULL CHECK (status IN (
        'pending', 'refused', 'awaiting_approval', 'approved',
        'executing', 'succeeded', 'failed', 'timed_out',
        'denied', 'expired', 'cancelled'
    )),
    approval_required        boolean NOT NULL,
    expires_at               timestamptz,
    decided_by_membership_id uuid REFERENCES tenant_memberships(id),
    decided_at               timestamptz,
    started_at               timestamptz,
    finished_at              timestamptz,
    result                   jsonb,
    error                    text,
    chain_index              smallint NOT NULL,
    created_at               timestamptz NOT NULL DEFAULT now(),
    updated_at               timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chk_never_executed CHECK (
        status NOT IN ('denied', 'expired', 'cancelled') OR started_at IS NULL
    ),
    CONSTRAINT chk_source_tenant_tool_id CHECK (
        (tool_source = 'tenant' AND tenant_tool_id IS NOT NULL)
        OR (tool_source = 'builtin' AND tenant_tool_id IS NULL)
    )
);

CREATE INDEX idx_tool_requests_conversation
    ON tool_requests(tenant_id, conversation_id, created_at DESC);

CREATE INDEX idx_tool_requests_pending_approval
    ON tool_requests(tenant_id, status) WHERE status = 'awaiting_approval';

CREATE INDEX idx_tool_requests_expiry_sweep
    ON tool_requests(status, expires_at) WHERE status = 'awaiting_approval';

CREATE INDEX idx_tool_requests_generation
    ON tool_requests(generation_id);

-- 4. Extend ai_generations.outcome CHECK to include 'awaiting_tool_approval'
ALTER TABLE ai_generations DROP CONSTRAINT IF EXISTS ai_generations_outcome_check;

ALTER TABLE ai_generations ADD CONSTRAINT ai_generations_outcome_check CHECK (
    outcome IN ('success', 'superseded', 'cancelled_escalation', 'failed', 'fallback', 'awaiting_tool_approval')
);
