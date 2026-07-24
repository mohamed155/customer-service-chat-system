CREATE TABLE billing_plans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    monthly_price_cents INTEGER NOT NULL CHECK (monthly_price_cents >= 0),
    token_allowance BIGINT NULL CHECK (token_allowance >= 0),
    conversation_allowance INTEGER NULL CHECK (conversation_allowance >= 0),
    is_default BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT billing_plans_status_check CHECK (status IN ('active', 'retired'))
);

CREATE UNIQUE INDEX billing_plans_one_default_idx
    ON billing_plans (is_default) WHERE is_default;

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON billing_plans
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TABLE tenant_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE RESTRICT,
    plan_id UUID NOT NULL REFERENCES billing_plans(id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'active',
    assigned_by UUID NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tenant_subscriptions_status_check
        CHECK (status IN ('active', 'past_due', 'canceled'))
);

CREATE TRIGGER set_updated_at
    BEFORE UPDATE ON tenant_subscriptions
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

INSERT INTO billing_plans
    (name, monthly_price_cents, token_allowance, conversation_allowance, is_default, status)
VALUES ('Free', 0, 100000, 100, true, 'active');
