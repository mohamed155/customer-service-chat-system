import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { BillingOverview } from '../../../core/api/tenant-api.models';
import { BillingApiService } from './billing-api.service';
import { BillingStore } from './billing.store';

const MOCK_OVERVIEW: BillingOverview = {
  plan: {
    id: 'plan-pro-001',
    name: 'Professional',
    monthlyPriceCents: 29900,
    tokenAllowance: 100_000,
    conversationAllowance: 500,
    isDefault: false,
    status: 'active',
  },
  subscriptionStatus: 'active',
  period: {
    start: new Date('2026-07-01T00:00:00Z'),
    end: new Date('2026-08-01T00:00:00Z'),
  },
  usage: {
    tokens: { used: 40000, allowance: 100_000, percent: 40, state: 'normal' },
    conversations: { used: 100, allowance: 500, percent: 20, state: 'normal' },
  },
  aiPaused: false,
  aiPausedReason: null,
};

describe('BillingStore', () => {
  const getOverview = vi.fn();

  beforeEach(() => {
    getOverview.mockReset();
    getOverview.mockReturnValue(of({ data: MOCK_OVERVIEW }));
    TestBed.configureTestingModule({
      providers: [BillingStore, { provide: BillingApiService, useValue: { getOverview } }],
    });
  });

  it('load() populates overview and clears loading', async () => {
    const store = TestBed.inject(BillingStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.overview()).toEqual(MOCK_OVERVIEW);
      expect(store.loading()).toBe(false);
    });
  });

  it('load() sets error when API fails', async () => {
    getOverview.mockReturnValue(throwError(() => new Error('boom')));
    const store = TestBed.inject(BillingStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.error()).toMatchObject({ code: 'unknown_error' });
      expect(store.loading()).toBe(false);
      expect(store.overview()).toBeNull();
    });
  });
});
