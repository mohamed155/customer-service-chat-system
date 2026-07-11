import { Component, computed, signal } from '@angular/core';
import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router, Routes } from '@angular/router';
import { provideMockStore } from '@ngrx/store/testing';
import { provideTaiga } from '@taiga-ui/core';
import { readFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import { of } from 'rxjs';

import { AppComponent } from './app.component';
import { APP_CONFIG } from './core/config/app-config';
import { environment } from '../environments/environment';
import { ApiService } from './core/api/api.service';
import { AuthService } from './core/auth/auth.service';
import { Permission } from './core/authz/permissions';
import { PermissionsService } from './core/authz/permissions.service';
import { CurrentUserService } from './core/tenant/current-user.service';
import { BreadcrumbComponent } from './layout/breadcrumb/breadcrumb.component';
import { AppShellComponent } from './layout/app-shell/app-shell.component';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

@Component({ template: '', standalone: true })
class EmptyComponent {}

const ALL_PERMISSIONS: Permission[] = [
  'overview.view',
  'conversations.view',
  'conversations.manage',
  'customers.view',
  'customers.manage',
  'ai_agent.view',
  'ai_agent.manage',
  'knowledge_base.view',
  'knowledge_base.manage',
  'integrations.view',
  'integrations.manage',
  'analytics.view',
  'members.view',
  'members.manage',
  'settings.view',
  'settings.manage',
  'billing.view',
  'billing.manage',
  'tenant.delete',
  'owner.assign',
  'platform.tenants.list',
  'platform.tenants.switch',
  'platform.admin',
  'platform.billing.view',
  'platform.diagnostics.view',
];

function setupShellIntegration(allowedPermissions: Permission[]) {
  const userSignal = signal({
    id: 'user-1',
    email: 'admin@example.com',
    displayName: 'Admin User',
    platformRole: null,
    memberships: [
      {
        tenantId: 'tenant-1',
        tenantName: 'Test Tenant',
        tenantSlug: 'test-tenant',
        role: 'admin' as const,
        permissions: allowedPermissions,
      },
    ],
    platformPermissions: [] as Permission[],
    staffTenantPermissions: null,
  });

  const currentUserMock = {
    currentUser: userSignal.asReadonly(),
    isPlatformUser: computed(() => false),
    clear: vi.fn(),
    load: vi.fn(),
  };

  const permissionsMock = {
    has: vi.fn((p: Permission) => allowedPermissions.includes(p)),
    effective: computed(() => new Set(allowedPermissions)),
  };

  const authMock = { logout: vi.fn().mockResolvedValue(undefined) };

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AppShellComponent],
    providers: [
      provideRouter([
        { path: '', redirectTo: '/tenant/overview', pathMatch: 'full' },
        {
          path: 'tenant',
          children: [
            {
              path: 'overview',
              component: EmptyComponent,
              data: { pageTitle: 'overview' },
            },
            {
              path: 'conversations',
              component: EmptyComponent,
              data: { pageTitle: 'conversations' },
            },
            {
              path: 'customers',
              component: EmptyComponent,
              data: { pageTitle: 'customers' },
            },
            {
              path: 'settings',
              data: { pageTitle: 'settings' },
              children: [
                {
                  path: 'billing',
                  component: EmptyComponent,
                  data: { pageTitle: 'aiAgent' },
                },
              ],
            },
          ],
        },
      ]),
      provideTaiga(),
      provideZonelessChangeDetection(),
      provideMockStore({
        initialState: {
          appUi: { themeMode: 'light', sidebarCollapsed: false },
          tenantContext: { activeTenant: null, status: 'idle' },
        },
      }),
      { provide: APP_CONFIG, useValue: environment },
      {
        provide: ApiService,
        useValue: {
          get: vi.fn().mockReturnValue(of({ data: {} })),
          list: vi.fn().mockReturnValue(of({ data: { items: [] } })),
        },
      },
      { provide: AuthService, useValue: authMock },
      { provide: CurrentUserService, useValue: currentUserMock },
      { provide: PermissionsService, useValue: permissionsMock },
    ],
  });
}

