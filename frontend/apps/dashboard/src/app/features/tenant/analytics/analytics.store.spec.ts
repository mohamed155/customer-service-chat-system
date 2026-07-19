import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { AnalyticsSummary } from '../../../core/api/tenant-api.models';
import { AnalyticsApiService } from './analytics-api.service';
import { AnalyticsStore } from './analytics.store';

const MOCK_SUMMARY: AnalyticsSummary = {
  range: { from: '2026-06-20', to: '2026-07-19' },
  channel: null,
  conversationVolume: 150,
  concludedCount: 120,
  aiResolutionRate: 0.667,
  handoffRate: 0.333,
  avgFirstResponseSeconds: 45,
  avgResponseSeconds: 120,
  satisfactionAvg: 4.2,
  satisfactionCount: 42,
  totalTokens: 50000,
  unattributedTokens: 5000,
  channels: [],
};

const MOCK_TIMESERIES = {
  range: { from: '2026-07-17', to: '2026-07-19' },
  channel: null,
  days: [
    {
      date: '2026-07-17',
      conversationVolume: 50,
      aiResolved: 30,
      handedOff: 20,
      satisfactionAvg: 4.5,
      satisfactionCount: 10,
      totalTokens: 15000,
    },
    {
      date: '2026-07-18',
      conversationVolume: 60,
      aiResolved: 40,
      handedOff: 20,
      satisfactionAvg: null,
      satisfactionCount: 0,
      totalTokens: 20000,
    },
    {
      date: '2026-07-19',
      conversationVolume: 40,
      aiResolved: 25,
      handedOff: 15,
      satisfactionAvg: 4.0,
      satisfactionCount: 8,
      totalTokens: 12000,
    },
  ],
};

describe('AnalyticsStore', () => {
  const getSummary = vi.fn();
  const getTimeseries = vi.fn();

  beforeEach(() => {
    getSummary.mockReset();
    getTimeseries.mockReset();
    TestBed.configureTestingModule({
      providers: [
        AnalyticsStore,
        {
          provide: AnalyticsApiService,
          useValue: { getSummary, getTimeseries },
        },
      ],
    });
  });

  it('initial from/to span exactly 30 inclusive days ending today', () => {
    const store = TestBed.inject(AnalyticsStore);
    const from = new Date(store.from());
    const to = new Date(store.to());
    const diffMs = to.getTime() - from.getTime();
    const diffDays = Math.round(diffMs / (1000 * 60 * 60 * 24));
    expect(diffDays).toBe(29);
    const today = new Date();
    expect(to.getFullYear()).toBe(today.getFullYear());
    expect(to.getMonth()).toBe(today.getMonth());
    expect(to.getDate()).toBe(today.getDate());
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.summary()).toBeNull();
  });

  it('load() calls getSummary and patches summary', async () => {
    const mockSummary: AnalyticsSummary = {
      range: { from: '2026-06-20', to: '2026-07-19' },
      channel: null,
      conversationVolume: 150,
      concludedCount: 120,
      aiResolutionRate: 0.667,
      handoffRate: 0.333,
      avgFirstResponseSeconds: 45,
      avgResponseSeconds: 120,
      satisfactionAvg: 4.2,
      satisfactionCount: 42,
      totalTokens: 50000,
      unattributedTokens: 5000,
      channels: [],
    };
    getSummary.mockReturnValue(of({ data: mockSummary }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    const store = TestBed.inject(AnalyticsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.summary()).toEqual(mockSummary);
      expect(store.loading()).toBe(false);
    });
    expect(getSummary).toHaveBeenCalledWith({
      from: store.from(),
      to: store.to(),
      channel: store.channel(),
    });
  });

  it('API error patches error and leaves loading false', async () => {
    getSummary.mockReturnValue(throwError(() => new Error('boom')));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    const store = TestBed.inject(AnalyticsStore);
    store.load();
    await vi.waitFor(() => {
      expect(store.error()).toBe('boom');
      expect(store.loading()).toBe(false);
    });
  });

  it("setPreset('7') produces 7-day window and calls getSummary", async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    const store = TestBed.inject(AnalyticsStore);
    store.setPreset('7');
    await vi.waitFor(() => {
      expect(store.summary()).toEqual(MOCK_SUMMARY);
      expect(store.loading()).toBe(false);
    });
    const from = new Date(store.from());
    const to = new Date(store.to());
    const diffMs = to.getTime() - from.getTime();
    const diffDays = Math.round(diffMs / (1000 * 60 * 60 * 24));
    expect(diffDays).toBe(6);
    expect(getSummary).toHaveBeenCalledWith({
      from: store.from(),
      to: store.to(),
      channel: store.channel(),
    });
  });

  it('setCustomRange sends provided dates and reloads', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    const store = TestBed.inject(AnalyticsStore);
    store.setCustomRange('2026-03-10', '2026-03-12');
    await vi.waitFor(() => {
      expect(store.summary()).toEqual(MOCK_SUMMARY);
    });
    expect(store.from()).toBe('2026-03-10');
    expect(store.to()).toBe('2026-03-12');
    expect(store.preset()).toBe('custom');
    expect(getSummary).toHaveBeenCalledWith({
      from: '2026-03-10',
      to: '2026-03-12',
      channel: null,
    });
  });

  it('setCustomRange with from > to sets error and does not call API', () => {
    const store = TestBed.inject(AnalyticsStore);
    store.setCustomRange('2026-03-12', '2026-03-10');
    expect(store.error()).toBe('From date must be on or before To date');
    expect(getSummary).not.toHaveBeenCalled();
  });

  it('single setPreset issues exactly one getSummary call', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    const store = TestBed.inject(AnalyticsStore);
    store.setPreset('7');
    await vi.waitFor(() => {
      expect(store.summary()).toEqual(MOCK_SUMMARY);
    });
    expect(getSummary).toHaveBeenCalledTimes(1);
  });

  describe('setChannel', () => {
    it("setChannel('widget') sends channel: 'widget' to both endpoints exactly once each", async () => {
      getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
      getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
      const store = TestBed.inject(AnalyticsStore);
      store.setChannel('widget');
      await vi.waitFor(() => {
        expect(store.summary()).toEqual(MOCK_SUMMARY);
      });
      expect(getSummary).toHaveBeenCalledTimes(1);
      expect(getTimeseries).toHaveBeenCalledTimes(1);
      expect(getSummary).toHaveBeenCalledWith({
        from: store.from(),
        to: store.to(),
        channel: 'widget',
      });
      expect(getTimeseries).toHaveBeenCalledWith({
        from: store.from(),
        to: store.to(),
        channel: 'widget',
      });
    });

    it("setChannel('all') sends channel: null to both endpoints", async () => {
      getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
      getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
      const store = TestBed.inject(AnalyticsStore);
      store.setChannel('all');
      await vi.waitFor(() => {
        expect(store.summary()).toEqual(MOCK_SUMMARY);
      });
      expect(getSummary).toHaveBeenCalledWith({
        from: store.from(),
        to: store.to(),
        channel: null,
      });
      expect(getTimeseries).toHaveBeenCalledWith({
        from: store.from(),
        to: store.to(),
        channel: null,
      });
    });
  });
});
