import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { ProviderStatus } from '../../../core/api/platform-analytics.models';
import { AiProvidersApiService } from './ai-providers-api.service';
import { AiProvidersStore } from './ai-providers.store';

const MOCK_STATUS: ProviderStatus = {
  window: { from: '2026-07-24T09:14:02Z', to: '2026-07-25T09:14:02Z', preset: '24h' },
  providers: [
    {
      provider: 'openai',
      verdict: 'healthy',
      totalCalls: 100,
      successes: 98,
      failures: 2,
      failureRate: 0.02,
      p50LatencyMs: 820,
      p95LatencyMs: 4100,
      inputTokens: 91002,
      outputTokens: 22140,
      failuresByCategory: {
        authentication: 0,
        rateLimited: 0,
        unavailable: 0,
        timeout: 0,
        invalidRequest: 0,
      },
    },
  ],
  recentFailures: [],
  generatedAt: '2026-07-25T09:14:02Z',
};

describe('AiProvidersStore', () => {
  const getStatus = vi.fn();

  beforeEach(() => {
    getStatus.mockReset();
    TestBed.configureTestingModule({
      providers: [AiProvidersStore, { provide: AiProvidersApiService, useValue: { getStatus } }],
    });
  });

  it('initial window is 24h', () => {
    const store = TestBed.inject(AiProvidersStore);
    expect(store.window()).toBe('24h');
  });

  it('setWindow patches state and reloads', async () => {
    getStatus.mockReturnValue(of({ data: MOCK_STATUS }));
    const store = TestBed.inject(AiProvidersStore);
    store.setWindow('7d');
    await vi.waitFor(() => {
      expect(store.window()).toBe('7d');
      expect(store.status()).toEqual(MOCK_STATUS);
    });
  });

  it('API error sets error and clears loading', async () => {
    getStatus.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(AiProvidersStore);
    store.setWindow('1h');
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });
});
