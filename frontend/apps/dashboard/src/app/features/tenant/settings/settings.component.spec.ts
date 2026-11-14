import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import {
  PagePayload,
  RoutedPageDataService,
} from '../../../core/routed-page/routed-page-data.service';
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

  it('writes money, months, dates and names for the language, from the data', async () => {
    loadSettings.mockReturnValue(
      of({
        page: 'settings',
        data: {
          profile: { name: 'Acme', domain: 'acme.test', timezone: 'UTC', defaultLanguage: 'en' },
          team: [
            {
              id: 'm1',
              name: 'Dana',
              email: 'dana@acme.test',
              avatarInitials: 'D',
              role: 'owner',
              status: 'invited',
            },
          ],
          usage: [{ label: 'AI conversations', used: 1284, limit: 2000, unit: 'conversations' }],
          invoices: [
            { id: 'i1', periodStart: '2026-07-01', amount: 1240, currency: 'USD', status: 'due' },
            { id: 'i2', periodStart: '2026-06-01', amount: 1188, currency: 'EUR', status: 'paid' },
          ],
          apiKey: { label: 'Key', maskedValue: 'hx_••••', createdAt: '2026-06-12T09:30:00.000Z' },
          sessions: [
            {
              id: 's1',
              device: 'Chrome',
              location: 'Cairo',
              lastActiveAt: '2026-07-07T08:50:00.000Z',
              current: true,
            },
          ],
        },
      } as unknown as PagePayload),
    );
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(SettingsComponent);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const open = async (tab: string, expected: string) => {
      (
        Array.from(el.querySelectorAll('[role="tab"]')).find(
          (b) => b.textContent?.trim() === tab,
        ) as HTMLElement
      ).click();
      fixture.detectChanges();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(el.textContent).toContain(expected);
      });
    };
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Light');
    });
    expect(el.textContent).toContain('Dark');
    expect(el.textContent).toContain('System');

    await open('Team', 'Dana');
    expect(el.textContent).toContain('Owner');
    expect(el.textContent).toContain('Invited');

    await open('Billing', 'July 2026');
    expect(el.textContent).toContain('$1,240');
    expect(el.textContent).toContain('€1,188');
    expect(el.textContent).toContain('June 2026');
    expect(el.textContent).toContain('Due');
    expect(el.textContent).toContain('Paid');
    expect(el.textContent).toContain('1,284 / 2,000 conversations');

    await open('API Keys', 'Created Jun 12, 2026');

    await open('Security', 'Jul 7, 2026');
    expect(el.textContent).toContain('Current');
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
