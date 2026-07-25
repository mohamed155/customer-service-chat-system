export interface MetricUsageFixture {
  readonly used: number;
  readonly allowance: number | null;
  readonly percent: number | null;
  readonly state: string;
}

export interface UsageFixture {
  readonly tokens: MetricUsageFixture;
  readonly conversations: MetricUsageFixture;
}

export interface PlanFixture {
  readonly id: string;
  readonly name: string;
  readonly monthly_price_cents: number;
  readonly token_allowance: number | null;
  readonly conversation_allowance: number | null;
  readonly is_default: boolean;
  readonly status: string;
}

export interface PeriodFixture {
  readonly start: string;
  readonly end: string;
}

export interface BillingOverviewFixture {
  readonly plan: PlanFixture;
  readonly subscription_status: string;
  readonly period: PeriodFixture;
  readonly usage: UsageFixture;
  readonly ai_paused: boolean;
  readonly ai_paused_reason: string | null;
}

export const OVERVIEW_NORMAL: BillingOverviewFixture = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthly_price_cents: 29900,
    token_allowance: 100_000,
    conversation_allowance: 500,
    is_default: false,
    status: 'active',
  },
  subscription_status: 'active',
  period: {
    start: '2026-07-01T00:00:00Z',
    end: '2026-08-01T00:00:00Z',
  },
  usage: {
    tokens: { used: 40000, allowance: 100_000, percent: 40, state: 'normal' },
    conversations: { used: 100, allowance: 500, percent: 20, state: 'normal' },
  },
  ai_paused: false,
  ai_paused_reason: null,
};

export const OVERVIEW_EIGHTYFIVE_PERCENT: BillingOverviewFixture = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthly_price_cents: 29900,
    token_allowance: 100_000,
    conversation_allowance: 500,
    is_default: false,
    status: 'active',
  },
  subscription_status: 'active',
  period: {
    start: '2026-07-01T00:00:00Z',
    end: '2026-08-01T00:00:00Z',
  },
  usage: {
    tokens: { used: 85000, allowance: 100_000, percent: 85, state: 'warning' },
    conversations: { used: 420, allowance: 500, percent: 84, state: 'warning' },
  },
  ai_paused: false,
  ai_paused_reason: null,
};

export const OVERVIEW_EXCEEDED: BillingOverviewFixture = {
  plan: {
    id: 'plan-starter-002',
    name: 'Starter',
    monthly_price_cents: 9900,
    token_allowance: 50_000,
    conversation_allowance: 200,
    is_default: false,
    status: 'active',
  },
  subscription_status: 'active',
  period: {
    start: '2026-07-01T00:00:00Z',
    end: '2026-08-01T00:00:00Z',
  },
  usage: {
    tokens: { used: 50000, allowance: 50_000, percent: 100, state: 'exceeded' },
    conversations: { used: 200, allowance: 200, percent: 100, state: 'exceeded' },
  },
  ai_paused: true,
  ai_paused_reason: 'Token allowance exhausted for this billing period.',
};

export const OVERVIEW_UNLIMITED: BillingOverviewFixture = {
  plan: {
    id: 'plan-enterprise-003',
    name: 'Enterprise',
    monthly_price_cents: 99900,
    token_allowance: null,
    conversation_allowance: null,
    is_default: false,
    status: 'active',
  },
  subscription_status: 'active',
  period: {
    start: '2026-07-01T00:00:00Z',
    end: '2026-08-01T00:00:00Z',
  },
  usage: {
    tokens: { used: 250000, allowance: null, percent: null, state: 'normal' },
    conversations: { used: 1200, allowance: null, percent: null, state: 'normal' },
  },
  ai_paused: false,
  ai_paused_reason: null,
};

export const OVERVIEW_PAST_DUE: BillingOverviewFixture = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthly_price_cents: 29900,
    token_allowance: 100_000,
    conversation_allowance: 500,
    is_default: false,
    status: 'active',
  },
  subscription_status: 'past_due',
  period: {
    start: '2026-07-01T00:00:00Z',
    end: '2026-08-01T00:00:00Z',
  },
  usage: {
    tokens: { used: 30000, allowance: 100_000, percent: 30, state: 'normal' },
    conversations: { used: 80, allowance: 500, percent: 16, state: 'normal' },
  },
  ai_paused: true,
  ai_paused_reason: 'Subscription is past due. Payment required to resume AI service.',
};
