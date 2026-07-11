import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import {
  CHANNEL_BREAKDOWN,
  OVERVIEW_METRICS,
  OVERVIEW_TREND_SERIES,
} from '../../../shared/fixtures/analytics.fixtures';
import { CONVERSATION_FIXTURES } from '../../../shared/fixtures/conversation.fixtures';
import { CUSTOMER_FIXTURES } from '../../../shared/fixtures/customer.fixtures';
import { OVERVIEW_ALERT } from '../../../shared/fixtures/settings.fixtures';
import { PagePayload, RoutedPageDataService } from '../routed-page-data.service';
import { OverviewComponent } from './overview.component';

const MOCK_OVERVIEW: PagePayload = {
  page: 'overview',
  data: {
    alert: OVERVIEW_ALERT,
    metrics: OVERVIEW_METRICS,
    trendSeries: OVERVIEW_TREND_SERIES,
    breakdown: CHANNEL_BREAKDOWN,
    recentConversations: CONVERSATION_FIXTURES.slice(0, 5),
    customers: CUSTOMER_FIXTURES,
  },
};

describe('OverviewComponent', () => {
  const loadOverview = vi.fn();

  beforeEach(() => {
    loadOverview.mockReset();
    TestBed.configureTestingModule({
      imports: [OverviewComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadOverview } },
      ],
    });
  });

  const createLoadedComponent = async () => {
    loadOverview.mockReturnValue(of(MOCK_OVERVIEW));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
    return fixture;
  };

  it('renders five metric cards and the main overview sections', async () => {
    const fixture = await createLoadedComponent();
    const element: HTMLElement = fixture.nativeElement;

    expect(element.querySelectorAll('app-metric-card').length).toBe(5);
    expect(element.textContent).toContain('Conversation trends');
    expect(element.textContent).toContain('Channel mix');
    expect(element.textContent).toContain('Recent activity');
  });

  it('renders content after its page data lifecycle completes', async () => {
    loadOverview.mockReturnValue(of(MOCK_OVERVIEW));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
      expect(fixture.nativeElement.textContent).toContain('Overview');
    });
  });

  it('removes the escalation alert when dismissed', async () => {
    const fixture = await createLoadedComponent();

    expect(fixture.nativeElement.querySelector('app-escalation-banner')).toBeTruthy();
    (
      fixture.nativeElement.querySelector('[aria-label="Dismiss alert"]') as HTMLButtonElement
    ).click();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('app-escalation-banner')).toBeFalsy();
  });

  it('moves from pending to empty state', async () => {
    loadOverview.mockReturnValue(of(null));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
    expect(fixture.nativeElement.textContent).toContain('No data yet');
  });

  it('moves from pending to error and retries', async () => {
    loadOverview.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(OverviewComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadOverview.mockReturnValue(of(MOCK_OVERVIEW));
    const retryBtn = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.trim() === 'Try again')!;
    retryBtn.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });
});
