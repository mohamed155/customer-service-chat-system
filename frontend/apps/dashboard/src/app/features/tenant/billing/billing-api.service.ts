import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  BillingOverview,
  billingOverviewFromWire,
  BillingOverviewWire,
} from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class BillingApiService {
  private readonly api = inject(ApiService);

  getOverview(): Observable<ApiResponse<BillingOverview>> {
    return this.api
      .get<{ data: BillingOverviewWire }>('/tenant/billing/overview')
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: billingOverviewFromWire(data.data) })));
  }
}
