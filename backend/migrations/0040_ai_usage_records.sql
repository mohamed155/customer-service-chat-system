-- Migration 0040: AI usage records — append-only audit of AI provider calls
--
-- Immutable log: no updated_at or deleted_at. Tenant-scoped for billing/monitoring.

CREATE TABLE ai_usage_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL CHECK (provider IN ('openai','anthropic','gemini')),
    model TEXT NOT NULL,
    input_tokens INTEGER NULL CHECK (input_tokens >= 0),
    output_tokens INTEGER NULL CHECK (output_tokens >= 0),
    status TEXT NOT NULL CHECK (status IN ('success','failure')),
    error_category TEXT NULL CHECK (error_category IN ('authentication','rate_limited','unavailable','timeout','invalid_request')),
    streamed BOOLEAN NOT NULL,
    latency_ms INTEGER NOT NULL,
    request_id TEXT NULL,
    request_content JSONB NULL,
    response_content TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ai_usage_records_tenant_created_idx
    ON ai_usage_records (tenant_id, created_at DESC);
