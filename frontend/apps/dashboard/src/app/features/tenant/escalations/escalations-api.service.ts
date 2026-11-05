import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { ApiListQuery, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { QueueEntry } from '../../../core/api/tenant-api.models';

/** Wire envelope of `GET /tenant/escalations/queue` (`QueueListResponse`). */
interface QueueListWire {
  readonly data: QueueEntry[];
  readonly pagination: { readonly next_cursor: string | null; readonly has_more: boolean };
}

@Injectable({ providedIn: 'root' })
export class EscalationsApiService {
  private readonly api = inject(ApiService);

  listQueue(query: ApiListQuery = {}): Observable<ApiResponse<PaginatedResponse<QueueEntry>>> {
    let params = new HttpParams();
    for (const [key, value] of Object.entries(query))
      if (value !== undefined) params = params.set(key, String(value));
    return this.api.get<QueueListWire>('tenant/escalations/queue', params).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: {
          items: data.data,
          nextCursor: data.pagination.next_cursor,
          hasMore: data.pagination.has_more,
        },
      })),
    );
  }

  claim(id: string): Observable<ApiResponse<void>> {
    return this.api.post<void>(`tenant/escalations/${id}/claim`, {});
  }
}
