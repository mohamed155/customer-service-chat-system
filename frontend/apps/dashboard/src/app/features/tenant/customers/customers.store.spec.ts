import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Store } from '@ngrx/store';
import { Subject, of, throwError } from 'rxjs';
import { Customer, TenantSummary } from '../../../core/api/tenant-api.models';
import { CustomersApiService } from './customers-api.service';
import { CustomersStore } from './customers.store';

const customer = (id: string, displayName: string): Customer => ({
  id,
  displayName,
  email: `${id}@example.test`,
  phone: null,
  channels: ['email'],
  createdAt: '2026-07-13T10:00:00Z',
  updatedAt: '2026-07-13T10:00:00Z',
});

const page = (items: Customer[], nextCursor: string | null = null, hasMore = false) => ({
  data: { items, nextCursor, hasMore },
});

describe('CustomersStore', () => {
  let api: { list: ReturnType<typeof vi.fn> };

  function configureStore(activeTenant = signal<TenantSummary | null>(null)) {
    TestBed.configureTestingModule({
      providers: [
        CustomersStore,
        { provide: CustomersApiService, useValue: api },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
      ],
    });

    return { store: TestBed.inject(CustomersStore), activeTenant };
  }

  beforeEach(() => {
    api = { list: vi.fn() };
  });

  it('loads the first page on initialization', () => {
    api.list.mockReturnValue(of(page([customer('customer-1', 'Sara Ali')])));

    const { store } = configureStore();
    TestBed.flushEffects();

    expect(api.list).toHaveBeenCalledWith({ limit: 25 }, undefined);
    expect(store.items()).toEqual([customer('customer-1', 'Sara Ali')]);
    expect(store.nextCursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
  });

  it('appends a continuation page using the stored cursor', () => {
    api.list
      .mockReturnValueOnce(of(page([customer('customer-1', 'Sara Ali')], 'cursor-1', true)))
      .mockReturnValueOnce(of(page([customer('customer-2', 'Tariq Noor')])));

    const { store } = configureStore();
    TestBed.flushEffects();
    store.loadMore();

    expect(api.list).toHaveBeenLastCalledWith({ limit: 25 }, 'cursor-1');
    expect(store.items()).toEqual([
      customer('customer-1', 'Sara Ali'),
      customer('customer-2', 'Tariq Noor'),
    ]);
    expect(store.nextCursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
  });

  it('debounces search, resets pagination, and cancels an obsolete request', () => {
    vi.useFakeTimers();
    try {
      const firstSearch = new Subject<ReturnType<typeof page>>();
      api.list
        .mockReturnValueOnce(of(page([customer('customer-1', 'Sara Ali')], 'cursor-1', true)))
        .mockReturnValueOnce(firstSearch.asObservable())
        .mockReturnValueOnce(of(page([customer('customer-3', 'Sarah Khan')])));

      const { store } = configureStore();
      TestBed.flushEffects();
      store.search('sara');
      vi.advanceTimersByTime(300);
      expect(api.list).toHaveBeenLastCalledWith({ q: 'sara', limit: 25 }, undefined);

      store.search('sarah');
      vi.advanceTimersByTime(300);
      firstSearch.next(page([customer('customer-2', 'Stale Result')]));
      firstSearch.complete();
      TestBed.flushEffects();

      expect(api.list).toHaveBeenLastCalledWith({ q: 'sarah', limit: 25 }, undefined);
      expect(store.query()).toBe('sarah');
      expect(store.items()).toEqual([customer('customer-3', 'Sarah Khan')]);
      expect(store.nextCursor()).toBeNull();
      expect(store.hasMore()).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('clears and reloads the directory when the active tenant changes', () => {
    const tenant1: TenantSummary = {
      id: 'tenant-1',
      name: 'Tenant One',
      slug: 'tenant-one',
      status: 'active',
      plan: 'trial',
    };
    const tenant2: TenantSummary = { ...tenant1, id: 'tenant-2', name: 'Tenant Two' };
    const activeTenant = signal<TenantSummary | null>(tenant1);
    api.list
      .mockReturnValueOnce(
        of(page([customer('customer-1', 'Tenant One Customer')], 'cursor-1', true)),
      )
      .mockReturnValueOnce(of(page([customer('customer-2', 'Tenant Two Customer')])));

    const { store } = configureStore(activeTenant);
    TestBed.flushEffects();
    store.search('one');
    activeTenant.set(tenant2);
    TestBed.flushEffects();

    expect(api.list).toHaveBeenCalledTimes(2);
    expect(api.list).toHaveBeenLastCalledWith({ limit: 25 }, undefined);
    expect(store.query()).toBe('');
    expect(store.items()).toEqual([customer('customer-2', 'Tenant Two Customer')]);
    expect(store.nextCursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
  });

  it('fetches a query re-entered after switching tenants', () => {
    vi.useFakeTimers();
    try {
      const tenant1: TenantSummary = {
        id: 'tenant-1',
        name: 'Tenant One',
        slug: 'tenant-one',
        status: 'active',
        plan: 'trial',
      };
      const tenant2: TenantSummary = { ...tenant1, id: 'tenant-2', name: 'Tenant Two' };
      const activeTenant = signal<TenantSummary | null>(tenant1);
      api.list.mockReturnValue(of(page([])));

      const { store } = configureStore(activeTenant);
      TestBed.flushEffects();
      store.search('sara');
      vi.advanceTimersByTime(300);
      activeTenant.set(tenant2);
      TestBed.flushEffects();
      store.search('sara');
      vi.advanceTimersByTime(300);

      expect(api.list).toHaveBeenCalledTimes(4);
      expect(api.list).toHaveBeenLastCalledWith({ q: 'sara', limit: 25 }, undefined);
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps the current page and retries the failed continuation', () => {
    const appendError = { code: 'network_error', message: 'Connection lost' };
    api.list
      .mockReturnValueOnce(of(page([customer('customer-1', 'Sara Ali')], 'cursor-1', true)))
      .mockReturnValueOnce(throwError(() => appendError))
      .mockReturnValueOnce(of(page([customer('customer-2', 'Tariq Noor')])));

    const { store } = configureStore();
    TestBed.flushEffects();
    store.loadMore();

    expect(store.items()).toEqual([customer('customer-1', 'Sara Ali')]);
    expect(store.nextCursor()).toBe('cursor-1');
    expect(store.hasMore()).toBe(true);
    expect(store.status()).toBe('success');
    expect(store.error()).toBeNull();
    expect(store.loadMoreError()).toBe(appendError);

    store.retry();

    expect(api.list).toHaveBeenLastCalledWith({ limit: 25 }, 'cursor-1');
    expect(store.items()).toEqual([
      customer('customer-1', 'Sara Ali'),
      customer('customer-2', 'Tariq Noor'),
    ]);
    expect(store.loadMoreError()).toBeNull();
  });
});
