import { inject, Injectable } from '@angular/core';
import { Store } from '@ngrx/store';
import { catchError, firstValueFrom, map, Observable, switchMap, tap, throwError } from 'rxjs';
import { ApiService } from '../api/api.service';
import type { components } from '../api/generated/api';
import { TenantSummary, tenantSummaryFromWire } from '../api/tenant-api.models';
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

  /**
   * Switches to a workspace. Cold: nothing is sent until it is subscribed to, and unsubscribing
   * cancels the request. The store is told when it starts, succeeds and fails.
   */
  select$(tenantId: string): Observable<TenantSummary> {
    return new Observable<void>((subscriber) => {
      this.store.dispatch(tenantContextActions.switchTenantRequested({ tenantId }));
      subscriber.next();
      subscriber.complete();
    }).pipe(
      switchMap(() =>
        this.api.post<components['schemas']['TenantSummary']>(
          `/platform/tenants/${tenantId}/switch`,
          undefined,
        ),
      ),
      map((response) => tenantSummaryFromWire(response.data)),
      tap((tenant) => this.store.dispatch(tenantContextActions.switchTenantSucceeded({ tenant }))),
      catchError(() => {
        this.store.dispatch(tenantContextActions.switchTenantFailed());
        return throwError(() => new Error('Failed to switch tenant'));
      }),
    );
  }

  /** The same switch as a Promise, for the callers that are not reactive. */
  select(tenantId: string): Promise<TenantSummary> {
    return firstValueFrom(this.select$(tenantId));
  }

  clear(): void {
    this.store.dispatch(tenantContextActions.clearActiveTenant());
  }

  set(tenant: TenantSummary): void {
    this.store.dispatch(tenantContextActions.setActiveTenant({ tenant }));
  }
}
