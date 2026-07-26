import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  SystemHealth,
  SystemHealthWire,
  systemHealthFromWire,
} from '../../../core/api/platform-analytics.models';

@Injectable({ providedIn: 'root' })
export class SystemHealthApiService {
  private readonly api = inject(ApiService);

  getHealth(): Observable<ApiResponse<SystemHealth>> {
    return this.api
      .get<SystemHealthWire>('/platform/health')
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: systemHealthFromWire(data) })));
  }
}
