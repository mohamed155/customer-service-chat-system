import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import { ApiResponse, PaginatedResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  CreateTenantPayload,
  PlatformTenantDetail,
  TenantDirectoryQuery,
  TenantSummary,
  UpdateTenantPayload,
} from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class PlatformTenantsService {
  private readonly api = inject(ApiService);

  list(
    params: TenantDirectoryQuery = {},
  ): Observable<ApiResponse<PaginatedResponse<TenantSummary>>> {
    return this.api.list<TenantSummary>('/platform/tenants', params);
  }

  get(id: string): Observable<ApiResponse<PlatformTenantDetail>> {
    return this.api.get<PlatformTenantDetail>(`/platform/tenants/${id}`);
  }

  create(payload: CreateTenantPayload): Observable<ApiResponse<PlatformTenantDetail>> {
    return this.api.post<PlatformTenantDetail>('/platform/tenants', payload);
  }

  update(id: string, payload: UpdateTenantPayload): Observable<ApiResponse<PlatformTenantDetail>> {
    return this.api.patch<PlatformTenantDetail>(`/platform/tenants/${id}`, payload);
  }
}
