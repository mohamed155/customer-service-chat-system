import { TestBed } from '@angular/core/testing';
import { provideMockStore, MockStore } from '@ngrx/store/testing';
import { TenantContextService } from './tenant-context.service';
import { ApiService } from '../api/api.service';
import { TenantSummary } from '../api/tenant-api.models';
import { tenantContextActions } from '../state/tenant-context.feature';
import { of, Subject, throwError } from 'rxjs';

const fakeTenant: TenantSummary = {
  id: 't-1',
  name: 'Acme',
  slug: 'acme',
  status: 'active',
  plan: 'trial',
};

const reason = { reason: 'Investigating a customer request', durationMinutes: 30 };
const accessSession = {
  id: 'access-1',
  tenantId: fakeTenant.id,
  reason: reason.reason,
  startedAt: '2026-11-23T10:00:00Z',
  expiresAt: '2026-11-23T10:30:00Z',
};
const wire = { ...fakeTenant, accessSession };

describe('TenantContextService', () => {
  let service: TenantContextService;
  let store: MockStore;
  let api: { post: ReturnType<typeof vi.fn>; get: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    api = {
      post: vi.fn(),
      get: vi.fn().mockReturnValue(of({ data: { ...accessSession, tenantName: fakeTenant.name } })),
    };
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
    api.post.mockReturnValue(of({ data: wire }));

    const result = await service.select('t-1', reason);

    expect(result).toEqual(fakeTenant);
    expect(api.post).toHaveBeenCalledWith('/platform/tenants/t-1/switch', reason);
    expect(api.get).toHaveBeenCalledWith('/platform/access-session');
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

    await expect(service.select('t-1', reason)).rejects.toThrow('Network error');
    expect(dispatchSpy).toHaveBeenCalledWith(
      tenantContextActions.switchTenantRequested({ tenantId: 't-1' }),
    );
    expect(dispatchSpy).toHaveBeenCalledWith(tenantContextActions.switchTenantFailed());
  });

  describe('select$', () => {
    it('is cold: nothing is sent or dispatched until it is subscribed to', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(of({ data: wire }));

      service.select$('t-1', reason);

      expect(api.post).not.toHaveBeenCalled();
      expect(dispatchSpy).not.toHaveBeenCalled();
    });

    it('cancels the request when it is unsubscribed, and says so to the store', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      const pending = new Subject<{ data: TenantSummary }>();
      api.post.mockReturnValue(pending);

      const subscription = service.select$('t-1', reason).subscribe();
      expect(pending.observed).toBe(true);
      subscription.unsubscribe();

      expect(pending.observed).toBe(false);
      expect(dispatchSpy).toHaveBeenCalledWith(tenantContextActions.switchTenantCancelled());
    });

    it('does not report a cancel after an answer', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(of({ data: wire }));

      service.select$('t-1', reason).subscribe();

      expect(dispatchSpy).not.toHaveBeenCalledWith(tenantContextActions.switchTenantCancelled());
    });

    it('does not report a cancel after a failure', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(throwError(() => new Error('down')));

      service.select$('t-1', reason).subscribe({ error: () => undefined });

      expect(dispatchSpy).not.toHaveBeenCalledWith(tenantContextActions.switchTenantCancelled());
    });

    it('emits the tenant and dispatches the same actions as select()', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(of({ data: wire }));
      const seen: TenantSummary[] = [];

      service.select$('t-1', reason).subscribe((tenant) => seen.push(tenant));

      expect(seen).toEqual([fakeTenant]);
      expect(dispatchSpy).toHaveBeenCalledWith(
        tenantContextActions.switchTenantSucceeded({ tenant: fakeTenant }),
      );
    });

    it('preserves the original error and dispatches switchTenantFailed', () => {
      const dispatchSpy = vi.spyOn(store, 'dispatch');
      api.post.mockReturnValue(throwError(() => new Error('Network error')));
      let error: unknown;

      service.select$('t-1', reason).subscribe({ error: (e: unknown) => (error = e) });

      expect((error as Error).message).toBe('Network error');
      expect(dispatchSpy).toHaveBeenCalledWith(tenantContextActions.switchTenantFailed());
    });
  });

  it('loads server access and restores the workspace when no cached summary matches', async () => {
    const dispatch = vi.spyOn(store, 'dispatch');
    api.get.mockReturnValueOnce(of({ data: { ...accessSession, tenantName: fakeTenant.name } }));
    api.get.mockReturnValueOnce(of({ data: fakeTenant }));

    await service.restoreAccess();

    expect(service.accessSession()).toEqual({ ...accessSession, tenantName: fakeTenant.name });
    expect(api.get).toHaveBeenNthCalledWith(2, '/platform/tenants/t-1');
    expect(dispatch).toHaveBeenCalledWith(
      tenantContextActions.setActiveTenant({ tenant: fakeTenant }),
    );
  });

  it('ignores a session lookup that arrives after current access was cleared', async () => {
    const pending = new Subject<{ data: typeof accessSession & { tenantName: string } }>();
    api.get.mockReturnValueOnce(pending);
    const restoring = service.restoreAccess();

    service.clear();
    pending.next({ data: { ...accessSession, tenantName: fakeTenant.name } });
    await restoring;

    expect(service.accessSession()).toBeNull();
    expect(api.get).toHaveBeenCalledTimes(1);
  });

  it('ends access on the server, clears the workspace, and verifies the current session again', async () => {
    await service.restoreAccess(fakeTenant);
    const dispatch = vi.spyOn(store, 'dispatch');
    api.post.mockReturnValueOnce(of({ data: null }));
    api.get.mockReturnValueOnce(throwError(() => ({ status: 404, code: 'not_found' })));

    await service.endAccess();

    expect(api.post).toHaveBeenCalledWith('/platform/access-session/end', undefined);
    expect(service.accessSession()).toBeNull();
    expect(dispatch).toHaveBeenCalledWith(tenantContextActions.clearActiveTenant());
    expect(api.get).toHaveBeenLastCalledWith('/platform/access-session');
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
