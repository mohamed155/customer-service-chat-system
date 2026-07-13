import { TestBed } from '@angular/core/testing';
import { firstValueFrom, of, throwError } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  ConversationSummaryWire,
  CreateCustomerPayload,
  CustomerDetailWire,
  UpdateCustomerPayload,
  conversationSummaryFromWire,
  customerDetailFromWire,
  customerFromWire,
} from '../../../core/api/tenant-api.models';
import { CustomersApiService } from './customers-api.service';

// apiError helper removed - unused

describe('CustomersApiService', () => {
  let service: CustomersApiService;
  let api: {
    list: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    post: ReturnType<typeof vi.fn>;
    patch: ReturnType<typeof vi.fn>;
  };

  beforeEach(() => {
    api = { list: vi.fn(), get: vi.fn(), post: vi.fn(), patch: vi.fn() };
    TestBed.configureTestingModule({
      providers: [CustomersApiService, { provide: ApiService, useValue: api }],
    });
    service = TestBed.inject(CustomersApiService);
  });

  it('maps the customer list wire response while forwarding query parameters', async () => {
    const wireCustomer = {
      id: 'customer-1',
      display_name: 'Sara Ali',
      email: 'sara@example.com',
      phone: '+201001234567',
      channels: ['email'],
      created_at: '2026-07-13T10:00:00Z',
      updated_at: '2026-07-13T10:00:00Z',
    };
    const response = {
      data: {
        data: [wireCustomer],
        pagination: { next_cursor: 'next-page', has_more: true },
      },
      requestId: 'request-1',
    };
    api.get.mockReturnValue(of(response));

    const result = await firstValueFrom(service.list({ q: 'sara', limit: 20 }, 'next-page'));

    expect(api.get).toHaveBeenCalled();
    expect(api.get.mock.calls[0][0]).toBe('/tenant/customers');
    expect(result).toEqual({
      data: { items: [customerFromWire(wireCustomer)], nextCursor: 'next-page', hasMore: true },
      requestId: 'request-1',
    });
  });

  it('fetches a customer detail via the targeted path and forwards the response', async () => {
    const wireDetail: CustomerDetailWire = {
      id: 'customer-1',
      display_name: 'Sara Ali',
      email: 'sara@example.com',
      phone: '+201001234567',
      channels: ['email', 'whatsapp'],
      created_at: '2026-07-13T10:00:00Z',
      updated_at: '2026-07-13T10:00:00Z',
      identifiers: [{ id: 'id-1', channel: 'whatsapp', identifier: '+201001234567' }],
      metadata: { plan: 'enterprise' },
    };
    const customerDetail = customerDetailFromWire(wireDetail);
    api.get.mockReturnValue(of({ data: { data: wireDetail }, requestId: 'req-detail' }));

    const result = await firstValueFrom(service.getCustomer('customer-1'));

    expect(api.get).toHaveBeenCalledWith('/tenant/customers/customer-1');
    expect(result).toEqual({ data: customerDetail, requestId: 'req-detail' });
  });

  it('maps the conversation history wire response with has_more but no next_cursor', async () => {
    const wireSummary: ConversationSummaryWire = {
      id: 'conv-1',
      channel: 'web_chat',
      status: 'open',
      last_activity_at: '2026-07-13T09:30:00Z',
      created_at: '2026-07-12T14:00:00Z',
    };
    const summary = conversationSummaryFromWire(wireSummary);
    api.get.mockReturnValue(
      of({
        data: {
          data: [wireSummary],
          pagination: { next_cursor: null, has_more: true },
        },
        requestId: 'req-history',
      }),
    );

    const result = await firstValueFrom(service.getConversationHistory('customer-1'));

    expect(api.get).toHaveBeenCalledWith('/tenant/customers/customer-1/conversations');
    expect(result).toEqual({
      data: { items: [summary], nextCursor: null, hasMore: true },
      requestId: 'req-history',
    });
  });

  it('returns an empty history when the customer has no conversations', async () => {
    api.get.mockReturnValue(
      of({
        data: { data: [], pagination: { next_cursor: null, has_more: false } },
        requestId: 'req-history-empty',
      }),
    );

    const result = await firstValueFrom(service.getConversationHistory('customer-2'));

    expect(api.get).toHaveBeenCalledWith('/tenant/customers/customer-2/conversations');
    expect(result.data).toEqual({ items: [], nextCursor: null, hasMore: false });
    expect(result.requestId).toBe('req-history-empty');
  });

  describe('createCustomer', () => {
    const payload: CreateCustomerPayload = {
      displayName: 'Sara Ali',
      email: 'sara@example.com',
      identifiers: [{ channel: 'email', identifier: 'sara@example.com' }],
    };

    const buildWireDetail = (overrides: Partial<CustomerDetailWire> = {}): CustomerDetailWire => ({
      id: 'customer-1',
      display_name: 'Sara Ali',
      email: 'sara@example.com',
      phone: '+201001234567',
      channels: ['email'],
      identifiers: [],
      metadata: {},
      created_at: '2026-07-13T10:00:00Z',
      updated_at: '2026-07-13T10:00:00Z',
      ...overrides,
    });

    it('sends a POST request with the payload and returns the created customer', async () => {
      const wireDetail = buildWireDetail();
      const detail = customerDetailFromWire(wireDetail);
      api.post.mockReturnValue(of({ data: { data: wireDetail }, requestId: 'req-create' }));

      const result = await firstValueFrom(service.createCustomer(payload));

      expect(api.post).toHaveBeenCalledWith('/tenant/customers', {
        display_name: payload.displayName,
        email: payload.email,
        identifiers: payload.identifiers,
      });
      expect(result).toEqual({ data: detail, requestId: 'req-create' });
    });

    it('propagates a 409 conflict response with holder details', async () => {
      const conflict: ApiError = {
        code: 'conflict',
        message: 'Identifier already held by another customer',
        status: 409,
        details: [
          {
            field: 'identifiers',
            code: 'unique_violation',
            message: 'Identifier already held by Sara Ali (holder@example.com)',
          },
        ],
      };
      api.post.mockReturnValue(throwError(() => conflict));

      await expect(firstValueFrom(service.createCustomer(payload))).rejects.toEqual(conflict);
    });

    it('propagates a 422 validation error with field-level details', async () => {
      const validation: ApiError = {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [
          { field: 'displayName', code: 'required', message: 'Display name is required' },
          {
            field: 'identifiers[0].channel',
            code: 'invalid',
            message: 'Invalid channel value',
          },
        ],
      };
      api.post.mockReturnValue(throwError(() => validation));

      await expect(firstValueFrom(service.createCustomer(payload))).rejects.toEqual(validation);
    });
  });

  describe('updateCustomer', () => {
    const payload: UpdateCustomerPayload = {
      displayName: 'Sara Updated',
      email: null,
      identifiers: [],
      metadata: {},
    };

    const buildWireDetail = (overrides: Partial<CustomerDetailWire> = {}): CustomerDetailWire => ({
      id: 'customer-1',
      display_name: 'Sara Updated',
      email: null,
      phone: '+201001234567',
      channels: ['email'],
      identifiers: [],
      metadata: {},
      created_at: '2026-07-13T10:00:00Z',
      updated_at: '2026-07-13T10:00:00Z',
      ...overrides,
    });

    it('sends PATCH with snake_case wire body and maps response', async () => {
      const wireDetail = buildWireDetail();
      const detail = customerDetailFromWire(wireDetail);
      api.patch.mockReturnValue(of({ data: { data: wireDetail }, requestId: 'req-update' }));

      const result = await firstValueFrom(service.updateCustomer('customer-1', payload));

      expect(api.patch).toHaveBeenCalledWith('/tenant/customers/customer-1', {
        display_name: 'Sara Updated',
        email: null,
        identifiers: [],
        metadata: {},
      });
      expect(result).toEqual({
        data: detail,
        requestId: 'req-update',
      });
    });

    it('sends explicit null clears in the snake_case wire body', async () => {
      const clearPayload: UpdateCustomerPayload = {
        displayName: 'Sara',
        email: null,
        phone: null,
        identifiers: [],
        metadata: {},
      };
      const wireDetail = buildWireDetail({
        display_name: 'Sara',
        phone: null,
      });
      api.patch.mockReturnValue(of({ data: { data: wireDetail }, requestId: 'req-clear' }));

      await firstValueFrom(service.updateCustomer('customer-1', clearPayload));

      expect(api.patch).toHaveBeenCalledWith('/tenant/customers/customer-1', {
        display_name: 'Sara',
        email: null,
        phone: null,
        identifiers: [],
        metadata: {},
      });
    });

    it('propagates a 409 conflict response with holder details', async () => {
      const conflict: ApiError = {
        code: 'conflict',
        message: 'Identifier already held by another customer',
        status: 409,
        details: [
          {
            field: 'identifiers',
            code: 'unique_violation',
            message: 'Identifier already held by Sara Ali (holder@example.com)',
          },
        ],
      };
      api.patch.mockReturnValue(throwError(() => conflict));

      await expect(firstValueFrom(service.updateCustomer('customer-1', payload))).rejects.toEqual(
        conflict,
      );
    });

    it('propagates a 422 validation error with field-level details', async () => {
      const validation: ApiError = {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [
          {
            field: 'displayName',
            code: 'required',
            message: 'Display name is required',
          },
        ],
      };
      api.patch.mockReturnValue(throwError(() => validation));

      await expect(firstValueFrom(service.updateCustomer('customer-1', payload))).rejects.toEqual(
        validation,
      );
    });
  });
});
