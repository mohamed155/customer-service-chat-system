import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { AnalyticsSummary, AnalyticsTimeseries } from '../../../core/api/tenant-api.models';
import { AnalyticsApiService } from './analytics-api.service';
import { AnalyticsStore } from './analytics.store';
import { AnalyticsComponent } from './analytics.component';

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
  channels: [
    { channel: 'widget', conversationCount: 80, share: 0.533 },
    { channel: 'email', conversationCount: 40, share: 0.267 },
    { channel: 'phone', conversationCount: 30, share: 0.2 },
  ],
};

const NULL_SUMMARY: AnalyticsSummary = {
  ...MOCK_SUMMARY,
  aiResolutionRate: null,
  handoffRate: null,
  avgFirstResponseSeconds: null,
  avgResponseSeconds: null,
  satisfactionAvg: null,
  channels: [],
};

const MOCK_TIMESERIES: AnalyticsTimeseries = {
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

describe('AnalyticsComponent', () => {
  const getSummary = vi.fn();
  const getTimeseries = vi.fn();

  beforeEach(() => {
    getSummary.mockReset();
    getTimeseries.mockReset();
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    TestBed.configureTestingModule({
      imports: [AnalyticsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: AnalyticsApiService,
          useValue: { getSummary, getTimeseries },
        },
      ],
    });
  });

  it('renders seven metric cards when all values present', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelectorAll('app-metric-card').length).toBe(7);
    });
    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Avg first response');
    expect(text).toContain('Avg response');
  });

  it('writes the figures for the language: percentages, durations, ratings and tokens', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.componentRef.injector.get(AnalyticsStore).load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelectorAll('app-metric-card').length).toBe(7);
    });
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('50,000');
    expect(text).toMatch(/\d+\.\d%/);
    expect(text).toContain('/ 5 (42 ratings)');
    expect(text).toMatch(/\d+ s|\d+ m \d+ s/);
  });

  it('names the channels, the widget as the website widget, and shows an unknown one as sent', async () => {
    getSummary.mockReturnValue(
      of({
        data: {
          ...MOCK_SUMMARY,
          channels: [
            { channel: 'widget', conversationCount: 5, share: 0.5 },
            { channel: 'whatsapp', conversationCount: 4, share: 0.4 },
            { channel: 'carrier_pigeon', conversationCount: 1, share: 0.1 },
          ],
        },
      }),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.componentRef.injector.get(AnalyticsStore).load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('Website widget');
      expect(text).toContain('WhatsApp');
      expect(text).toContain('carrier_pigeon');
    });
  });

  it('words a failed load by its code with the request reference, never the API text', async () => {
    getSummary.mockReturnValue(
      throwError(() => ({
        code: 'internal_error',
        message: 'Failed to load analytics',
        status: 500,
        requestId: 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f',
      })),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.componentRef.injector.get(AnalyticsStore).load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const text = fixture.nativeElement.textContent as string;
      expect(text).toContain('The service is temporarily unavailable. Please try again.');
      expect(text).not.toContain('Failed to load analytics');
      expect(fixture.nativeElement.querySelector('[data-testid="reference"]')?.textContent).toBe(
        '3c4d5e6f',
      );
    });
  });

  it('renders em dash for null metrics, not 0%', async () => {
    getSummary.mockReturnValue(of({ data: NULL_SUMMARY }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('\u2014');
    });
    expect(fixture.nativeElement.textContent).not.toContain('0%');
  });

  it('shows error empty state on API error', async () => {
    getSummary.mockReturnValue(throwError(() => new Error('boom')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });
  });

  it('renders four trend-chart elements when timeseries loaded', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelectorAll('app-trend-chart').length).toBe(4);
    });
  });

  it('renders breakdown card with one row per channel entry', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const breakdown = fixture.nativeElement.querySelector('app-breakdown-bars');
      expect(breakdown).toBeTruthy();
      const items = breakdown.querySelectorAll('li');
      expect(items.length).toBe(3);
      expect(items[0].textContent).toContain('Website widget');
      expect(items[0].textContent).toContain('80');
      expect(items[1].textContent).toContain('Email');
      expect(items[1].textContent).toContain('40');
      expect(items[2].textContent).toContain('Phone');
      expect(items[2].textContent).toContain('30');
    });
  });

  it('renders satisfaction null as em dash in chart hidden table, not 0', async () => {
    getSummary.mockReturnValue(of({ data: MOCK_SUMMARY }));
    getTimeseries.mockReturnValue(of({ data: MOCK_TIMESERIES }));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    const store = fixture.componentRef.injector.get(AnalyticsStore);
    store.load();
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      const tables = fixture.nativeElement.querySelectorAll('app-trend-chart table.sr-only');
      expect(tables.length).toBe(4);
      // The satisfaction chart is the 3rd (index 2)
      const satisfactionTable = tables[2];
      const nullRow = satisfactionTable.querySelector('tbody tr:nth-child(2)');
      const nullCell = nullRow.querySelector('td');
      expect(nullCell.textContent?.trim()).toBe('\u2014');
      expect(nullCell.textContent?.trim()).not.toBe('0');
    });
  });
});
