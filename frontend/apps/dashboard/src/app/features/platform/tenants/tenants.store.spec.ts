import { TestBed } from '@angular/core/testing';
import { firstValueFrom, Subject, of, tap, throwError } from 'rxjs';
import { ApiError, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import {
  CreateTenantPayload,
  PlatformTenantDetail,
  TenantSummary,
  UpdateTenantPayload,
} from '../../../core/api/tenant-api.models';
import { PlatformTenantsService } from './platform-tenants.service';
import { TenantsStore } from './tenants.store';

const tenantSummary = (id: string, name: string): TenantSummary => ({
  id,
  name,
  slug: id,
  status: 'active',
  plan: 'starter',
});

const paginated = (
  items: readonly TenantSummary[],
  nextCursor: string | null = null,
  hasMore = false,
): ApiResponse<PaginatedResponse<TenantSummary>> => ({
  data: { items: [...items], nextCursor, hasMore },
});

const tenantDetail = (id: string, name: string): PlatformTenantDetail => ({
  id,
  name,
  slug: id,
  status: 'active',
  plan: 'starter',
  contactName: null,
  contactEmail: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
});

describe('TenantsStore', () => {
  let service: {
    list: ReturnType<typeof vi.fn>;
    create: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    update: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    service = {
      list: vi.fn(),
      create: vi.fn(),
      get: vi.fn(),
      update: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [TenantsStore, { provide: PlatformTenantsService, useValue: service }],
    });
  });

  it('starts in a pending state with no items, no filters, and no error', () => {
    const store = TestBed.inject(TenantsStore);
    expect(store.status()).toBe('pending');
    expect(store.items()).toEqual([]);
    expect(store.query()).toBe('');
    expect(store.statusFilter()).toBeNull();
    expect(store.nextCursor()).toBeNull();
    expect(store.hasMore()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.loading()).toBe(true);
    expect(store.loadingMore()).toBe(false);
    expect(service.list).not.toHaveBeenCalled();
  });

  it('requests the directory when load() is called', async () => {
    service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

    const store = TestBed.inject(TenantsStore);
    store.load();

    expect(service.list).toHaveBeenCalledTimes(1);
    expect(service.list).toHaveBeenCalledWith({
      q: undefined,
      status: undefined,
      cursor: undefined,
      limit: 25,
    });
    await vi.waitFor(() => {
      expect(store.status()).toBe('success');
      expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]);
    });
  });

  it('marks status as empty when the service returns no items', async () => {
    service.list.mockReturnValue(of(paginated([])));

    const store = TestBed.inject(TenantsStore);
    store.load();

    await vi.waitFor(() => {
      expect(store.status()).toBe('empty');
      expect(store.items()).toEqual([]);
      expect(store.loading()).toBe(false);
    });
  });

  it('captures the error when the service fails', async () => {
    const error: ApiError = { code: 'internal_error', message: 'boom', status: 500 };
    service.list.mockReturnValue(throwError(() => error));

    const store = TestBed.inject(TenantsStore);
    store.load();

    await vi.waitFor(() => {
      expect(store.status()).toBe('error');
      expect(store.error()).toEqual(error);
    });
  });

  it('replaces the items and resets filters when a new load completes', async () => {
    const responses = [
      of(paginated([tenantSummary('t-1', 'Acme')])),
      of(paginated([tenantSummary('t-2', 'Globex'), tenantSummary('t-3', 'Initech')])),
    ];
    service.list.mockImplementation(() => responses.shift() as ReturnType<typeof of>);

    const store = TestBed.inject(TenantsStore);
    store.load();
    await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

    store.load();
    await vi.waitFor(() =>
      expect(store.items()).toEqual([
        tenantSummary('t-2', 'Globex'),
        tenantSummary('t-3', 'Initech'),
      ]),
    );
    expect(service.list).toHaveBeenCalledTimes(2);
  });

  it('replays the load through rxMethod so subjects also drive the state', async () => {
    const subject = new Subject<ApiResponse<PaginatedResponse<TenantSummary>>>();
    service.list.mockReturnValue(subject.asObservable());

    const store = TestBed.inject(TenantsStore);
    store.load();

    expect(store.status()).toBe('pending');

    subject.next(paginated([tenantSummary('t-1', 'Acme')]));
    subject.complete();

    await vi.waitFor(() => {
      expect(store.status()).toBe('success');
      expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]);
    });
  });

  it('returns the new detail and triggers a reload when create() resolves', async () => {
    const detail = tenantDetail('t-9', 'NewCo');
    service.create.mockReturnValue(of({ data: detail }));
    service.list.mockReturnValue(of(paginated([tenantSummary('t-9', 'NewCo')])));

    const store = TestBed.inject(TenantsStore);
    const payload: CreateTenantPayload = { name: 'NewCo', slug: 'newco' };
    const result = await firstValueFrom(store.create(payload));

    expect(result).toEqual(detail);
    expect(service.create).toHaveBeenCalledWith(payload);
    await vi.waitFor(() => {
      expect(service.list).toHaveBeenCalledTimes(1);
      expect(store.items()).toEqual([tenantSummary('t-9', 'NewCo')]);
    });
  });

  it('rethrows the create error without reloading when the service fails', async () => {
    const error: ApiError = { code: 'validation_error', message: 'bad slug', status: 400 };
    service.create.mockReturnValue(throwError(() => error));

    const store = TestBed.inject(TenantsStore);
    await expect(
      firstValueFrom(store.create({ name: 'Bad', slug: 'bad slug' })),
    ).rejects.toMatchObject({
      status: 400,
    });

    expect(service.list).not.toHaveBeenCalled();
    expect(store.status()).toBe('pending');
  });

  describe('update', () => {
    it('returns the new detail and triggers a reload when update() resolves', async () => {
      const detail: PlatformTenantDetail = {
        ...tenantDetail('t-1', 'Acme Inc'),
        name: 'Acme Inc',
      };
      service.update.mockReturnValue(of({ data: detail }));
      service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme Inc')])));

      const store = TestBed.inject(TenantsStore);
      const payload: UpdateTenantPayload = { name: 'Acme Inc' };
      const result = await firstValueFrom(store.update('t-1', payload));

      expect(result).toEqual(detail);
      expect(service.update).toHaveBeenCalledWith('t-1', payload);
      await vi.waitFor(() => {
        expect(service.list).toHaveBeenCalledTimes(1);
        expect(store.items()).toEqual([tenantSummary('t-1', 'Acme Inc')]);
      });
    });

    it('rethrows the update error without reloading when the service fails', async () => {
      const error: ApiError = { code: 'conflict', message: 'slug taken', status: 409 };
      service.update.mockReturnValue(throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      await expect(firstValueFrom(store.update('t-1', { slug: 'taken' }))).rejects.toMatchObject({
        status: 409,
      });

      expect(service.list).not.toHaveBeenCalled();
    });
  });

  describe('getDetail', () => {
    it('returns the unwrapped tenant detail from the service response', async () => {
      const detail = tenantDetail('t-1', 'Acme');
      service.get.mockReturnValue(of({ data: detail }));

      const store = TestBed.inject(TenantsStore);
      const result = await firstValueFrom(store.getDetail('t-1'));

      expect(result).toEqual(detail);
      expect(service.get).toHaveBeenCalledWith('t-1');
    });

    it('propagates errors from the service', async () => {
      const error: ApiError = { code: 'not_found', message: 'missing', status: 404 };
      service.get.mockReturnValue(throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      await expect(firstValueFrom(store.getDetail('t-1'))).rejects.toEqual(error);
    });
  });

  describe('search (setQueryInput)', () => {
    it('forwards the query to the service after the debounce window', async () => {
      vi.useFakeTimers();
      try {
        service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

        const store = TestBed.inject(TenantsStore);
        store.setQueryInput('acme');

        expect(service.list).not.toHaveBeenCalled();

        vi.advanceTimersByTime(300);

        await vi.waitFor(() => {
          expect(service.list).toHaveBeenCalledWith({ q: 'acme', limit: 25 });
        });
        expect(store.query()).toBe('acme');
      } finally {
        vi.useRealTimers();
      }
    });

    it('debounces rapid typing and only issues a single reload for the final value', async () => {
      vi.useFakeTimers();
      try {
        service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

        const store = TestBed.inject(TenantsStore);
        store.setQueryInput('a');
        vi.advanceTimersByTime(100);
        store.setQueryInput('ac');
        vi.advanceTimersByTime(100);
        store.setQueryInput('acm');
        store.setQueryInput('acme');
        vi.advanceTimersByTime(300);

        await vi.waitFor(() => {
          expect(service.list).toHaveBeenCalledTimes(1);
        });
        expect(service.list).toHaveBeenCalledWith({ q: 'acme', limit: 25 });
      } finally {
        vi.useRealTimers();
      }
    });

    it('drops duplicate consecutive inputs via distinctUntilChanged', async () => {
      vi.useFakeTimers();
      try {
        service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

        const store = TestBed.inject(TenantsStore);
        store.setQueryInput('acme');
        vi.advanceTimersByTime(400);
        await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(1));

        store.setQueryInput('acme');
        vi.advanceTimersByTime(400);

        expect(service.list).toHaveBeenCalledTimes(1);
      } finally {
        vi.useRealTimers();
      }
    });

    it('resets accumulated items and cursor when a new search starts', async () => {
      vi.useFakeTimers();
      try {
        const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
        const page1Search = of(paginated([tenantSummary('t-7', 'AcmeSearch')]));
        service.list.mockImplementationOnce(() => page1).mockImplementationOnce(() => page1Search);

        const store = TestBed.inject(TenantsStore);
        store.load();
        await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));
        expect(store.hasMore()).toBe(true);
        expect(store.nextCursor()).toBe('cursor-1');

        store.setQueryInput('acme');
        vi.advanceTimersByTime(300);
        await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-7', 'AcmeSearch')]));
        expect(store.hasMore()).toBe(false);
        expect(store.nextCursor()).toBeNull();
      } finally {
        vi.useRealTimers();
      }
    });

    it('T118/T139: type-then-immediate-clear invalidates pending debounced search and issues exactly one unfiltered request', async () => {
      vi.useFakeTimers();
      try {
        service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

        const store = TestBed.inject(TenantsStore);
        store.load(); // initial load
        await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(1));

        store.setQueryInput('acme'); // triggers debounced reload
        expect(service.list).toHaveBeenCalledTimes(1); // not called yet

        store.resetFilters(); // resets before debounce fires — should invalidate

        vi.advanceTimersByTime(500); // past the debounce window

        // Only the reset's reload (one call), not the canceled search
        expect(service.list).toHaveBeenCalledTimes(2); // initial load + reset

        // Query should be blank after reset
        expect(store.query()).toBe('');

        // The final request is unfiltered — no q, no status
        expect(service.list).toHaveBeenLastCalledWith({ limit: 25 });

        // No additional request after the debounce interval
        vi.advanceTimersByTime(500);
        expect(service.list).toHaveBeenCalledTimes(2);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('status filter (setStatusFilter)', () => {
    it('forwards the status to the service and reloads the directory', async () => {
      service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

      const store = TestBed.inject(TenantsStore);
      store.setStatusFilter('suspended');

      await vi.waitFor(() => {
        expect(service.list).toHaveBeenCalledWith({ status: 'suspended', limit: 25 });
      });
      expect(store.statusFilter()).toBe('suspended');
    });

    it('treats null filter as "all statuses" and omits the status param', async () => {
      service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

      const store = TestBed.inject(TenantsStore);
      store.setStatusFilter('active');
      await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(1));

      service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));
      store.setStatusFilter(null);
      await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(2));
      expect(service.list).toHaveBeenLastCalledWith({ limit: 25 });
      expect(store.statusFilter()).toBeNull();
    });

    it('resets accumulated items and cursor when the filter changes', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const page1Filtered = of(paginated([tenantSummary('t-2', 'Globex')]));
      service.list.mockImplementationOnce(() => page1).mockImplementationOnce(() => page1Filtered);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));
      expect(store.hasMore()).toBe(true);

      store.setStatusFilter('suspended');
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-2', 'Globex')]));
      expect(store.hasMore()).toBe(false);
      expect(store.nextCursor()).toBeNull();
    });
  });

  describe('resetFilters (T064)', () => {
    it('clears query, statusFilter, items, cursor, and hasMore in a single reload', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      service.list.mockImplementationOnce(() => page1);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));
      expect(store.hasMore()).toBe(true);
      expect(store.nextCursor()).toBe('cursor-1');

      const resetPage = of(paginated([tenantSummary('t-2', 'Globex')]));
      service.list.mockImplementationOnce(() => resetPage);

      store.resetFilters();

      await vi.waitFor(() => {
        expect(service.list).toHaveBeenCalledTimes(2);
      });
      expect(service.list).toHaveBeenLastCalledWith({ limit: 25 });
      expect(store.query()).toBe('');
      expect(store.statusFilter()).toBeNull();
      expect(store.items()).toEqual([tenantSummary('t-2', 'Globex')]);
      expect(store.nextCursor()).toBeNull();
      expect(store.hasMore()).toBe(false);
      expect(store.status()).toBe('success');
    });

    it('triggers exactly one list call from a non-default state, not two', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const filterPage = of(paginated([tenantSummary('t-2', 'Globex')], 'cursor-2', true));
      service.list.mockImplementationOnce(() => page1).mockImplementationOnce(() => filterPage);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));
      store.setStatusFilter('suspended');
      await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(2));
      expect(store.statusFilter()).toBe('suspended');

      service.list.mockReturnValue(of(paginated([tenantSummary('t-3', 'Initech')])));

      const callsBeforeReset = service.list.mock.calls.length;
      store.resetFilters();
      await vi.waitFor(() => {
        expect(service.list).toHaveBeenCalledTimes(callsBeforeReset + 1);
      });
      const resetCalls = service.list.mock.calls.length - callsBeforeReset;
      expect(resetCalls).toBe(1);
    });
  });

  describe('loadMore', () => {
    it('appends the next page to the existing items using the stored cursor', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const page2 = of(paginated([tenantSummary('t-2', 'Globex')], 'cursor-2', true));
      const page3 = of(paginated([tenantSummary('t-3', 'Initech')], null, false));
      service.list
        .mockImplementationOnce(() => page1)
        .mockImplementationOnce(() => page2)
        .mockImplementationOnce(() => page3);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() =>
        expect(store.items()).toEqual([
          tenantSummary('t-1', 'Acme'),
          tenantSummary('t-2', 'Globex'),
        ]),
      );
      expect(service.list).toHaveBeenNthCalledWith(2, {
        cursor: 'cursor-1',
        limit: 25,
      });

      store.loadMore();
      await vi.waitFor(() =>
        expect(store.items()).toEqual([
          tenantSummary('t-1', 'Acme'),
          tenantSummary('t-2', 'Globex'),
          tenantSummary('t-3', 'Initech'),
        ]),
      );
      expect(store.hasMore()).toBe(false);
      expect(store.nextCursor()).toBeNull();
    });

    it('is a no-op when there is no next cursor', async () => {
      service.list.mockReturnValue(of(paginated([tenantSummary('t-1', 'Acme')])));

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));
      expect(store.hasMore()).toBe(false);

      store.loadMore();

      expect(service.list).toHaveBeenCalledTimes(1);
    });

    it('transitions to loadingMore while the next page is in flight and back to success on completion', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const subject = new Subject<ApiResponse<PaginatedResponse<TenantSummary>>>();
      service.list.mockImplementationOnce(() => page1).mockImplementationOnce(() => subject);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() => expect(store.status()).toBe('loadingMore'));
      expect(store.loadingMore()).toBe(true);

      subject.next(paginated([tenantSummary('t-2', 'Globex')], null, false));
      subject.complete();
      await vi.waitFor(() => expect(store.status()).toBe('success'));
      expect(store.loadingMore()).toBe(false);
    });

    it('captures the error and stops loadingMore when the next page request fails', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const error: ApiError = { code: 'internal_error', message: 'boom', status: 500 };
      service.list
        .mockImplementationOnce(() => page1)
        .mockImplementationOnce(() => throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() => {
        expect(store.status()).toBe('success');
        expect(store.loadMoreError()).toEqual(error);
      });
      expect(store.loadingMore()).toBe(false);
      expect(store.error()).toBeNull();
    });

    it('preserves existing items when loadMore fails', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const error: ApiError = { code: 'internal_error', message: 'boom', status: 500 };
      service.list
        .mockImplementationOnce(() => page1)
        .mockImplementationOnce(() => throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() => {
        expect(store.status()).toBe('success');
        expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]);
        expect(store.loadMoreError()).toEqual(error);
        expect(store.loadingMore()).toBe(false);
      });
    });

    it('clears loadMoreError on a new load', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const page2 = of(paginated([tenantSummary('t-2', 'Globex')]));
      const error: ApiError = { code: 'internal_error', message: 'boom', status: 500 };
      service.list
        .mockImplementationOnce(() => page1)
        .mockImplementationOnce(() => throwError(() => error))
        .mockImplementationOnce(() => page2);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() => expect(store.loadMoreError()).toEqual(error));

      store.load();
      await vi.waitFor(() => {
        expect(store.loadMoreError()).toBeNull();
        expect(store.items()).toEqual([tenantSummary('t-2', 'Globex')]);
      });
    });

    it('clears loadMoreError on a new successful loadMore', async () => {
      const page1 = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
      const page2 = of(paginated([tenantSummary('t-2', 'Globex')], null, false));
      const error: ApiError = { code: 'internal_error', message: 'boom', status: 500 };
      service.list
        .mockImplementationOnce(() => page1)
        .mockImplementationOnce(() => throwError(() => error))
        .mockImplementationOnce(() => page2);

      const store = TestBed.inject(TenantsStore);
      store.load();
      await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

      store.loadMore();
      await vi.waitFor(() => expect(store.loadMoreError()).toEqual(error));

      store.loadMore();
      await vi.waitFor(() => {
        expect(store.loadMoreError()).toBeNull();
        expect(store.items()).toEqual([
          tenantSummary('t-1', 'Acme'),
          tenantSummary('t-2', 'Globex'),
        ]);
      });
    });

    it('T120/T137: preserves query, status filter, and cursor on loadMore failure; retry sends identical params', async () => {
      vi.useFakeTimers();
      try {
        const initialPage = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-init', true));
        const filteredPage = of(paginated([tenantSummary('t-7', 'AcmeSearch')], 'cursor-1', true));
        const error: ApiError = { code: 'internal_error', message: 'Network failure', status: 500 };
        const retryPage = of(paginated([tenantSummary('t-8', 'AnotherResult')], null, false));
        service.list
          .mockImplementationOnce(() => initialPage)
          .mockImplementationOnce(() => filteredPage)
          .mockImplementationOnce(() => throwError(() => error))
          .mockImplementationOnce(() => retryPage);

        const store = TestBed.inject(TenantsStore);
        store.load();
        await vi.waitFor(() => expect(store.items()).toEqual([tenantSummary('t-1', 'Acme')]));

        // Type a search query to get a filtered cursor
        store.setQueryInput('acme');
        vi.advanceTimersByTime(300);
        await vi.waitFor(() => {
          expect(store.items()).toEqual([tenantSummary('t-7', 'AcmeSearch')]);
        });
        expect(store.query()).toBe('acme');
        expect(store.nextCursor()).toBe('cursor-1');
        expect(store.hasMore()).toBe(true);

        // loadMore fails — cursor, query, items must stay intact
        store.loadMore();
        await vi.waitFor(() => {
          expect(store.status()).toBe('success');
          expect(store.loadMoreError()).toEqual(error);
        });

        expect(store.query()).toBe('acme');
        expect(store.nextCursor()).toBe('cursor-1');
        expect(store.items()).toEqual([tenantSummary('t-7', 'AcmeSearch')]);

        // Retry sends identical cursor + query params
        store.loadMore();
        await vi.waitFor(() => {
          expect(store.items()).toEqual([
            tenantSummary('t-7', 'AcmeSearch'),
            tenantSummary('t-8', 'AnotherResult'),
          ]);
          expect(store.loadMoreError()).toBeNull();
        });
        expect(service.list).toHaveBeenLastCalledWith({
          q: 'acme',
          cursor: 'cursor-1',
          limit: 25,
        });
      } finally {
        vi.useRealTimers();
      }
    });

    it('preserves the current search and status filters when loading more', async () => {
      vi.useFakeTimers();
      try {
        const searchResults = of(paginated([tenantSummary('t-1', 'Acme')], 'cursor-1', true));
        const filterResults = of(paginated([tenantSummary('t-2', 'Globex')], 'cursor-2', true));
        const nextPage = of(paginated([tenantSummary('t-3', 'Initech')], null, false));
        service.list
          .mockImplementationOnce(() => searchResults)
          .mockImplementationOnce(() => filterResults)
          .mockImplementationOnce(() => nextPage);

        const store = TestBed.inject(TenantsStore);
        store.setQueryInput('a');
        vi.advanceTimersByTime(300);
        store.setStatusFilter('active');
        await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(2));
        expect(store.nextCursor()).toBe('cursor-2');

        store.loadMore();
        await vi.waitFor(() => expect(service.list).toHaveBeenCalledTimes(3));
        expect(service.list).toHaveBeenLastCalledWith({
          q: 'a',
          status: 'active',
          cursor: 'cursor-2',
          limit: 25,
        });
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('write pipelines (T063 ordered write contract)', () => {
    it('create fires exactly one HTTP request per call', async () => {
      const detail = tenantDetail('t-9', 'NewCo');
      service.create.mockReturnValue(of({ data: detail }));
      service.list.mockReturnValue(of(paginated([])));

      const store = TestBed.inject(TenantsStore);
      const result = await firstValueFrom(store.create({ name: 'NewCo', slug: 'newco' }));

      expect(service.create).toHaveBeenCalledTimes(1);
      expect(service.create).toHaveBeenCalledWith({ name: 'NewCo', slug: 'newco' });
      expect(result).toEqual(detail);
    });

    it('update fires exactly one HTTP request per call', async () => {
      const detail = tenantDetail('t-1', 'Acme Inc');
      service.update.mockReturnValue(of({ data: detail }));
      service.list.mockReturnValue(of(paginated([])));

      const store = TestBed.inject(TenantsStore);
      const result = await firstValueFrom(store.update('t-1', { name: 'Acme Inc' }));

      expect(service.update).toHaveBeenCalledTimes(1);
      expect(service.update).toHaveBeenCalledWith('t-1', { name: 'Acme Inc' });
      expect(result).toEqual(detail);
    });

    it("concurrent creates are ordered: first call's result emits before second call's", async () => {
      const firstSubject = new Subject<ApiResponse<PlatformTenantDetail>>();
      const secondSubject = new Subject<ApiResponse<PlatformTenantDetail>>();
      const firstDetail = tenantDetail('t-1', 'First');
      const secondDetail = tenantDetail('t-2', 'Second');
      service.create
        .mockImplementationOnce(() => firstSubject.asObservable())
        .mockImplementationOnce(() => secondSubject.asObservable());
      service.list.mockReturnValue(of(paginated([])));

      const store = TestBed.inject(TenantsStore);

      const order: string[] = [];
      const firstResult = firstValueFrom(
        store
          .create({ name: 'First', slug: 'first' })
          .pipe(tap((d) => order.push(`first:${d.id}`))),
      );
      const secondResult = firstValueFrom(
        store
          .create({ name: 'Second', slug: 'second' })
          .pipe(tap((d) => order.push(`second:${d.id}`))),
      );

      await Promise.resolve();

      expect(service.create).toHaveBeenCalledTimes(1);
      expect(order).toEqual([]);

      firstSubject.next({ data: firstDetail });
      firstSubject.complete();

      const first = await firstResult;
      expect(first.id).toBe('t-1');
      expect(order).toEqual(['first:t-1']);

      secondSubject.next({ data: secondDetail });
      secondSubject.complete();

      const second = await secondResult;
      expect(second.id).toBe('t-2');
      expect(order).toEqual(['first:t-1', 'second:t-2']);
      expect(service.create).toHaveBeenCalledTimes(2);
    });

    it('create error propagates to the caller', async () => {
      const error: ApiError = { code: 'validation_error', message: 'bad slug', status: 400 };
      service.create.mockReturnValue(throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      await expect(firstValueFrom(store.create({ name: 'Bad', slug: 'bad slug' }))).rejects.toEqual(
        error,
      );
    });

    it('update error propagates to the caller', async () => {
      const error: ApiError = { code: 'conflict', message: 'slug taken', status: 409 };
      service.update.mockReturnValue(throwError(() => error));

      const store = TestBed.inject(TenantsStore);
      await expect(firstValueFrom(store.update('t-1', { slug: 'taken' }))).rejects.toEqual(error);
    });

    it('unsubscribe prevents further emissions to the caller', () => {
      const subject = new Subject<ApiResponse<PlatformTenantDetail>>();
      service.create.mockReturnValue(subject.asObservable());
      service.list.mockReturnValue(of(paginated([])));

      const store = TestBed.inject(TenantsStore);

      let nextCount = 0;
      let errorReceived: ApiError | null = null;
      let completed = false;

      const sub = store.create({ name: 'X', slug: 'x' }).subscribe({
        next: () => {
          nextCount++;
        },
        error: (err: ApiError) => {
          errorReceived = err;
        },
        complete: () => {
          completed = true;
        },
      });

      sub.unsubscribe();

      subject.next({ data: tenantDetail('t-1', 'X') });
      subject.complete();

      expect(nextCount).toBe(0);
      expect(errorReceived).toBeNull();
      expect(completed).toBe(false);
    });

    it('successful create triggers a list reload as a side effect', async () => {
      const detail = tenantDetail('t-9', 'NewCo');
      service.create.mockReturnValue(of({ data: detail }));
      service.list.mockReturnValue(of(paginated([tenantSummary('t-9', 'NewCo')])));

      const store = TestBed.inject(TenantsStore);
      const result = await firstValueFrom(store.create({ name: 'NewCo', slug: 'newco' }));

      expect(result).toEqual(detail);
      await vi.waitFor(() => {
        expect(service.list).toHaveBeenCalledTimes(1);
        expect(store.items()).toEqual([tenantSummary('t-9', 'NewCo')]);
      });
    });
  });

  describe('T107 serialized rapid status updates', () => {
    it('processes three sequential status updates in order with exactly one HTTP call each', async () => {
      const subjects: Subject<ApiResponse<PlatformTenantDetail>>[] = [];
      service.update
        .mockImplementationOnce(() => {
          const s = new Subject<ApiResponse<PlatformTenantDetail>>();
          subjects.push(s);
          return s.asObservable();
        })
        .mockImplementationOnce(() => {
          const s = new Subject<ApiResponse<PlatformTenantDetail>>();
          subjects.push(s);
          return s.asObservable();
        })
        .mockImplementationOnce(() => {
          const s = new Subject<ApiResponse<PlatformTenantDetail>>();
          subjects.push(s);
          return s.asObservable();
        });
      service.list.mockReturnValue(of(paginated([])));

      const store = TestBed.inject(TenantsStore);

      const order: string[] = [];
      const p1 = firstValueFrom(
        store.update('t-1', { status: 'active' }).pipe(tap(() => order.push('active'))),
      );
      const p2 = firstValueFrom(
        store.update('t-1', { status: 'suspended' }).pipe(tap(() => order.push('suspended'))),
      );
      const p3 = firstValueFrom(
        store.update('t-1', { status: 'active' }).pipe(tap(() => order.push('active-final'))),
      );

      await Promise.resolve();
      expect(service.update).toHaveBeenCalledTimes(1);
      expect(order).toEqual([]);

      subjects[0].next({ data: { ...tenantDetail('t-1', 'Acme'), status: 'active' as const } });
      subjects[0].complete();
      await p1;
      expect(order).toEqual(['active']);
      expect(service.update).toHaveBeenCalledTimes(2);
      expect(service.update).toHaveBeenNthCalledWith(1, 't-1', { status: 'active' });

      subjects[1].next({ data: { ...tenantDetail('t-1', 'Acme'), status: 'suspended' as const } });
      subjects[1].complete();
      await p2;
      expect(order).toEqual(['active', 'suspended']);
      expect(service.update).toHaveBeenCalledTimes(3);
      expect(service.update).toHaveBeenNthCalledWith(2, 't-1', { status: 'suspended' });

      subjects[2].next({ data: { ...tenantDetail('t-1', 'Acme'), status: 'active' as const } });
      subjects[2].complete();
      const lastResult = await p3;
      expect(order).toEqual(['active', 'suspended', 'active-final']);
      expect(lastResult.status).toBe('active');

      expect(service.update).toHaveBeenNthCalledWith(3, 't-1', { status: 'active' });
      expect(service.update).toHaveBeenCalledTimes(3);
    });
  });
});
