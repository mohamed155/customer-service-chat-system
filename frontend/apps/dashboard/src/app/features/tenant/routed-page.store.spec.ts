import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Store } from '@ngrx/store';
import { Subject, of, throwError } from 'rxjs';
import { PagePayload, PAGE_ROUTE, RoutedPageDataService } from './routed-page-data.service';
import { RoutedPageStore } from './routed-page.store';

describe('RoutedPageStore', () => {
  const activeTenant = signal({ id: 'tenant-1' });
  let load: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    activeTenant.set({ id: 'tenant-1' });
    load = vi.fn();
    TestBed.configureTestingModule({
      providers: [
        RoutedPageStore,
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
        { provide: RoutedPageDataService, useValue: { load } },
        { provide: PAGE_ROUTE, useValue: 'test-page' },
      ],
    });
  });

  it('moves from pending to content when the initial page load resolves', () => {
    const store = TestBed.inject(RoutedPageStore);
    const payload: PagePayload = { page: 'test-page', data: 'test-data' } as unknown as PagePayload;
    load.mockReturnValue(of(payload));
    TestBed.flushEffects();

    expect(store.lifecycle().status).toBe('data');
    expect(store.loading()).toBe(false);
    expect(store.data()).toEqual(payload);
    expect(load).toHaveBeenCalledWith('test-page', 'tenant-1');
  });

  it('moves from pending to empty when load returns null/undefined', () => {
    const store = TestBed.inject(RoutedPageStore);
    load.mockReturnValue(of(null));
    TestBed.flushEffects();

    expect(store.lifecycle().status).toBe('empty');
    expect(store.loading()).toBe(false);
    expect(store.data()).toBeUndefined();
  });

  it('moves from pending to error when load rejects', () => {
    const store = TestBed.inject(RoutedPageStore);
    load.mockReturnValue(throwError(() => new Error('fail')));
    TestBed.flushEffects();

    expect(store.lifecycle().status).toBe('error');
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeInstanceOf(Error);
  });

  it('reloads when the active tenant changes', () => {
    const store = TestBed.inject(RoutedPageStore);
    const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
    load.mockReturnValue(of(payload));
    TestBed.flushEffects();

    expect(store.lifecycle().status).toBe('data');

    load.mockReturnValue(of(payload));
    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    expect(store.lifecycle().status).toBe('data');
    expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-2');
  });

  it('ignores stale response from earlier tenant load', () => {
    const store = TestBed.inject(RoutedPageStore);
    const subjects: Subject<PagePayload>[] = [];
    load.mockImplementation(() => {
      const s = new Subject<PagePayload>();
      subjects.push(s);
      return s.asObservable();
    });
    TestBed.flushEffects();

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
    const store = TestBed.inject(RoutedPageStore);
    const subjects: Subject<PagePayload>[] = [];
    load.mockImplementation(() => {
      const s = new Subject<PagePayload>();
      subjects.push(s);
      return s.asObservable();
    });
    TestBed.flushEffects();
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
      const store = TestBed.inject(RoutedPageStore);
      const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
      load.mockReturnValue(of(payload));
      TestBed.flushEffects();

      load.mockReturnValue(of(payload));
      store.retry();
      expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-1');
    });

    it('uses the new active tenant after a tenant switch', () => {
      const store = TestBed.inject(RoutedPageStore);
      const payload: PagePayload = { page: 'test-page', data: 'data' } as unknown as PagePayload;
      load.mockReturnValue(of(payload));
      TestBed.flushEffects();

      activeTenant.set({ id: 'tenant-42' });
      TestBed.flushEffects();

      load.mockReturnValue(of(payload));
      store.retry();
      expect(load).toHaveBeenLastCalledWith('test-page', 'tenant-42');
    });
  });
});
