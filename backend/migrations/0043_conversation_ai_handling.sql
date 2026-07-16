-- Migration 0043: per-conversation fallback decision while a tenant has no
-- live agent_configurations row. NULL = undecided; ignored entirely once
-- the tenant configures its own agent (see contracts/agent-runtime.md step 1).

ALTER TABLE conversations
    ADD COLUMN ai_handling TEXT NULL CHECK (ai_handling IN ('platform_ai', 'human'));
