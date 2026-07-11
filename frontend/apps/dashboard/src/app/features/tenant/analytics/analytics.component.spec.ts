import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { PagePayload, RoutedPageDataService } from '../routed-page-data.service';
import { AnalyticsComponent } from './analytics.component';

describe('AnalyticsComponent', () => {
  const loadAnalytics = vi.fn();
  const MOCK_ANALYTICS: PagePayload = {
    page: 'analytics',
    data: {
      metrics: [],
      charts: [],
      topArticles: [],
    },
  };

  beforeEach(() => {
    loadAnalytics.mockReset();
    TestBed.configureTestingModule({
      imports: [AnalyticsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadAnalytics } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadAnalytics.mockReturnValue(of(MOCK_ANALYTICS));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });

  it('moves from pending to empty state', async () => {
    loadAnalytics.mockReturnValue(of(null));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from pending to error and retries', async () => {
    loadAnalytics.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(AnalyticsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadAnalytics.mockReturnValue(of(MOCK_ANALYTICS));
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
