import { TenantSummary } from '../api/tenant-api.models';
import {
  createInitialTenantContextState,
  tenantContextActions,
  tenantContextFeature,
} from './tenant-context.feature';

const mockTenant: TenantSummary = {
  id: 'tenant-1',
  name: 'Test Corp',
  slug: 'test-corp',
  status: 'active',
};

describe('TenantContext feature', () => {
  beforeEach(() => localStorage.clear());

  it('starts with idle and null tenant', () => {
    const state = createInitialTenantContextState();
    expect(state.activeTenant).toBeNull();
    expect(state.status).toBe('idle');
  });

  it('restores valid tenant from localStorage', () => {
    localStorage.setItem('app.tenant', JSON.stringify(mockTenant));
    const state = createInitialTenantContextState();
    expect(state.activeTenant).toEqual(mockTenant);
  });

  it('discards invalid localStorage data', () => {
    localStorage.setItem('app.tenant', '{"id":"only-id"}');
    expect(createInitialTenantContextState().activeTenant).toBeNull();
  });

  it('discards malformed JSON', () => {
    localStorage.setItem('app.tenant', '{bad json}');
    expect(createInitialTenantContextState().activeTenant).toBeNull();
  });

  it('handles setActiveTenant', () => {
    let state = createInitialTenantContextState();
    state = tenantContextFeature.reducer(
      state,
      tenantContextActions.setActiveTenant({ tenant: mockTenant }),
    );
    expect(state.activeTenant).toEqual(mockTenant);
    expect(state.status).toBe('idle');
  });

  it('handles clearActiveTenant', () => {
    let state = tenantContextFeature.reducer(
      createInitialTenantContextState(),
      tenantContextActions.setActiveTenant({ tenant: mockTenant }),
    );
    state = tenantContextFeature.reducer(state, tenantContextActions.clearActiveTenant());
    expect(state.activeTenant).toBeNull();
    expect(state.status).toBe('idle');
  });

  it('handles switchTenantRequested', () => {
    const state = tenantContextFeature.reducer(
      createInitialTenantContextState(),
      tenantContextActions.switchTenantRequested({ tenantId: 'tenant-2' }),
    );
    expect(state.status).toBe('switching');
  });

  it('handles switchTenantSucceeded', () => {
    const newTenant: TenantSummary = {
      id: 'tenant-2',
      name: 'New Co',
      slug: 'new-co',
      status: 'active',
    };
    let state = tenantContextFeature.reducer(
      createInitialTenantContextState(),
      tenantContextActions.switchTenantRequested({ tenantId: 'tenant-2' }),
    );
    state = tenantContextFeature.reducer(
      state,
      tenantContextActions.switchTenantSucceeded({ tenant: newTenant }),
    );
    expect(state.activeTenant).toEqual(newTenant);
    expect(state.status).toBe('idle');
  });

  it('handles switchTenantFailed', () => {
    let state = tenantContextFeature.reducer(
      createInitialTenantContextState(),
      tenantContextActions.switchTenantRequested({ tenantId: 'x' }),
    );
    state = tenantContextFeature.reducer(state, tenantContextActions.switchTenantFailed());
    expect(state.status).toBe('error');
  });

  it('selects expected slices', () => {
    const root = {
      tenantContext: {
        activeTenant: mockTenant,
        status: 'idle' as const,
      },
    };
    expect(tenantContextFeature.selectActiveTenant(root)).toEqual(mockTenant);
    expect(tenantContextFeature.selectStatus(root)).toBe('idle');
  });
});
