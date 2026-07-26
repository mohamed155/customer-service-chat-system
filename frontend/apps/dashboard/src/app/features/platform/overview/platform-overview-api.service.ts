import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  PlatformSummary,
  PlatformSummaryWire,
  platformSummaryFromWire,
} from '../../../core/api/platform-analytics.models';

@Injectable({ providedIn: 'root' })
export class PlatformOverviewApiService {
  private readonly api = inject(ApiService);

  getSummary(query: { from?: string; to?: string }): Observable<ApiResponse<PlatformSummary>> {
    let params: HttpParams | undefined;
    if (query.from || query.to) {
      params = new HttpParams();
      if (query.from) params = params.set('from', query.from);
      if (query.to) params = params.set('to', query.to);
    }
    return this.api
      .get<PlatformSummaryWire>('/platform/analytics/summary', params)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: platformSummaryFromWire(data) })));
  }
}
