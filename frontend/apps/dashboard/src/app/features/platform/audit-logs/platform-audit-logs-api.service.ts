import { HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import { AuditList, AuditListWire, auditListFromWire } from '../../../core/api/tenant-api.models';

@Injectable({ providedIn: 'root' })
export class PlatformAuditLogsApiService {
  private readonly api = inject(ApiService);

  list(query: {
    cursor?: string | null;
    from?: string;
    to?: string;
    category?: string | null;
    actorId?: string | null;
    tenantId?: string | null;
  }): Observable<ApiResponse<AuditList>> {
    return this.api.get<AuditListWire>('/platform/audit-logs', this.buildParams(query)).pipe(
      map(({ data, ...response }) => ({
        ...response,
        data: auditListFromWire(data),
      })),
    );
  }

  private buildParams(query: Record<string, string | null | undefined>): HttpParams | undefined {
    let params = new HttpParams();
    let hasParam = false;
    for (const [key, value] of Object.entries(query)) {
      if (value !== null && value !== undefined && value !== '') {
        const wireKey = key === 'actorId' ? 'actor_id' : key === 'tenantId' ? 'tenant_id' : key;
        params = params.set(wireKey, value);
        hasParam = true;
      }
    }
    return hasParam ? params : undefined;
  }
}
