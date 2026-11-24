import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';
import type { components } from './generated/api';
import { ApiService } from './api.service';
import { ApiResponse } from './api.models';

type Schemas = components['schemas'];

export type RoutingCapacityResponse = Schemas['RoutingCapacityResponse'];
export type AgentCapacity = Schemas['AgentCapacity'];

@Injectable({ providedIn: 'root' })
export class RoutingCapacityApiService {
  private readonly api = inject(ApiService);

  get(): Observable<ApiResponse<RoutingCapacityResponse>> {
    return this.api.get<RoutingCapacityResponse>('tenant/routing/capacity');
  }

  setDefault(default_capacity: number): Observable<ApiResponse<RoutingCapacityResponse>> {
    return this.api.put<RoutingCapacityResponse>('tenant/routing/capacity/default', {
      default_capacity,
    });
  }

  setAgent(
    membershipId: string,
    capacity: number | null,
  ): Observable<ApiResponse<RoutingCapacityResponse>> {
    return this.api.put<RoutingCapacityResponse>(`tenant/routing/capacity/agents/${membershipId}`, {
      capacity,
    });
  }
}
