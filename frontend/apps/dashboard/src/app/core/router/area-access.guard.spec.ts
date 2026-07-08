import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { areaAccessGuard } from './area-access.guard';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';

describe('areaAccessGuard', () => {
  let currentUser: { isPlatformUser: ReturnType<typeof vi.fn> };
  let tenantContext: { activeTenant: ReturnType<typeof vi.fn> };
  let router: { parseUrl: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    currentUser = { isPlatformUser: vi.fn() };
    tenantContext = { activeTenant: vi.fn() };
    router = { parseUrl: vi.fn().mockReturnValue('redirect') };
    TestBed.configureTestingModule({
      providers: [
        { provide: CurrentUserService, useValue: currentUser },
        { provide: TenantContextService, useValue: tenantContext },
        { provide: Router, useValue: router },
      ],
    });
  });

  describe('platform area', () => {
    it('allows platform users', () => {
      currentUser.isPlatformUser.mockReturnValue(true);
      const result = TestBed.runInInjectionContext(() =>
        areaAccessGuard({ data: { area: 'platform' }, path: '' } as never, [], {} as never),
      );
      expect(result).toBe(true);
    });

    it('redirects non-platform users', () => {
      currentUser.isPlatformUser.mockReturnValue(false);
      const result = TestBed.runInInjectionContext(() =>
        areaAccessGuard({ data: { area: 'platform' }, path: '' } as never, [], {} as never),
      );
      expect(router.parseUrl).toHaveBeenCalledWith('/');
      expect(result).toEqual('redirect');
    });
  });

  describe('tenant area', () => {
    it('allows when tenant is active', () => {
      tenantContext.activeTenant.mockReturnValue({ id: 't-1' });
      const result = TestBed.runInInjectionContext(() =>
        areaAccessGuard({ data: { area: 'tenant' }, path: '' } as never, [], {} as never),
      );
      expect(result).toBe(true);
    });

    it('redirects to tenant selection when no active tenant', () => {
      tenantContext.activeTenant.mockReturnValue(null);
      const result = TestBed.runInInjectionContext(() =>
        areaAccessGuard({ data: { area: 'tenant' }, path: '' } as never, [], {} as never),
      );
      expect(router.parseUrl).toHaveBeenCalledWith('/tenant/select');
      expect(result).toEqual('redirect');
    });
  });

  it('passes through when no area is specified', () => {
    const result = TestBed.runInInjectionContext(() =>
      areaAccessGuard({ path: 'auth' } as never, [], {} as never),
    );
    expect(result).toBe(true);
  });
});
