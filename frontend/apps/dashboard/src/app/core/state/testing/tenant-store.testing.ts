import { EnvironmentProviders } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideStore, Store } from '@ngrx/store';
import { TenantSummary } from '../../api/tenant-api.models';
import { tenantContextActions, tenantContextFeature } from '../tenant-context.feature';

/** A workspace for tests. */
export const testTenant = (id: string, name = `Tenant ${id}`): TenantSummary => ({
  id,
  name,
  slug: id,
  status: 'active',
  plan: 'starter',
});

/**
 * The real tenant-context store, for a spec that needs the active workspace to change the way
 * it does in the app. Add it to the providers, then drive it with `tenantStore()`.
 */
export const provideTenantStore = (): EnvironmentProviders =>
  provideStore({ [tenantContextFeature.name]: tenantContextFeature.reducer });

/** Moves the active workspace through the real actions. Call it after TestBed is configured. */
export function tenantStore() {
  const store = TestBed.inject(Store);
  return {
    /** The workspace is active without a switch, like the one restored after a page load. */
    activate: (tenant: TenantSummary | null) =>
      tenant === null
        ? store.dispatch(tenantContextActions.clearActiveTenant())
        : store.dispatch(tenantContextActions.setActiveTenant({ tenant })),
    switchTo: (tenant: TenantSummary) =>
      store.dispatch(tenantContextActions.switchTenantSucceeded({ tenant })),
    clear: () => store.dispatch(tenantContextActions.clearActiveTenant()),
  };
}
