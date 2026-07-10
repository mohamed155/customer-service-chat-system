import { TestBed } from '@angular/core/testing';
import { provideStore, Store } from '@ngrx/store';
import { of, throwError } from 'rxjs';
import { ApiService } from '../api/api.service';
import { MeResponse } from '../api/tenant-api.models';
import { CurrentUserService } from './current-user.service';

describe('CurrentUserService', () => {
  let service: CurrentUserService;
  let api: { get: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { get: vi.fn() };
    TestBed.configureTestingModule({
      providers: [CurrentUserService, provideStore({}), { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(CurrentUserService);
  });

  it('loads a platform user and exposes isPlatformUser', async () => {
    const me: MeResponse = {
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: 'super_admin',
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    };
    api.get.mockReturnValue(of({ data: me }));

    await service.load();

    expect(service.currentUser()).toEqual(me);
    expect(service.isPlatformUser()).toBe(true);
  });

  it('identifies a non-platform user', async () => {
    const me: MeResponse = {
      id: 'u-2',
      email: 'agent@test.com',
      displayName: 'Agent',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [
        { tenantId: 't1', tenantName: 'T1', tenantSlug: 't1', role: 'agent', permissions: [] },
      ],
    };
    api.get.mockReturnValue(of({ data: me }));

    await service.load();

    expect(service.currentUser()?.displayName).toBe('Agent');
    expect(service.isPlatformUser()).toBe(false);
  });

  it('clears the cached user', async () => {
    const me: MeResponse = {
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: 'super_admin',
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    };
    api.get.mockReturnValue(of({ data: me }));
    await service.load();
    expect(service.currentUser()).not.toBeNull();

    service.clear();

    expect(service.currentUser()).toBeNull();
    expect(service.isPlatformUser()).toBe(false);
  });
  it('resolves unauthenticated load as signed out', async () => {
    api.get.mockReturnValue(
      throwError(() => ({
        code: 'unauthenticated',
        message: 'Authentication required',
        status: 401,
      })),
    );

    await expect(service.load()).resolves.toBeUndefined();

    expect(service.currentUser()).toBeNull();
  });

  it('still rejects non-authentication load failures', async () => {
    const error = { code: 'network_error', message: 'Network request failed', status: 0 };
    api.get.mockReturnValue(throwError(() => error));

    await expect(service.load()).rejects.toBe(error);
  });

  it('clears tenant context when clearing the cached user', async () => {
    const store = TestBed.inject(Store);
    const dispatch = vi.spyOn(store, 'dispatch');

    service.clear();

    expect(dispatch).toHaveBeenCalledWith(
      expect.objectContaining({ type: '[Tenant Context] Clear Active Tenant' }),
    );
  });
});
