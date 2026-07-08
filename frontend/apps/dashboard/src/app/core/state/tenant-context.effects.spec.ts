import { TestBed } from '@angular/core/testing';
import { Actions } from '@ngrx/effects';
import { Subject } from 'rxjs';
import { TenantSummary } from '../api/tenant-api.models';
import { TenantContextEffects } from './tenant-context.effects';
import { tenantContextActions } from './tenant-context.feature';

const mockTenant: TenantSummary = {
  id: 'tenant-1',
  name: 'Test Corp',
  slug: 'test-corp',
  status: 'active',
};

describe('TenantContextEffects', () => {
  beforeEach(() => localStorage.clear());

  it('persists active tenant on setActiveTenant', () => {
    const actions = new Subject<ReturnType<typeof tenantContextActions.setActiveTenant>>();
    TestBed.configureTestingModule({
      providers: [TenantContextEffects, { provide: Actions, useValue: actions }],
    });
    TestBed.inject(TenantContextEffects).persistActiveTenant.subscribe();
    actions.next(tenantContextActions.setActiveTenant({ tenant: mockTenant }));
    expect(localStorage.getItem('app.tenant')).toBe(JSON.stringify(mockTenant));
  });

  it('persists active tenant on switchTenantSucceeded', () => {
    const actions = new Subject<ReturnType<typeof tenantContextActions.switchTenantSucceeded>>();
    TestBed.configureTestingModule({
      providers: [TenantContextEffects, { provide: Actions, useValue: actions }],
    });
    TestBed.inject(TenantContextEffects).persistActiveTenant.subscribe();
    actions.next(tenantContextActions.switchTenantSucceeded({ tenant: mockTenant }));
    expect(localStorage.getItem('app.tenant')).toBe(JSON.stringify(mockTenant));
  });

  it('clears persisted tenant on clearActiveTenant', () => {
    localStorage.setItem('app.tenant', JSON.stringify(mockTenant));
    const actions = new Subject<ReturnType<typeof tenantContextActions.clearActiveTenant>>();
    TestBed.configureTestingModule({
      providers: [TenantContextEffects, { provide: Actions, useValue: actions }],
    });
    TestBed.inject(TenantContextEffects).clearPersistedTenant.subscribe();
    actions.next(tenantContextActions.clearActiveTenant());
    expect(localStorage.getItem('app.tenant')).toBeNull();
  });
});
