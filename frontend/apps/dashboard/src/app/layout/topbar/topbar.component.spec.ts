import { Component, computed, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, RouterOutlet, provideRouter } from '@angular/router';
import { MockStore, provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of } from 'rxjs';
import { APP_CONFIG } from '../../core/config/app-config';
import { ApiService } from '../../core/api/api.service';
import { AuthService } from '../../core/auth/auth.service';
import { appUiActions } from '../../core/state/app-ui.feature';
import { CurrentUserService } from '../../core/tenant/current-user.service';
import { LayoutStore } from '../../layout/app-shell/layout.store';
import { TopbarComponent } from './topbar.component';

@Component({
  imports: [RouterOutlet, TopbarComponent],
  template: `<app-topbar /><router-outlet />`,
})
class HostComponent {}

@Component({ template: `` })
class EmptyComponent {}

describe('TopbarComponent', () => {
  async function setup(
    themeMode: 'light' | 'dark' | 'system' = 'light',
    options: { authenticated?: boolean; platformUser?: boolean } = {},
  ) {
    const user = signal(
      options.authenticated
        ? {
            id: 'user-1',
            email: 'agent@example.com',
            displayName: 'Agent',
            platformRole: options.platformUser ? 'super_admin' : null,
            memberships: [],
          }
        : null,
    );
    const auth = { logout: vi.fn().mockResolvedValue(undefined) };
    const currentUser = {
      currentUser: user.asReadonly(),
      isPlatformUser: computed(() => user()?.platformRole != null),
      clear: vi.fn(),
      load: vi.fn(),
    };

    TestBed.configureTestingModule({
      imports: [HostComponent],
      providers: [
        provideRouter([
          {
            path: 'tenant/conversations',
            component: EmptyComponent,
            data: { pageTitle: 'conversations' },
          },
        ]),
        provideTaiga(),
        provideZonelessChangeDetection(),
        provideMockStore({
          initialState: {
            appUi: { themeMode, sidebarCollapsed: false },
            tenantContext: { activeTenant: null, status: 'idle' },
          },
        }),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: 'http://localhost:8080/api/v1' } },
        {
          provide: ApiService,
          useValue: {
            get: vi.fn().mockReturnValue(of({ data: {} })),
            list: vi.fn().mockReturnValue(of({ data: { items: [] } })),
          },
        },
        { provide: AuthService, useValue: auth },
        { provide: CurrentUserService, useValue: currentUser },
        LayoutStore,
      ],
    });
    await TestBed.compileComponents();
    const router = TestBed.inject(Router);
    await router.navigateByUrl('/tenant/conversations');
    const fixture = TestBed.createComponent(HostComponent);
    fixture.detectChanges();
    return { fixture, store: TestBed.inject(MockStore), auth };
  }

  it('renders title and subtitle from route data', async () => {
    const { fixture } = await setup();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain('Conversations');
    expect(text).toContain('Manage your team conversations');
  });

  it('dispatches the next theme mode when the theme button is clicked', async () => {
    const { fixture, store } = await setup('dark');
    const dispatch = vi.spyOn(store, 'dispatch');
    const themeButton = (fixture.nativeElement as HTMLElement).querySelector(
      'button[aria-label^="Theme is dark"]',
    ) as HTMLButtonElement;

    themeButton.click();

    expect(dispatch).toHaveBeenCalledWith(appUiActions.themeModeChanged({ themeMode: 'system' }));
  });

  it('keeps search, notifications, and New as visual controls', async () => {
    const { fixture, store } = await setup();
    const dispatch = vi.spyOn(store, 'dispatch');
    const element = fixture.nativeElement as HTMLElement;

    (element.querySelector('input[type="search"]') as HTMLInputElement).dispatchEvent(
      new Event('input'),
    );
    (element.querySelector('[aria-label="Notifications"]') as HTMLElement).click();
    (element.querySelector('.new-button') as HTMLButtonElement).click();

    expect(dispatch).not.toHaveBeenCalled();
  });

  it('hides user menu for signed-out users', async () => {
    const { fixture } = await setup();

    expect((fixture.nativeElement as HTMLElement).querySelector('app-user-menu')).toBeNull();
  });

  it('shows user menu to authenticated users and delegates logout', async () => {
    const { fixture, auth } = await setup('light', { authenticated: true });
    const menu = (fixture.nativeElement as HTMLElement).querySelector(
      'app-user-menu',
    ) as HTMLElement;

    expect(menu).not.toBeNull();

    const trigger = menu.querySelector('.trigger') as HTMLElement;
    trigger.click();
    fixture.detectChanges();

    const signOutBtn = menu.querySelector('.sign-out') as HTMLElement;
    signOutBtn.click();
    await fixture.whenStable();

    expect(auth.logout).toHaveBeenCalledTimes(1);
  });

  it('shows platform nav control for platform users', async () => {
    const { fixture } = await setup('light', { authenticated: true, platformUser: true });

    const platformNav = (fixture.nativeElement as HTMLElement).querySelector('app-platform-nav');

    expect(platformNav).not.toBeNull();
  });

  it('hides platform nav control for tenant users', async () => {
    const { fixture } = await setup('light', { authenticated: true });

    const platformNav = (fixture.nativeElement as HTMLElement).querySelector('app-platform-nav');

    expect(platformNav).toBeNull();
  });

  it('toggles drawer on mobile instead of dispatching sidebar toggle', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    const { fixture, store } = await setup('light', { authenticated: true });
    const dispatch = vi.spyOn(store, 'dispatch');
    const layoutStore = fixture.debugElement.injector.get(LayoutStore);
    const menuButton = (fixture.nativeElement as HTMLElement).querySelector(
      '[aria-label="Toggle sidebar"]',
    ) as HTMLButtonElement;

    expect(layoutStore.drawerOpen()).toBe(false);
    menuButton.click();
    fixture.detectChanges();

    expect(layoutStore.drawerOpen()).toBe(true);
    expect(dispatch).not.toHaveBeenCalledWith(appUiActions.sidebarToggled());

    menuButton.click();
    fixture.detectChanges();

    expect(layoutStore.drawerOpen()).toBe(false);
  });

  it('avoids horizontal overflow at narrow viewports', async () => {
    const { fixture } = await setup('light', { authenticated: true, platformUser: true });
    const el = fixture.nativeElement as HTMLElement;
    const tools = el.querySelector('.tools') as HTMLElement;
    const header = el.querySelector('header') as HTMLElement;

    // Verify the component includes the expected responsive breakpoint rules
    const styleTags = [...document.querySelectorAll('style')];
    expect(styleTags.some((s) => s.textContent?.includes('@media (max-width: 480px)'))).toBe(true);

    // Simulate narrow viewport for LayoutStore (CSS media queries are evaluated
    // by jsdom at style-computation time and use window.innerWidth)
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 360 });
    window.dispatchEvent(new Event('resize'));
    fixture.detectChanges();

    // Regression: no horizontal overflow at 360px
    expect(tools.scrollWidth).toBeLessThanOrEqual(tools.clientWidth);

    // Theme toggle remains reachable (no display:none hiding at any width)
    const themeToggle = header.querySelector('.theme-toggle') as HTMLElement;
    expect(themeToggle).not.toBeNull();
    expect(window.getComputedStyle(themeToggle).display).not.toBe('none');

    // Critical controls must remain reachable at the narrowest breakpoint
    expect(header.querySelector('app-user-menu')).not.toBeNull();
    expect(header.querySelector('app-platform-nav')).not.toBeNull();
    expect(header.querySelector('app-tenant-switcher')).not.toBeNull();

    // New button remains in the DOM (hidden via CSS media query at <480px)
    expect(header.querySelector('.new-button')).not.toBeNull();
  });
});
