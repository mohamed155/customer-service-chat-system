import { inject, Injectable } from '@angular/core';
import { Store } from '@ngrx/store';
import { firstValueFrom } from 'rxjs';
import { ApiService } from '../api/api.service';
import { TenantSummary } from '../api/tenant-api.models';
import {
  selectActiveTenant,
  selectStatus,
  tenantContextActions,
} from '../state/tenant-context.feature';

@Injectable({ providedIn: 'root' })
export class TenantContextService {
  private store = inject(Store);
  private api = inject(ApiService);

  readonly activeTenant = this.store.selectSignal(selectActiveTenant);
  readonly status = this.store.selectSignal(selectStatus);

  async select(tenantId: string): Promise<TenantSummary> {
    this.store.dispatch(tenantContextActions.switchTenantRequested({ tenantId }));
    try {
      const response = await firstValueFrom(
        this.api.post<TenantSummary>(`/platform/tenants/${tenantId}/switch`, undefined),
      );
      this.store.dispatch(tenantContextActions.switchTenantSucceeded({ tenant: response.data }));
      return response.data;
    } catch {
      this.store.dispatch(tenantContextActions.switchTenantFailed());
      throw new Error('Failed to switch tenant');
    }
  }

  clear(): void {
    this.store.dispatch(tenantContextActions.clearActiveTenant());
  }

  set(tenant: TenantSummary): void {
    this.store.dispatch(tenantContextActions.setActiveTenant({ tenant }));
  }
}
