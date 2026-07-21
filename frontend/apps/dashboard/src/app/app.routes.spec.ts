import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { provideTaiga } from '@taiga-ui/core';
import { provideMockStore } from '@ngrx/store/testing';
import { of } from 'rxjs';
import { environment } from '../environments/environment';
import { APP_CONFIG } from './core/config/app-config';
import { PermissionsService } from './core/authz/permissions.service';
import { CurrentUserService } from './core/tenant/current-user.service';
import { TenantContextService } from './core/tenant/tenant-context.service';
import { NotificationsApiService } from './core/notifications/notifications.api';
import { routes } from './app.routes';

describe('application routes', () => {
  let currentUser: {
    currentUser: ReturnType<typeof vi.fn>;
    isPlatformUser: ReturnType<typeof vi.fn>;
  };
  let permissions: { has: ReturnType<typeof vi.fn>; effective: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    currentUser = {
      currentUser: vi.fn(() => ({ id: 'user-1' })),
      isPlatformUser: vi.fn(() => true),
    };
    permissions = {
      has: vi.fn(() => true),
      effective: vi.fn(() => new Set()),
    };

    TestBed.configureTestingModule({
      providers: [
        provideRouter(routes),
        provideTaiga(),
        provideMockStore({
          initialState: {
            appUi: { themeMode: 'system', sidebarCollapsed: false },
            tenantContext: { activeTenant: null, status: 'idle' as const },
          },
        }),
        { provide: CurrentUserService, useValue: currentUser },
        {
          provide: TenantContextService,
          useValue: { activeTenant: vi.fn(() => ({ id: 'tenant-1' })) },
        },
        { provide: PermissionsService, useValue: permissions },
        provideZonelessChangeDetection(),
        provideHttpClientTesting(),
        { provide: NotificationsApiService, useValue: { unreadCount: () => of({ data: { count: 0 } }) } },
        { provide: APP_CONFIG, useValue: environment },
      ],
    });
  });

  it.each([
    ['/', 'Total conversations'],
    ['/platform/overview-placeholder', 'Platform overview'],
    ['/tenant/overview', 'Total conversations'],
    ['/nope', 'Page not found'],
  ])('renders protected route %s for authenticated users', async (url, expected) => {
    const harness = await RouterTestingHarness.create();
    await harness.navigateByUrl(url);
    await vi.waitFor(() => {
      expect(harness.routeNativeElement?.textContent).toContain(expected);
    });
  });

  it('renders /auth/login for signed-out users', async () => {
    currentUser.currentUser.mockReturnValue(null);

    const harness = await RouterTestingHarness.create();
    await harness.navigateByUrl('/auth/login');

    expect(harness.routeNativeElement?.textContent).toContain('Welcome back');
  });
});
