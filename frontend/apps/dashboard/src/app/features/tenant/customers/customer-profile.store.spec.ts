import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Store } from '@ngrx/store';
import { of, Subject, throwError } from 'rxjs';
import { ApiError, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ConversationSummary, CustomerDetail } from '../../../core/api/tenant-api.models';
import { CustomerProfileStore } from './customer-profile.store';
import { CustomersApiService } from './customers-api.service';

const apiError = (overrides: Partial<ApiError> = {}): ApiError => ({
  code: 'internal_error',
  message: 'Something went wrong',
  status: 500,
  requestId: 'req-1',
  ...overrides,
});

const customer = (id: string, overrides: Partial<CustomerDetail> = {}): CustomerDetail => ({
  id,
  displayName: `Customer ${id}`,
  email: `${id}@example.test`,
  phone: '+1 555 0100',
  channels: ['email'],
  identifiers: [],
  metadata: {},
  createdAt: '2026-07-13T10:00:00Z',
  updatedAt: '2026-07-13T10:00:00Z',
  ...overrides,
});

const conversation = (
  id: string,
  overrides: Partial<ConversationSummary> = {},
): ConversationSummary => ({
  id,
  channel: 'web_chat',
  status: 'open',
  lastActivityAt: '2026-07-13T09:30:00Z',
  createdAt: '2026-07-12T14:00:00Z',
  ...overrides,
});

