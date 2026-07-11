import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { PagePayload, RoutedPageDataService } from '../routed-page-data.service';
import { IntegrationsComponent } from './integrations.component';

describe('IntegrationsComponent', () => {
  const loadIntegrations = vi.fn();
  const MOCK_INTEGRATIONS: PagePayload = {
    page: 'integrations',
    data: [],
  };

  beforeEach(() => {
    loadIntegrations.mockReset();
    TestBed.configureTestingModule({
      imports: [IntegrationsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: RoutedPageDataService, useValue: { load: loadIntegrations } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadIntegrations.mockReturnValue(of(MOCK_INTEGRATIONS));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-loading-state')).toBeFalsy();
    });
  });

  it('moves from pending to empty state', async () => {
    loadIntegrations.mockReturnValue(of(null));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from pending to error and retries', async () => {
    loadIntegrations.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(IntegrationsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadIntegrations.mockReturnValue(of(MOCK_INTEGRATIONS));
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
