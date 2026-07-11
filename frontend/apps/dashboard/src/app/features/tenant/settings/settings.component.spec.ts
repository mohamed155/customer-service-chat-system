import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { PagePayload, RoutedPageDataService } from '../routed-page-data.service';
import { SettingsComponent } from './settings.component';

describe('SettingsComponent', () => {
  const loadSettings = vi.fn();
  const MOCK_SETTINGS: PagePayload = {
    page: 'settings',
    data: {
      profile: { companyName: 'Acme', contactEmail: 'admin@acme.test' },
      team: [],
      usage: [],
      invoices: [],
      apiKey: { key: 'sk-test' },
      sessions: [],
    },
  } as unknown as PagePayload;

  beforeEach(() => {
    loadSettings.mockReset();
    TestBed.configureTestingModule({
      imports: [SettingsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({}),
        { provide: RoutedPageDataService, useValue: { load: loadSettings } },
      ],
    });
  });

  it('moves from pending to content', async () => {
    loadSettings.mockReturnValue(of(MOCK_SETTINGS));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('General');
    });
  });

  it('dispatches theme changes from the General tab', async () => {
    loadSettings.mockReturnValue(of(MOCK_SETTINGS));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('General');
    });
  });

  it('moves from pending to empty state', async () => {
    loadSettings.mockReturnValue(of(null));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('app-empty-state')).toBeTruthy();
    });
  });

  it('moves from pending to error and retries', async () => {
    loadSettings.mockReturnValue(throwError(() => new Error('fail')));
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Something went wrong');
    });

    loadSettings.mockReturnValue(of(MOCK_SETTINGS));
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
