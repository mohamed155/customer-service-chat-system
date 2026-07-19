import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  AnalyticsSummary,
  analyticsSummaryFromWire,
  AnalyticsSummaryWire,
  AnalyticsTimeseries,
  analyticsTimeseriesFromWire,
  AnalyticsTimeseriesWire,
} from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class AnalyticsApiService {
  private readonly api = inject(ApiService);

  getSummary(query: {
    from?: string;
    to?: string;
    channel?: string | null;
  }): Observable<ApiResponse<AnalyticsSummary>> {
    return this.api
      .get<{ data: AnalyticsSummaryWire }>('/tenant/analytics/summary', this.buildParams(query))
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: analyticsSummaryFromWire(data.data) })));
  }

  getTimeseries(query: {
    from?: string;
    to?: string;
    channel?: string | null;
  }): Observable<ApiResponse<AnalyticsTimeseries>> {
    return this.api
      .get<{ data: AnalyticsTimeseriesWire }>(
        '/tenant/analytics/timeseries',
        this.buildParams(query),
      )
      .pipe(
        map(({ data, ...rest }) => ({ ...rest, data: analyticsTimeseriesFromWire(data.data) })),
      );
  }

  private buildParams(query: {
    from?: string;
    to?: string;
    channel?: string | null;
  }): HttpParams | undefined {
    let params = new HttpParams();
    let hasParam = false;
    if (query.from) {
      params = params.set('from', query.from);
      hasParam = true;
    }
    if (query.to) {
      params = params.set('to', query.to);
      hasParam = true;
    }
    if (query.channel) {
      params = params.set('channel', query.channel);
      hasParam = true;
    }
    return hasParam ? params : undefined;
  }
}
