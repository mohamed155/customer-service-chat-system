import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { TenantContextService } from './tenant-context.service';
import { ApiService } from '../api/api.service';
import { TenantSummary } from '../api/tenant-api.models';
import { tenantContextActions } from '../state/tenant-context.feature';
import { of, throwError } from 'rxjs';

const fakeTenant: TenantSummary = {
  id: 't-1',
  name: 'Acme',
  slug: 'acme',
  status: 'active',
  plan: 'trial',
};

describe('TenantContextService', () => {
  let service: TenantContextService;
  let store: MockStore;
  let api: { post: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = { post: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        TenantContextService,
        { provide: ApiService, useValue: api },
        provideMockStore({
          initialState: { tenantContext: { activeTenant: null, status: 'idle' } },
        }),
      ],
    });
    service = TestBed.inject(TenantContextService);
    store = TestBed.inject(MockStore);
  });

  it('dispatches switchTenantRequested and switchTenantSucceeded on successful select', async () => {
    const dispatchSpy = vi.spyOn(store, 'dispatch');
    api.post.mockReturnValue(of({ data: fakeTenant }));

    const result = await service.select('t-1');

    expect(result).toEqual(fakeTenant);
    expect(dispatchSpy).toHaveBeenCalledWith(
      tenantContextActions.switchTenantRequested({ tenantId: 't-1' }),
    );
    expect(dispatchSpy).toHaveBeenCalledWith(
      tenantContextActions.switchTenantSucceeded({ tenant: fakeTenant }),
    );
  });

  it('dispatches switchTenantFailed on API error during select', async () => {
    const dispatchSpy = vi.spyOn(store, 'dispatch');
    api.post.mockReturnValue(throwError(() => new Error('Network error')));

    await expect(service.select('t-1')).rejects.toThrow('Failed to switch tenant');
    expect(dispatchSpy).toHaveBeenCalledWith(
      tenantContextActions.switchTenantRequested({ tenantId: 't-1' }),
    );
    expect(dispatchSpy).toHaveBeenCalledWith(tenantContextActions.switchTenantFailed());
  });

  it('dispatches clearActiveTenant on clear()', () => {
    const dispatchSpy = vi.spyOn(store, 'dispatch');

    service.clear();

    expect(dispatchSpy).toHaveBeenCalledWith(tenantContextActions.clearActiveTenant());
  });

  it('dispatches setActiveTenant on set()', () => {
    const dispatchSpy = vi.spyOn(store, 'dispatch');

    service.set(fakeTenant);

    expect(dispatchSpy).toHaveBeenCalledWith(
      tenantContextActions.setActiveTenant({ tenant: fakeTenant }),
    );
  });
});
