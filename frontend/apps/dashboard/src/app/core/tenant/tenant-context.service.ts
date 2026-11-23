import { inject, Injectable, signal } from '@angular/core';
import { Store } from '@ngrx/store';
import { catchError, finalize, defer, firstValueFrom, map, Observable, throwError } from 'rxjs';
import { ApiService } from '../api/api.service';
import type { components } from '../api/generated/api';
import { TenantSummary, tenantSummaryFromWire } from '../api/tenant-api.models';
import { isApiError } from '../errors/api-error';
import {
  selectActiveTenant,
  selectStatus,
  tenantContextActions,
} from '../state/tenant-context.feature';

type Schemas = components['schemas'];
export type PlatformAccessSession = Schemas['PlatformAccessSession'];

@Injectable({ providedIn: 'root' })
export class TenantContextService {
  private store = inject(Store);
  private api = inject(ApiService);
  private readonly session = signal<PlatformAccessSession | null>(null);
  // A read of server state may finish after another switch, end or sign-out.
  private revision = 0;
  // Restores can overlap inside one revision; only the latest one may commit.
  private restoreCount = 0;
  private handledDenial = false;

  readonly activeTenant = this.store.selectSignal(selectActiveTenant);
  readonly status = this.store.selectSignal(selectStatus);
  readonly accessSession = this.session.asReadonly();

  /**
   * Switches to a workspace. Cold: nothing is sent until it is subscribed to, and unsubscribing
   * cancels the request. The store is told when it starts, succeeds and fails.
   */
  select$(
    tenantId: string,
    request: Schemas['StartPlatformAccessRequest'],
  ): Observable<TenantSummary> {
    return defer(() => {
      let answered = false;
      const revision = ++this.revision;
      this.store.dispatch(tenantContextActions.switchTenantRequested({ tenantId }));
      return this.api
        .post<Schemas['SwitchTenantResponse']>(`/platform/tenants/${tenantId}/switch`, request)
        .pipe(
          map(({ data }) => {
            answered = true;
            const tenant = tenantSummaryFromWire(data);
            if (this.revision === revision) {
              this.revision++;
              this.handledDenial = false;
              this.session.set({ ...data.accessSession, tenantName: tenant.name });
              this.store.dispatch(tenantContextActions.switchTenantSucceeded({ tenant }));
              void this.restoreAccess(tenant);
            }
            return tenant;
          }),
          catchError((error: unknown) => {
            answered = true;
            if (this.revision === revision)
              this.store.dispatch(tenantContextActions.switchTenantFailed());
            return throwError(() => error);
          }),
          // Unsubscribed before an answer: the store must not stay on "switching".
          finalize(() => {
            if (!answered && this.revision === revision)
              this.store.dispatch(tenantContextActions.switchTenantCancelled());
          }),
        );
    });
  }

  /** The same switch as a Promise, for the callers that are not reactive. */
  select(tenantId: string, request: Schemas['StartPlatformAccessRequest']): Promise<TenantSummary> {
    return firstValueFrom(this.select$(tenantId, request));
  }

  async restoreAccess(knownTenant = this.activeTenant()): Promise<void> {
    const revision = this.revision;
    // Restores can overlap (start-up, after a switch, after ending). Only the latest one may
    // commit, so a slow answer for an older session never replaces a newer session or tenant.
    const restore = ++this.restoreCount;
    const latest = () => this.revision === revision && this.restoreCount === restore;
    try {
      const { data: session } = await firstValueFrom(
        this.api.get<PlatformAccessSession>('/platform/access-session'),
      );
      if (!latest()) return;
      this.handledDenial = false;
      this.session.set(session);
      if (knownTenant?.id !== session.tenantId) {
        const activeTenant = this.activeTenant();
        if (activeTenant && activeTenant.id !== session.tenantId) {
          this.store.dispatch(tenantContextActions.clearActiveTenant());
        }
        const { data } = await firstValueFrom(
          this.api.get<Schemas['PlatformTenantDetail']>(`/platform/tenants/${session.tenantId}`),
        );
        if (!latest()) return;
        // The banner can expire the session while the tenant is being fetched; don't bring a
        // workspace back for access that has ended.
        const live = this.session();
        if (live?.id !== session.id || Date.parse(live.expiresAt) <= Date.now()) return;
        this.set(tenantSummaryFromWire(data));
      }
    } catch (error: unknown) {
      if (!latest()) return;
      if (isApiError(error) && error.status === 404) {
        if (this.session() || this.activeTenant()) this.clear();
      } else if (!this.session() && this.activeTenant()) {
        this.clear();
      }
    }
  }

  async endAccess(): Promise<void> {
    await firstValueFrom(this.api.post<void>('/platform/access-session/end', undefined));
    this.clear();
    void this.restoreAccess(null);
  }

  /**
   * Handles a tenant request the server refused for lack of access. `issuedSessionId` is the
   * access session the browser held when it sent the request: a refusal for an older session
   * must not end a newer one (another tab can start access for the same tenant).
   */
  rejectAccess(
    tenantId: string | null,
    expectedTenant: TenantSummary | null,
    issuedSessionId: string | null = null,
  ): boolean {
    if (this.handledDenial) return false;
    const active = this.activeTenant();
    if (active !== expectedTenant || (tenantId !== null && active?.id !== tenantId)) return false;
    if (issuedSessionId !== null && this.session()?.id !== issuedSessionId) return false;
    if (this.status() === 'switching') {
      // A denial for the old workspace can arrive while the new access is being started.
      this.session.set(null);
      this.store.dispatch(tenantContextActions.clearActiveTenant());
    } else {
      this.clear();
    }
    this.handledDenial = true;
    return true;
  }

  expireAccess(id: string): void {
    if (this.session()?.id === id && this.activeTenant()) {
      this.store.dispatch(tenantContextActions.clearActiveTenant());
    }
  }

  clear(): void {
    this.revision++;
    this.handledDenial = false;
    this.session.set(null);
    this.store.dispatch(tenantContextActions.clearActiveTenant());
  }

  set(tenant: TenantSummary): void {
    this.store.dispatch(tenantContextActions.setActiveTenant({ tenant }));
  }
}
