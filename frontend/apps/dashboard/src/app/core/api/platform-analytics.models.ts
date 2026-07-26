export interface PlatformSummaryWire {
  range: { from: string; to: string };
  tenants: {
    total: number;
    active: number;
    suspended: number;
    created_in_range: number;
  };
  active_conversations: number;
  ai_usage: {
    total_calls: number;
    successful_calls: number;
    failed_calls: number;
    input_tokens: number;
    output_tokens: number;
  };
  generated_at: string;
}

export interface PlatformSummary {
  range: { from: string; to: string };
  tenants: {
    total: number;
    active: number;
    suspended: number;
    createdInRange: number;
  };
  activeConversations: number;
  aiUsage: {
    totalCalls: number;
    successfulCalls: number;
    failedCalls: number;
    inputTokens: number;
    outputTokens: number;
  };
  generatedAt: string;
}

export function platformSummaryFromWire(wire: PlatformSummaryWire): PlatformSummary {
  return {
    range: wire.range,
    tenants: {
      total: wire.tenants.total,
      active: wire.tenants.active,
      suspended: wire.tenants.suspended,
      createdInRange: wire.tenants.created_in_range,
    },
    activeConversations: wire.active_conversations,
    aiUsage: {
      totalCalls: wire.ai_usage.total_calls,
      successfulCalls: wire.ai_usage.successful_calls,
      failedCalls: wire.ai_usage.failed_calls,
      inputTokens: wire.ai_usage.input_tokens,
      outputTokens: wire.ai_usage.output_tokens,
    },
    generatedAt: wire.generated_at,
  };
}

export interface SystemHealthWire {
  overall: 'healthy' | 'degraded' | 'unhealthy';
  dependencies: {
    name: string;
    verdict: 'healthy' | 'degraded' | 'unhealthy';
    reason: string | null;
    probe_ms: number;
    detail: {
      kind: string;
      pending_count?: number;
      oldest_pending_age_seconds?: number | null;
      stuck_retry_count?: number;
      backend?: string;
      s3_configured?: boolean;
    } | null;
  }[];
  read_at: string;
}

export interface SystemHealth {
  overall: 'healthy' | 'degraded' | 'unhealthy';
  dependencies: {
    name: string;
    verdict: 'healthy' | 'degraded' | 'unhealthy';
    reason: string | null;
    probeMs: number;
    detail: {
      kind: string;
      pendingCount?: number;
      oldestPendingAgeSeconds?: number | null;
      stuckRetryCount?: number;
      backend?: string;
      s3Configured?: boolean;
    } | null;
  }[];
  readAt: string;
}

export function systemHealthFromWire(wire: SystemHealthWire): SystemHealth {
  return {
    overall: wire.overall,
    dependencies: wire.dependencies.map((d) => ({
      name: d.name,
      verdict: d.verdict,
      reason: d.reason,
      probeMs: d.probe_ms,
      detail: d.detail
        ? {
            kind: d.detail.kind,
            pendingCount: d.detail.pending_count,
            oldestPendingAgeSeconds: d.detail.oldest_pending_age_seconds,
            stuckRetryCount: d.detail.stuck_retry_count,
            backend: d.detail.backend,
            s3Configured: d.detail.s3_configured,
          }
        : null,
    })),
    readAt: wire.read_at,
  };
}

export interface ProviderStatusWire {
  window: { from: string; to: string; preset: string };
  providers: {
    provider: string;
    verdict: string;
    total_calls: number;
    successes: number;
    failures: number;
    failure_rate: number | null;
    p50_latency_ms: number | null;
    p95_latency_ms: number | null;
    input_tokens: number;
    output_tokens: number;
    failures_by_category: {
      authentication: number;
      rate_limited: number;
      unavailable: number;
      timeout: number;
      invalid_request: number;
    };
  }[];
  recent_failures: {
    provider: string;
    model: string;
    error_category: string | null;
    latency_ms: number;
    occurred_at: string;
  }[];
  generated_at: string;
}

export interface ProviderStatus {
  window: { from: string; to: string; preset: string };
  providers: {
    provider: string;
    verdict: 'healthy' | 'degraded' | 'unhealthy' | 'insufficient_data' | 'no_activity';
    totalCalls: number;
    successes: number;
    failures: number;
    failureRate: number | null;
    p50LatencyMs: number | null;
    p95LatencyMs: number | null;
    inputTokens: number;
    outputTokens: number;
    failuresByCategory: {
      authentication: number;
      rateLimited: number;
      unavailable: number;
      timeout: number;
      invalidRequest: number;
    };
  }[];
  recentFailures: {
    provider: string;
    model: string;
    errorCategory: string | null;
    latencyMs: number;
    occurredAt: string;
  }[];
  generatedAt: string;
}

export function providerStatusFromWire(wire: ProviderStatusWire): ProviderStatus {
  return {
    window: wire.window,
    providers: wire.providers.map((p) => ({
      provider: p.provider,
      verdict: p.verdict as ProviderStatus['providers'][number]['verdict'],
      totalCalls: p.total_calls,
      successes: p.successes,
      failures: p.failures,
      failureRate: p.failure_rate,
      p50LatencyMs: p.p50_latency_ms,
      p95LatencyMs: p.p95_latency_ms,
      inputTokens: p.input_tokens,
      outputTokens: p.output_tokens,
      failuresByCategory: {
        authentication: p.failures_by_category.authentication,
        rateLimited: p.failures_by_category.rate_limited,
        unavailable: p.failures_by_category.unavailable,
        timeout: p.failures_by_category.timeout,
        invalidRequest: p.failures_by_category.invalid_request,
      },
    })),
    recentFailures: wire.recent_failures.map((f) => ({
      provider: f.provider,
      model: f.model,
      errorCategory: f.error_category,
      latencyMs: f.latency_ms,
      occurredAt: f.occurred_at,
    })),
    generatedAt: wire.generated_at,
  };
}
