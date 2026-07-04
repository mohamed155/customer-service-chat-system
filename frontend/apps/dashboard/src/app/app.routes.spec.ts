import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { environment } from '../environments/environment';
import { APP_CONFIG } from './core/config/app-config';
import { routes } from './app.routes';

describe('application routes', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      providers: [
        provideRouter(routes),
        provideTaiga(),
        provideMockStore({
          initialState: { appUi: { themeMode: 'system', sidebarCollapsed: false } },
        }),
        provideZonelessChangeDetection(),
        { provide: APP_CONFIG, useValue: environment },
      ],
    }),
  );

  it.each([
    ['/', 'Tenant overview'],
    ['/auth/login-placeholder', 'Sign in'],
    ['/platform/overview-placeholder', 'Platform overview'],
    ['/tenant/overview-placeholder', 'Tenant overview'],
    ['/nope', 'Page not found'],
  ])('renders %s', async (url, expected) => {
    const harness = await RouterTestingHarness.create();
    await harness.navigateByUrl(url);
    expect(harness.routeNativeElement?.textContent).toContain(expected);
  });
});
