import { TestBed } from '@angular/core/testing';
import { ActivatedRouteSnapshot, provideRouter, Router } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { PermissionsService } from '../../core/authz/permissions.service';
import { PLATFORM_ROUTES } from './platform.routes';

describe('PLATFORM_ROUTES', () => {
  let permissions: { has: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    permissions = { has: vi.fn().mockReturnValue(true) };
    TestBed.configureTestingModule({
      providers: [
        provideRouter(PLATFORM_ROUTES),
        provideTaiga(),
        { provide: PermissionsService, useValue: permissions },
      ],
    });
  });

  it('defines a route for the tenant list under platform.tenants', () => {
    const tenantsRoute = PLATFORM_ROUTES.find((r) => r.path === 'tenants');
    expect(tenantsRoute).toBeDefined();
    expect(tenantsRoute?.data?.['pageTitle']).toBe('platformTenants');
    expect(tenantsRoute?.data?.['requiredPermission']).toBe('platform.tenants.list');
  });

  it('defines a parameterized route for tenant details at platform/tenants/:id', () => {
    const detailRoute = PLATFORM_ROUTES.find(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id',
    );
    expect(detailRoute).toBeDefined();
    expect(detailRoute?.path).toBe('tenants/:id');
    expect(detailRoute?.data?.['pageTitle']).toBe('platformTenantDetail');
    expect(detailRoute?.data?.['requiredPermission']).toBe('platform.tenants.list');
  });

  it('defines a route for the new tenant form at platform/tenants/new', () => {
    const newRoute = PLATFORM_ROUTES.find(
      (r) => typeof r.path === 'string' && r.path === 'tenants/new',
    );
    expect(newRoute).toBeDefined();
    expect(newRoute?.data?.['pageTitle']).toBe('platformTenantNew');
    expect(newRoute?.data?.['requiredPermission']).toBe('platform.tenants.list');
  });

  it('defines a route for editing a tenant at platform/tenants/:id/edit', () => {
    const editRoute = PLATFORM_ROUTES.find(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id/edit',
    );
    expect(editRoute).toBeDefined();
    expect(editRoute?.data?.['pageTitle']).toBe('platformTenantDetail');
    expect(editRoute?.data?.['requiredPermission']).toBe('platform.tenants.list');
    expect(editRoute?.canMatch).toBeDefined();
    expect(editRoute?.loadComponent).toBeDefined();
  });

  it('places the edit route before the detail route so the :id param does not swallow /edit', () => {
    const editIndex = PLATFORM_ROUTES.findIndex(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id/edit',
    );
    const detailIndex = PLATFORM_ROUTES.findIndex(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id',
    );
    expect(editIndex).toBeGreaterThanOrEqual(0);
    expect(detailIndex).toBeGreaterThanOrEqual(0);
    expect(editIndex).toBeLessThan(detailIndex);
  });

  it('loads the tenant form component for the edit route', async () => {
    const editRoute = PLATFORM_ROUTES.find(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id/edit',
    );
    const loader = editRoute?.loadComponent as () => Promise<unknown>;
    const loaded = await loader();
    expect(loaded).toBeDefined();
  });

  it('uses permissionGuard on every guarded route', () => {
    for (const route of PLATFORM_ROUTES) {
      if (route.path === '' || route.redirectTo) continue;
      expect(route.canMatch).toBeDefined();
    }
  });

  it('loads the detail component lazily', async () => {
    const detailRoute = PLATFORM_ROUTES.find(
      (r) => typeof r.path === 'string' && r.path === 'tenants/:id',
    );
    expect(detailRoute?.loadComponent).toBeDefined();
    const loader = detailRoute?.loadComponent as () => Promise<unknown>;
    const loaded = await loader();
    expect(loaded).toBeDefined();
  });

  describe('route order (declarations)', () => {
    it('declares tenants/new before tenants/:id so /new is not swallowed by the :id parameter', () => {
      const newIndex = PLATFORM_ROUTES.findIndex(
        (r) => typeof r.path === 'string' && r.path === 'tenants/new',
      );
      const detailIndex = PLATFORM_ROUTES.findIndex(
        (r) => typeof r.path === 'string' && r.path === 'tenants/:id',
      );
      expect(newIndex).toBeGreaterThanOrEqual(0);
      expect(detailIndex).toBeGreaterThanOrEqual(0);
      expect(newIndex).toBeLessThan(detailIndex);
    });
  });

  describe('route resolution under /platform', () => {
    let router: Router;

    beforeEach(() => {
      router = TestBed.inject(Router);
      router.resetConfig([{ path: 'platform', children: PLATFORM_ROUTES }]);
    });

    async function resolveLeafFor(url: string): Promise<ActivatedRouteSnapshot> {
      await router.navigateByUrl(url);
      let route = router.routerState.snapshot.root;
      while (route.firstChild) {
        route = route.firstChild;
      }
      return route;
    }

    async function loadResolvedComponent(url: string): Promise<unknown> {
      const leaf = await resolveLeafFor(url);
      const loader = leaf.routeConfig?.loadComponent as (() => Promise<unknown>) | undefined;
      if (!loader) {
        throw new Error(`No loadComponent on resolved route for ${url}`);
      }
      return loader();
    }

    it('resolves /platform/tenants to TenantListComponent', async () => {
      const component = (await loadResolvedComponent('/platform/tenants')) as { name: string };
      expect(component.name).toContain('TenantListComponent');
    });

    it('resolves /platform/tenants/new to TenantFormComponent in create mode', async () => {
      const leaf = await resolveLeafFor('/platform/tenants/new');
      expect(leaf.routeConfig?.path).toBe('tenants/new');
      expect(leaf.params['id']).toBeUndefined();
      const component = (await loadResolvedComponent('/platform/tenants/new')) as {
        name: string;
      };
      expect(component.name).toContain('TenantFormComponent');
    });

    it('resolves /platform/tenants/abc-123/edit to TenantFormComponent in edit mode', async () => {
      const leaf = await resolveLeafFor('/platform/tenants/abc-123/edit');
      expect(leaf.routeConfig?.path).toBe('tenants/:id/edit');
      expect(leaf.params['id']).toBe('abc-123');
      const component = (await loadResolvedComponent('/platform/tenants/abc-123/edit')) as {
        name: string;
      };
      expect(component.name).toContain('TenantFormComponent');
    });

    it('resolves /platform/tenants/abc-123 to TenantDetailComponent', async () => {
      const component = (await loadResolvedComponent('/platform/tenants/abc-123')) as {
        name: string;
      };
      expect(component.name).toContain('TenantDetailComponent');
    });
  });
});
