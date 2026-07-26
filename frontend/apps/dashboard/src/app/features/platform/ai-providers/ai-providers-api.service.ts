import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  ProviderStatus,
  ProviderStatusWire,
  providerStatusFromWire,
} from '../../../core/api/platform-analytics.models';

@Injectable({ providedIn: 'root' })
export class AiProvidersApiService {
  private readonly api = inject(ApiService);

  getStatus(window: '1h' | '24h' | '7d'): Observable<ApiResponse<ProviderStatus>> {
    const params = new HttpParams().set('window', window);
    return this.api
      .get<ProviderStatusWire>('/platform/ai-providers', params)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: providerStatusFromWire(data) })));
  }
}
