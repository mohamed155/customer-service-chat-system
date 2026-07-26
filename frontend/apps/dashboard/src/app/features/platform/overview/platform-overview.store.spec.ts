import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { PlatformSummary } from '../../../core/api/platform-analytics.models';
import { PlatformOverviewApiService } from './platform-overview-api.service';
import { PlatformOverviewStore } from './platform-overview.store';

const MOCK_SUMMARY: PlatformSummary = {
  range: { from: '2026-06-25', to: '2026-07-25' },
  tenants: { total: 3, active: 2, suspended: 1, createdInRange: 1 },
  activeConversations: 47,
  aiUsage: {
    totalCalls: 12840,
    successfulCalls: 12612,
    failedCalls: 228,
    inputTokens: 4821004,
    outputTokens: 1120553,
  },
  generatedAt: '2026-07-25T09:14:02Z',
};

describe('PlatformOverviewStore', () => {
  const getSummary = vi.fn();

  beforeEach(() => {
    getSummary.mockReset();
    TestBed.configureTestingModule({
      providers: [
        PlatformOverviewStore,
        { provide: PlatformOverviewApiService, useValue: { getSummary } },
      ],
    });
  });

  it('initial state has no summary and not loading', () => {
    const store = TestBed.inject(PlatformOverviewStore);
    expect(store.summary()).toBeNull();
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
  });

  it('load populates summary and clears loading', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    const store = TestBed.inject(PlatformOverviewStore);
    store.load({});
    await vi.waitFor(() => {
      expect(store.summary()).toEqual(MOCK_SUMMARY);
      expect(store.loading()).toBe(false);
    });
  });

  it('API error sets error and clears loading', async () => {
    getSummary.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(PlatformOverviewStore);
    store.load({});
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });
});
