import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  IntegrationDetail,
  IntegrationDetailWire,
  integrationDetailFromWire,
  IntegrationEventList,
  IntegrationEventListWire,
  integrationEventListFromWire,
  IntegrationList,
  IntegrationListWire,
  integrationListFromWire,
} from '../../../core/api/tenant-api.models';

export interface IntegrationConfigPayload {
  readonly config: Record<string, string>;
  readonly secrets?: Record<string, string>;
}

@Injectable({ providedIn: 'root' })
export class IntegrationsApiService {
  private readonly api = inject(ApiService);

  list(): Observable<ApiResponse<IntegrationList>> {
    return this.api.get<IntegrationListWire>('/tenant/integrations').pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: integrationListFromWire(data),
      })),
    );
  }

  detail(slug: string): Observable<ApiResponse<IntegrationDetail>> {
    return this.api.get<IntegrationDetailWire>(`/tenant/integrations/${slug}`).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: integrationDetailFromWire(data),
      })),
    );
  }

  connect(
    slug: string,
    body: IntegrationConfigPayload,
  ): Observable<ApiResponse<IntegrationDetail>> {
    return this.api.post<IntegrationDetailWire>(`/tenant/integrations/${slug}/connect`, body).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: integrationDetailFromWire(data),
      })),
    );
  }

  updateConfig(
    slug: string,
    body: IntegrationConfigPayload,
  ): Observable<ApiResponse<IntegrationDetail>> {
    return this.api.put<IntegrationDetailWire>(`/tenant/integrations/${slug}/config`, body).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: integrationDetailFromWire(data),
      })),
    );
  }

  disconnect(slug: string): Observable<ApiResponse<IntegrationDetail>> {
    return this.api.post<IntegrationDetailWire>(`/tenant/integrations/${slug}/disconnect`, {}).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: integrationDetailFromWire(data),
      })),
    );
  }

  events(slug: string, cursor: string | null): Observable<ApiResponse<IntegrationEventList>> {
    const params = cursor ? new HttpParams().set('cursor', cursor) : undefined;
    return this.api
      .get<IntegrationEventListWire>(`/tenant/integrations/${slug}/events`, params)
      .pipe(
        map(({ data, ...response }) => ({
          ...response,
          data: integrationEventListFromWire(data),
        })),
      );
  }
}
