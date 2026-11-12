import { TestBed } from '@angular/core/testing';
import { Subject, of, throwError } from 'rxjs';
import {
  provideTenantStore,
  tenantStore,
  testTenant,
} from '../../core/state/testing/tenant-store.testing';
import { PagePayload, PAGE_ROUTE, RoutedPageDataService } from './routed-page-data.service';
import { RoutedPageStore } from './routed-page.store';

describe('RoutedPageStore', () => {
  let load: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    load = vi.fn();
    TestBed.configureTestingModule({
      providers: [
        RoutedPageStore,
        provideTenantStore(),
        { provide: RoutedPageDataService, useValue: { load } },
        { provide: PAGE_ROUTE, useValue: 'test-page' },
      ],
    });
    tenantStore().activate(testTenant('tenant-1'));
  });

  it('moves from pending to content when the initial page load resolves', () => {
    const payload: PagePayload = { page: 'test-page', data: 'test-data' } as unknown as PagePayload;
    load.mockReturnValue(of(payload));
    const store = TestBed.inject(RoutedPageStore);

    expect(store.lifecycle().status).toBe('data');
    expect(store.loading()).toBe(false);
    expect(store.data()).toEqual(payload);
    expect(load).toHaveBeenCalledWith('test-page', 'tenant-1');
  });

  it('moves from pending to empty when load returns null/undefined', () => {
    load.mockReturnValue(of(null));
    const store = TestBed.inject(RoutedPageStore);

    expect(store.lifecycle().status).toBe('empty');
    expect(store.loading()).toBe(false);
    expect(store.data()).toBeUndefined();
  });

  it('moves from pending to error when load rejects', () => {
    load.mockReturnValue(throwError(() => new Error('fail')));
    const store = TestBed.inject(RoutedPageStore);

    expect(store.lifecycle().status).toBe('error');
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeInstanceOf(Error);
  });

  it('reloads when the active tenant changes', () => {
    const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
    load.mockReturnValue(of(payload));
    const store = TestBed.inject(RoutedPageStore);

    expect(store.lifecycle().status).toBe('data');

    load.mockReturnValue(of(payload));
    tenantStore().switchTo(testTenant('tenant-2'));

    expect(store.lifecycle().status).toBe('data');
    expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-2');
  });

  it('ignores stale response from earlier tenant load', () => {
    const subjects: Subject<PagePayload>[] = [];
    load.mockImplementation(() => {
      const s = new Subject<PagePayload>();
      subjects.push(s);
      return s.asObservable();
    });
    const store = TestBed.inject(RoutedPageStore);

    expect(load).toHaveBeenCalledTimes(1);
    expect(store.lifecycle().status).toBe('pending');

    store.load('tenant-2');
    expect(load).toHaveBeenCalledTimes(2);

    store.load('tenant-3');
    expect(load).toHaveBeenCalledTimes(3);

    subjects[1].next({ page: 'test-page', data: 'tenant-2-data' } as unknown as PagePayload);
    subjects[1].complete();
    expect(store.lifecycle().status).toBe('pending');

    subjects[2].next({ page: 'test-page', data: 'tenant-3-data' } as unknown as PagePayload);
    subjects[2].complete();
    expect(store.lifecycle().status).toBe('data');
    expect(store.data()).toEqual({ page: 'test-page', data: 'tenant-3-data' });
  });

  it('ignores stale error from earlier tenant load', () => {
    const subjects: Subject<PagePayload>[] = [];
    load.mockImplementation(() => {
      const s = new Subject<PagePayload>();
      subjects.push(s);
      return s.asObservable();
    });
    const store = TestBed.inject(RoutedPageStore);
    expect(load).toHaveBeenCalledTimes(1);

    store.load('tenant-2');
    expect(load).toHaveBeenCalledTimes(2);

    store.load('tenant-3');
    expect(load).toHaveBeenCalledTimes(3);

    subjects[1].error(new Error('A failed'));
    expect(store.lifecycle().status).toBe('pending');

    subjects[2].next({ page: 'test-page', data: 'tenant-3-data' } as unknown as PagePayload);
    subjects[2].complete();
    expect(store.lifecycle().status).toBe('data');
    expect(store.data()).toEqual({ page: 'test-page', data: 'tenant-3-data' });
  });

  describe('retry', () => {
    it('calls load with the current active tenant', () => {
      const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
      load.mockReturnValue(of(payload));
      const store = TestBed.inject(RoutedPageStore);

      load.mockReturnValue(of(payload));
      store.retry();
      expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-1');
    });

    it('uses the new active tenant after a tenant switch', () => {
      const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
      load.mockReturnValue(of(payload));
      const store = TestBed.inject(RoutedPageStore);

      tenantStore().switchTo(testTenant('tenant-42'));

      load.mockReturnValue(of(payload));
      store.retry();
      expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-42');
    });
  });
});
