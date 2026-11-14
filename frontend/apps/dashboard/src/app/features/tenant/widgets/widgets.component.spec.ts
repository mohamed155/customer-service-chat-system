import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { WidgetInstance } from '../../../core/api/widget.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { provideTenantStore } from '../../../core/state/testing/tenant-store.testing';
import { WidgetApiService } from './widget-api.service';
import { WidgetsComponent } from './widgets.component';

const WIDGET: WidgetInstance = {
  id: 'w1',
  publicId: 'pub_1',
  name: 'Support',
  displayName: 'Support Chat',
  primaryColor: '#0066FF',
  welcomeMessage: 'Hi!',
  position: 'bottom-right',
  theme: 'light',
  enabled: true,
  allowedDomains: ['example.com'],
  createdAt: '2026-07-01T00:00:00Z',
  updatedAt: '2026-07-10T00:00:00Z',
};

describe('WidgetsComponent', () => {
  const list = vi.fn();
  const getSnippet = vi.fn();

  async function render() {
    TestBed.configureTestingModule({
      imports: [WidgetsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideRouter([]),
        provideTenantStore(),
        { provide: WidgetApiService, useValue: { list, getSnippet } },
        { provide: PermissionsService, useValue: { has: () => true } },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(WidgetsComponent);
    fixture.detectChanges();
    return fixture;
  }

  beforeEach(() => {
    list.mockReset();
    getSnippet.mockReset().mockReturnValue(of({ data: { snippet: '<script></script>' } }));
  });

  it('lists a widget with its state and the date written for the language', async () => {
    list.mockReturnValue(of({ data: [WIDGET, { ...WIDGET, id: 'w2', enabled: false }] }));
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Support Chat');
    });
    expect(el.textContent).toContain('Enabled · Updated Jul 10, 2026');
    expect(el.textContent).toContain('Disabled · Updated Jul 10, 2026');
    expect(el.textContent).toContain('New Widget');
  });

  it('words a failed load by its code with the reference, never the API text', async () => {
    list.mockReturnValue(
      throwError(() => ({
        code: 'internal_error',
        message: 'Failed to list widgets',
        status: 500,
        requestId: 'req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f',
      })),
    );
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('The service is temporarily unavailable. Please try again.');
    });
    expect(el.textContent).not.toContain('Failed to list widgets');
    expect(el.querySelector('[data-testid="reference"]')?.textContent).toBe('3c4d5e6f');
  });

  it('names the widget in the delete confirmation', async () => {
    list.mockReturnValue(of({ data: [WIDGET] }));
    const fixture = await render();
    const el = fixture.nativeElement as HTMLElement;
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(el.textContent).toContain('Support Chat');
    });
    (
      Array.from(el.querySelectorAll('button')).find((b) =>
        b.textContent?.includes('Delete'),
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();
    expect(el.querySelector('#delete-title')?.textContent?.trim()).toBe('Delete “Support Chat”');
    expect(el.textContent).toContain('This action cannot be undone.');
  });
});
