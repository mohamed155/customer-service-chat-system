import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiService } from '../../../core/api/api.service';
import { ApiListQuery, ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { QueueEntry } from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class EscalationsApiService {
  private readonly api = inject(ApiService);

  listQueue(query?: ApiListQuery): Observable<ApiResponse<PaginatedResponse<QueueEntry>>> {
    return this.api.list<QueueEntry>('tenant/escalations/queue', query);
  }

  claim(id: string): Observable<ApiResponse<void>> {
    return this.api.post<void>(`tenant/escalations/${id}/claim`, {});
  }
}
