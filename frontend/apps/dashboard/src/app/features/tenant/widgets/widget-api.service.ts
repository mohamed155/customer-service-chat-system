import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  WidgetInstance,
  CreateWidgetInstancePayload,
  UpdateWidgetInstancePayload,
  WidgetSnippet,
} from '../../../core/api/widget.models';

@Injectable({ providedIn: 'root' })
export class WidgetApiService {
  private readonly api = inject(ApiService);

  list(): Observable<ApiResponse<WidgetInstance[]>> {
    return this.api.get<WidgetInstance[]>('tenant/widgets');
  }

  get(id: string): Observable<ApiResponse<WidgetInstance>> {
    return this.api.get<WidgetInstance>(`tenant/widgets/${id}`);
  }

  create(payload: CreateWidgetInstancePayload): Observable<ApiResponse<WidgetInstance>> {
    return this.api.post<WidgetInstance>('tenant/widgets', payload);
  }

  update(
    id: string,
    payload: UpdateWidgetInstancePayload,
  ): Observable<ApiResponse<WidgetInstance>> {
    return this.api.put<WidgetInstance>(`tenant/widgets/${id}`, payload);
  }

  delete(id: string): Observable<ApiResponse<void>> {
    return this.api.delete<void>(`tenant/widgets/${id}`);
  }

  getSnippet(id: string): Observable<ApiResponse<WidgetSnippet>> {
    return this.api.get<WidgetSnippet>(`tenant/widgets/${id}/snippet`);
  }
}
