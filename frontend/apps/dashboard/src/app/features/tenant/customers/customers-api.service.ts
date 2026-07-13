import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiListQuery, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  ConversationSummary,
  conversationSummaryFromWire,
  ConversationSummaryWire,
  CreateCustomerPayload,
  createPayloadToWire,
  Customer,
  CustomerDetail,
  customerDetailFromWire,
  CustomerDetailWire,
  customerFromWire,
  CustomerWire,
  UpdateCustomerPayload,
  updatePayloadToWire,
} from '../../../core/api/tenant-api.models';

interface CustomerListWireResponse {
  readonly data: CustomerWire[];
  readonly pagination: {
    readonly next_cursor: string | null;
    readonly has_more: boolean;
  };
}

interface CustomerDetailWireResponse {
  readonly data: CustomerDetailWire;
}

interface ConversationHistoryWireResponse {
  readonly data: ConversationSummaryWire[];
  readonly pagination: {
    readonly next_cursor: string | null;
    readonly has_more: boolean;
  };
}

@Injectable({ providedIn: 'root' })
export class CustomersApiService {
  private readonly api = inject(ApiService);

  list(
    query: Pick<ApiListQuery, 'q' | 'limit'> = {},
    cursor?: string,
  ): Observable<ApiResponse<PaginatedResponse<Customer>>> {
    return this.api
      .get<CustomerListWireResponse>('/tenant/customers', this.buildParams({ ...query, cursor }))
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            items: data.data.map(customerFromWire),
            nextCursor: data.pagination.next_cursor,
            hasMore: data.pagination.has_more,
          },
        })),
      );
  }

  getCustomer(id: string): Observable<ApiResponse<CustomerDetail>> {
    return this.api.get<CustomerDetailWireResponse>(`/tenant/customers/${id}`).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: customerDetailFromWire(data.data),
      })),
    );
  }

  createCustomer(payload: CreateCustomerPayload): Observable<ApiResponse<CustomerDetail>> {
    const wirePayload = createPayloadToWire(payload);
    return this.api.post<CustomerDetailWireResponse>('/tenant/customers', wirePayload).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: customerDetailFromWire(data.data),
      })),
    );
  }

  updateCustomer(
    id: string,
    payload: UpdateCustomerPayload,
  ): Observable<ApiResponse<CustomerDetail>> {
    const wirePayload = updatePayloadToWire(payload);
    return this.api.patch<CustomerDetailWireResponse>(`/tenant/customers/${id}`, wirePayload).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: customerDetailFromWire(data.data),
      })),
    );
  }

  getConversationHistory(
    id: string,
  ): Observable<ApiResponse<PaginatedResponse<ConversationSummary>>> {
    return this.api
      .get<ConversationHistoryWireResponse>(`/tenant/customers/${id}/conversations`)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: {
            items: data.data.map(conversationSummaryFromWire),
            nextCursor: data.pagination.next_cursor,
            hasMore: data.pagination.has_more,
          },
        })),
      );
  }

  private buildParams(query: ApiListQuery): HttpParams {
    let params = new HttpParams();
    for (const [key, value] of Object.entries(query))
      if (value !== undefined) params = params.set(key, String(value));
    return params;
  }
}