describe('CustomerProfileStore', () => {
  let api: {
    getCustomer: ReturnType<typeof vi.fn>;
    getConversationHistory: ReturnType<typeof vi.fn>;
  };

  function configureStore(activeTenant = signal<{ id: string } | null>(null)) {
    TestBed.configureTestingModule({
      providers: [
        CustomerProfileStore,
        { provide: CustomersApiService, useValue: api },
        { provide: Store, useValue: { selectSignal: () => activeTenant } },
      ],
    });
    return { store: TestBed.inject(CustomerProfileStore), activeTenant };
  }

  beforeEach(() => {
    api = { getCustomer: vi.fn(), getConversationHistory: vi.fn() };
  });

  it('starts in an empty, idle state with no error and no loading', () => {
    const { store } = configureStore();

    expect(store.customerId()).toBeNull();
    expect(store.customer()).toBeNull();
    expect(store.conversations()).toEqual([]);
    expect(store.hasMoreConversations()).toBe(false);
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.notFound()).toBe(false);
  });

  it('fetches customer detail and conversation history in parallel and stores the kernel payload', () => {
    const detail = customer('customer-1', {
      identifiers: [{ id: 'id-1', channel: 'whatsapp', identifier: '+201001234567' }],
      metadata: { plan: 'enterprise' },
    });
    const history: ConversationSummary[] = [
      conversation('c-1', { status: 'pending' }),
      conversation('c-2'),
    ];
    const historyResponse: ApiResponse<PaginatedResponse<ConversationSummary>> = {
      data: { items: history, nextCursor: null, hasMore: true },
      requestId: 'req-history',
    };
    api.getCustomer.mockReturnValue(of({ data: detail, requestId: 'req-customer' }));
    api.getConversationHistory.mockReturnValue(of(historyResponse));

    const { store } = configureStore();
    store.loadProfile('customer-1');

    expect(api.getCustomer).toHaveBeenCalledWith('customer-1');
    expect(api.getConversationHistory).toHaveBeenCalledWith('customer-1');
    expect(store.customerId()).toBe('customer-1');
    expect(store.customer()).toEqual(detail);
    expect(store.conversations()).toEqual(history);
    expect(store.hasMoreConversations()).toBe(true);
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.notFound()).toBe(false);
  });

  it('resets state to initial before loading a profile, preventing stale data', () => {
    const detail = customer('customer-1');
    const secondResponse$ = new Subject<ApiResponse<CustomerDetail>>();
    api.getCustomer
      .mockReturnValueOnce(of({ data: detail, requestId: 'req-customer' }))
      .mockReturnValueOnce(secondResponse$.asObservable());
    api.getConversationHistory
      .mockReturnValueOnce(
        of({ data: { items: [], nextCursor: null, hasMore: false }, requestId: 'req-h1' }),
      )
      .mockReturnValueOnce(
        of({ data: { items: [], nextCursor: null, hasMore: false }, requestId: 'req-h2' }),
      );

    const { store } = configureStore();
    store.loadProfile('customer-1');
    expect(store.customer()).toEqual(detail);

    store.loadProfile('customer-2');
    expect(store.customer()).toBeNull();
    expect(store.conversations()).toEqual([]);
    expect(store.loading()).toBe(true);
    expect(store.error()).toBeNull();
    expect(store.notFound()).toBe(false);

    secondResponse$.next({
      data: customer('customer-2', { displayName: 'Second' }),
      requestId: 'req-c2',
    });
    secondResponse$.complete();
    expect(store.customer()?.displayName).toBe('Second');
  });

  it('clears cached profile data and cancels stale requests when the active tenant changes', () => {
    const tenant1 = signal<{ id: string }>({ id: 'tenant-1' });
    api.getCustomer.mockReturnValue(of({ data: customer('customer-1'), requestId: 'req-c' }));
    api.getConversationHistory.mockReturnValue(
      of({ data: { items: [], nextCursor: null, hasMore: false }, requestId: 'req-h' }),
    );

    const { store, activeTenant } = configureStore(tenant1);
    TestBed.flushEffects();
    store.loadProfile('customer-1');
    expect(store.customer()).not.toBeNull();

    activeTenant.set({ id: 'tenant-2' });
    TestBed.flushEffects();

    expect(store.customerId()).toBeNull();
    expect(store.customer()).toBeNull();
    expect(store.conversations()).toEqual([]);
    expect(store.hasMoreConversations()).toBe(false);
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.notFound()).toBe(false);
  });

  it('captures a 404 not_found kernel error envelope and marks the profile as not found', () => {
    const failure = apiError({ code: 'not_found', message: 'Customer not found', status: 404 });
    api.getCustomer.mockReturnValue(throwError(() => failure));
    api.getConversationHistory.mockReturnValue(
      of({
        data: { items: [], nextCursor: null, hasMore: false },
        requestId: 'req-history',
      }),
    );

    const { store } = configureStore();
    store.loadProfile('customer-missing');

    expect(store.loading()).toBe(false);
    expect(store.error()).toEqual(failure);
    expect(store.notFound()).toBe(true);
    expect(store.customer()).toBeNull();
    expect(store.conversations()).toEqual([]);
  });

  it('captures non-not_found kernel errors as a separate error signal and stops loading', () => {
    const failure = apiError({
      code: 'unauthorized',
      message: 'You do not have access',
      status: 403,
    });
    api.getCustomer.mockReturnValue(throwError(() => failure));
    api.getConversationHistory.mockReturnValue(
      of({
        data: { items: [], nextCursor: null, hasMore: false },
        requestId: 'req-history',
      }),
    );

    const { store } = configureStore();
    store.loadProfile('customer-1');

    expect(store.notFound()).toBe(false);
    expect(store.error()).toEqual(failure);
    expect(store.loading()).toBe(false);
  });

  it('captures a kernel error from the history fetch as the error signal', () => {
    const detail = customer('customer-1');
    api.getCustomer.mockReturnValue(of({ data: detail, requestId: 'req-customer' }));
    const failure = apiError({
      code: 'service_unavailable',
      message: 'History is temporarily unavailable',
      status: 503,
    });
    api.getConversationHistory.mockReturnValue(throwError(() => failure));

    const { store } = configureStore();
    store.loadProfile('customer-1');

    expect(store.error()).toEqual(failure);
    expect(store.loading()).toBe(false);
  });

  it('cancels an in-flight load when a new id is requested and replaces the customer detail', () => {
    const firstDetail = customer('customer-1');
    const secondDetail = customer('customer-2', { displayName: 'Second Customer' });
    const firstHistory$ = new Subject<ApiResponse<PaginatedResponse<ConversationSummary>>>();
    api.getCustomer
      .mockReturnValueOnce(of({ data: firstDetail, requestId: 'req-c1' }))
      .mockReturnValueOnce(of({ data: secondDetail, requestId: 'req-c2' }));
    api.getConversationHistory
      .mockReturnValueOnce(firstHistory$.asObservable())
      .mockReturnValueOnce(
        of({
          data: { items: [], nextCursor: null, hasMore: false },
          requestId: 'req-h2',
        }),
      );

    const { store } = configureStore();
    store.loadProfile('customer-1');
    store.loadProfile('customer-2');

    expect(store.customerId()).toBe('customer-2');
    expect(store.customer()).toEqual(secondDetail);
    expect(api.getCustomer).toHaveBeenCalledTimes(2);
    expect(api.getConversationHistory).toHaveBeenCalledTimes(2);
  });

  it('re-fetches both endpoints with the most recent id when retry is invoked', () => {
    const detail = customer('customer-1');
    const failure = apiError({ code: 'service_unavailable', status: 503 });
    api.getCustomer
      .mockReturnValueOnce(throwError(() => failure))
      .mockReturnValueOnce(of({ data: detail, requestId: 'req-customer' }));
    api.getConversationHistory
      .mockReturnValueOnce(
        of({
          data: { items: [], nextCursor: null, hasMore: false },
          requestId: 'req-h1',
        }),
      )
      .mockReturnValueOnce(
        of({
          data: { items: [conversation('c-1')], nextCursor: null, hasMore: false },
          requestId: 'req-h2',
        }),
      );

    const { store } = configureStore();
    store.loadProfile('customer-1');
    expect(store.error()).toEqual(failure);

    store.retry();

    expect(api.getCustomer).toHaveBeenCalledTimes(2);
    expect(api.getConversationHistory).toHaveBeenCalledTimes(2);
    expect(api.getCustomer).toHaveBeenLastCalledWith('customer-1');
    expect(api.getConversationHistory).toHaveBeenLastCalledWith('customer-1');
    expect(store.customer()).toEqual(detail);
    expect(store.conversations()).toEqual([conversation('c-1')]);
    expect(store.error()).toBeNull();
  });

  it('clears state on reset so the next load starts from idle', () => {
    const detail = customer('customer-1');
    api.getCustomer.mockReturnValue(of({ data: detail, requestId: 'req-customer' }));
    api.getConversationHistory.mockReturnValue(
      of({
        data: { items: [conversation('c-1')], nextCursor: null, hasMore: true },
        requestId: 'req-h',
      }),
    );

    const { store } = configureStore();
    store.loadProfile('customer-1');
    expect(store.customer()).not.toBeNull();

    store.reset();

    expect(store.customerId()).toBeNull();
    expect(store.customer()).toBeNull();
    expect(store.conversations()).toEqual([]);
    expect(store.hasMoreConversations()).toBe(false);
    expect(store.loading()).toBe(false);
    expect(store.error()).toBeNull();
    expect(store.notFound()).toBe(false);
  });
});