describe('Dashboard shell acceptance', () => {
  describe('T001/T002: Pre-bootstrap skeleton markup', () => {
    it('index.html contains skeleton-shell environment', () => {
      const html = readFileSync(resolve(__dirname, '../index.html'), 'utf-8');
      expect(html).toContain('skeleton-shell');
      expect(html).toContain('skeleton-sidebar');
      expect(html).toContain('skeleton-topbar');
      expect(html).toContain('skeleton-content');
      expect(html).toContain('skeleton-brand');
    });

    it('skeleton-shell has correct structural elements', () => {
      const html = readFileSync(resolve(__dirname, '../index.html'), 'utf-8');
      expect(html).toContain('<aside class="skeleton-sidebar">');
      expect(html).toContain('<header class="skeleton-topbar">');
      expect(html).toContain('<main class="skeleton-content">');
      expect(html).toContain('<div class="skeleton-brand">');
      expect(html).toContain('<div class="skeleton-nav">');
    });

    it('skeleton includes dark-theme media query', () => {
      const html = readFileSync(resolve(__dirname, '../index.html'), 'utf-8');
      expect(html).toContain("html[data-theme='dark'] .skeleton-shell");
    });

    it('includes inline theme script before first paint', () => {
      const html = readFileSync(resolve(__dirname, '../index.html'), 'utf-8');
      const scriptMatch = html.match(/<script>([\s\S]*?)<\/script>/);
      expect(scriptMatch).not.toBeNull();
      expect(scriptMatch![1]).toContain("localStorage.getItem('app.themeMode')");
      expect(scriptMatch![1]).toContain('data-theme');
    });
  });

  describe('SC-005/SC-006: Light/dark theme on shell surfaces', () => {
    it('sets data-theme="dark" and tuiTheme="dark" when themeMode is dark', async () => {
      const media = new EventTarget() as MediaQueryList;
      Object.assign(media, {
        matches: false,
        media: '(prefers-color-scheme: dark)',
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
      });
      vi.stubGlobal('matchMedia', vi.fn().mockReturnValue(media));

      await TestBed.configureTestingModule({
        imports: [AppComponent],
        providers: [
          provideRouter([]),
          provideTaiga(),
          provideZonelessChangeDetection(),
          provideMockStore({
            initialState: { appUi: { themeMode: 'dark', sidebarCollapsed: false } },
          }),
        ],
      }).compileComponents();

      const fixture = TestBed.createComponent(AppComponent);
      fixture.detectChanges();

      expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
      expect(fixture.nativeElement.querySelector('tui-root').getAttribute('tuiTheme')).toBe('dark');
    });

    it('shell renders without error when theme is dark', async () => {
      setupShellIntegration(ALL_PERMISSIONS);
      document.documentElement.setAttribute('data-theme', 'dark');
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const element: HTMLElement = fixture.nativeElement;
      expect(element.querySelector('app-sidebar')).toBeTruthy();
      expect(element.querySelector('app-topbar')).toBeTruthy();
      expect(element.querySelector('app-breadcrumb')).toBeTruthy();
      expect(element.querySelector('.shell')).toBeTruthy();
    });
  });

  describe('US1/AC: Role-appropriate navigation visibility', () => {
    it('shows full navigation with all permissions', async () => {
      setupShellIntegration(ALL_PERMISSIONS);
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const sidebar = fixture.nativeElement.querySelector('app-sidebar') as HTMLElement;
      expect(sidebar).toBeTruthy();
      expect(sidebar.querySelectorAll('app-sidebar-nav-group').length).toBe(4);
    });

    it('shows limited navigation for Support Agent (view-only subset)', async () => {
      const supportAgentPermissions: Permission[] = [
        'overview.view',
        'conversations.view',
        'customers.view',
        'knowledge_base.view',
      ];
      setupShellIntegration(supportAgentPermissions);
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const sidebar = fixture.nativeElement.querySelector('app-sidebar') as HTMLElement;
      expect(sidebar.querySelectorAll('app-sidebar-nav-group').length).toBe(2);
      expect(sidebar.textContent).toContain('Overview');
      expect(sidebar.textContent).toContain('Conversations');
      expect(sidebar.textContent).not.toContain('AI Agent');
      expect(sidebar.textContent).not.toContain('Analytics');
      expect(sidebar.textContent).not.toContain('Settings');
    });

    it('shows all view pages except Settings for Viewer', async () => {
      const viewerPermissions: Permission[] = [
        'overview.view',
        'conversations.view',
        'customers.view',
        'ai_agent.view',
        'knowledge_base.view',
        'integrations.view',
        'analytics.view',
      ];
      setupShellIntegration(viewerPermissions);
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const sidebar = fixture.nativeElement.querySelector('app-sidebar') as HTMLElement;
      expect(sidebar.querySelectorAll('app-sidebar-nav-group').length).toBe(3);
      expect(sidebar.textContent).toContain('Analytics');
      expect(sidebar.textContent).not.toContain('Settings');
    });
  });

  describe('FR-007: Breadcrumb ancestor navigation', () => {
    it('renders breadcrumb links with routerLink attributes', async () => {
      setupShellIntegration(ALL_PERMISSIONS);
      await TestBed.compileComponents();
      const router = TestBed.inject(Router);
      await router.navigateByUrl('/tenant/settings/billing');
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const breadcrumbNav = fixture.nativeElement.querySelector('nav[aria-label="Breadcrumb"]');
      expect(breadcrumbNav).not.toBeNull();
      const links: Element[] = Array.from(breadcrumbNav.querySelectorAll('a'));
      expect(links.length).toBeGreaterThanOrEqual(1);
      for (let i = 0; i < links.length; i++) {
        expect(links[i].getAttribute('href')).not.toBeNull();
        expect(links[i].getAttribute('href')).not.toBe('');
      }
      expect(breadcrumbNav.textContent).toContain('Settings');
    });

    it('renders non-navigable current crumb without routerLink', async () => {
      const routes: Routes = [
        {
          path: 'tenant',
          children: [
            {
              path: 'settings',
              component: EmptyComponent,
              data: { pageTitle: 'settings' },
            },
          ],
        },
      ];
      TestBed.resetTestingModule();
      await TestBed.configureTestingModule({
        providers: [provideRouter(routes), provideZonelessChangeDetection()],
      }).compileComponents();
      const router = TestBed.inject(Router);
      await router.navigateByUrl('/tenant/settings');
      const fixture = TestBed.createComponent(BreadcrumbComponent);
      fixture.detectChanges();
      const spans = fixture.nativeElement.querySelectorAll('span');
      const spanArray: Element[] = Array.from(spans);
      const pageSpan = spanArray.find((s: Element) => s.getAttribute('aria-current') === 'page');
      expect(pageSpan).not.toBeUndefined();
      expect(pageSpan!.textContent).toBe('Settings');
    });
  });

  describe('FR-010/SC-004: No horizontal overflow at 360px', () => {
    it('shell container has no horizontal overflow at 360px viewport', async () => {
      Object.defineProperty(window, 'innerWidth', { configurable: true, value: 360 });
      setupShellIntegration(ALL_PERMISSIONS);
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const shell = fixture.nativeElement.querySelector('.shell') as HTMLElement;
      expect(shell.scrollWidth).toBeLessThanOrEqual(shell.clientWidth);
    });

    it('critical controls remain reachable at 360px', async () => {
      Object.defineProperty(window, 'innerWidth', { configurable: true, value: 360 });
      setupShellIntegration(ALL_PERMISSIONS);
      await TestBed.compileComponents();
      const fixture = TestBed.createComponent(AppShellComponent);
      fixture.detectChanges();
      const topbar = fixture.nativeElement.querySelector('app-topbar') as HTMLElement;
      expect(topbar.querySelector('[aria-label="Toggle sidebar"]')).toBeTruthy();
      expect(topbar.querySelector('app-user-menu')).toBeTruthy();
      expect(topbar.querySelector('.new-button')).toBeTruthy();
    });
  });
});
