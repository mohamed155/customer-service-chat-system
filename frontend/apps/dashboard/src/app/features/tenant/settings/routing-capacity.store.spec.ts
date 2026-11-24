import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { RoutingCapacityApiService } from '../../../core/api/routing-capacity-api.service';
import { RoutingCapacityStore } from './routing-capacity.store';

describe('RoutingCapacityStore', () => {
  const api = {
    get: vi.fn(),
    setDefault: vi.fn(),
  };
  const permissions = { has: vi.fn() };

  beforeEach(() => {
    api.get.mockReset();
    api.setDefault.mockReset();
    permissions.has.mockReset();
  });

  function createStore() {
    TestBed.configureTestingModule({
      providers: [
        RoutingCapacityStore,
        { provide: RoutingCapacityApiService, useValue: api },
        { provide: PermissionsService, useValue: permissions },
      ],
    });
    return TestBed.inject(RoutingCapacityStore);
  }

  it('loads the tenant default for a team manager', () => {
    permissions.has.mockReturnValue(true);
    api.get.mockReturnValue(of({ data: { default_capacity: 14, agents: [] } }));

    const store = createStore();
    TestBed.flushEffects();

    expect(api.get).toHaveBeenCalledOnce();
    expect(store.defaultCapacity()).toBe(14);
    expect(store.status()).toBe('success');
  });

  it('does not call the capacity endpoint when the user cannot manage members', () => {
    permissions.has.mockReturnValue(false);

    const store = createStore();
    TestBed.flushEffects();

    expect(api.get).not.toHaveBeenCalled();
    expect(store.status()).toBe('pending');
  });

  it('saves the tenant default and keeps the returned value', () => {
    permissions.has.mockReturnValue(true);
    api.get.mockReturnValue(of({ data: { default_capacity: 10, agents: [] } }));
    api.setDefault.mockReturnValue(of({ data: { default_capacity: 20, agents: [] } }));
    const store = createStore();
    TestBed.flushEffects();

    store.saveDefault(20);

    expect(api.setDefault).toHaveBeenCalledWith(20);
    expect(store.defaultCapacity()).toBe(20);
    expect(store.saveStatus()).toBe('success');
  });

  it('keeps an API error for the page to show with its request id', () => {
    permissions.has.mockReturnValue(true);
    api.get.mockReturnValue(
      throwError(() => ({
        status: 503,
        code: 'service_unavailable',
        message: 'Unavailable',
        requestId: 'request-1',
      })),
    );
    const store = createStore();
    TestBed.flushEffects();

    expect(store.status()).toBe('error');
    expect(store.error()?.status).toBe(503);
    expect(store.error()?.requestId).toBe('request-1');
  });
});
