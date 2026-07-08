import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { ApiService } from '../api/api.service';
import { MeResponse } from '../api/tenant-api.models';
import { CurrentUserService } from './current-user.service';

describe('CurrentUserService', () => {
  let service: CurrentUserService;
  let api: { get: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { get: vi.fn() };
    TestBed.configureTestingModule({
      providers: [CurrentUserService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(CurrentUserService);
  });

  it('loads a platform user and exposes isPlatformUser', async () => {
    const me: MeResponse = {
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: 'super_admin',
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
      memberships: [{ tenantId: 't1', tenantName: 'T1', tenantSlug: 't1', role: 'agent' }],
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
      memberships: [],
    };
    api.get.mockReturnValue(of({ data: me }));
    await service.load();
    expect(service.currentUser()).not.toBeNull();

    service.clear();

    expect(service.currentUser()).toBeNull();
    expect(service.isPlatformUser()).toBe(false);
  });
});
