import { computed, inject, Injectable, signal } from '@angular/core';
import { Store } from '@ngrx/store';
import { firstValueFrom } from 'rxjs';
import { ApiError } from '../api/api.models';
import { ApiService } from '../api/api.service';
import { MeResponse, TenantSummary } from '../api/tenant-api.models';
import { selectActiveTenant } from '../state/tenant-context.feature';
import { tenantContextActions } from '../state/tenant-context.feature';

@Injectable({ providedIn: 'root' })
export class CurrentUserService {
  private readonly api = inject(ApiService);
  private readonly store = inject(Store);
  private readonly activeTenant = this.store.selectSignal(selectActiveTenant);
  private readonly user = signal<MeResponse | null>(null);

  readonly currentUser = this.user.asReadonly();
  readonly isPlatformUser = computed(() => this.user()?.platformRole != null);

  async load(preferredTenantId?: string): Promise<void> {
    try {
      const response = await firstValueFrom(this.api.get<MeResponse>('/me'));
      const data = response.data;
      this.user.set(data);

      if (data.platformRole == null && data.memberships.length > 0) {
        const activeTenantId = preferredTenantId ?? this.activeTenant()?.id;
        const first =
          activeTenantId == null
            ? data.memberships[0]
            : (data.memberships.find((membership) => membership.tenantId === activeTenantId) ??
              data.memberships[0]);
        const summary: TenantSummary = {
          id: first.tenantId,
          name: first.tenantName,
          slug: first.tenantSlug,
          status: 'active',
          plan: 'trial',
        };
        this.store.dispatch(tenantContextActions.setActiveTenant({ tenant: summary }));
      }
    } catch (error) {
      if (isUnauthenticated(error)) {
        this.clear();
        return;
      }
      throw error;
    }
  }

  clear(): void {
    this.user.set(null);
    this.store.dispatch(tenantContextActions.clearActiveTenant());
  }
}

const isUnauthenticated = (error: unknown): error is ApiError =>
  typeof error === 'object' &&
  error !== null &&
  (error as ApiError).status === 401 &&
  (error as ApiError).code === 'unauthenticated';
