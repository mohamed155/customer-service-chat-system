import {
  createActionGroup,
  createFeature,
  createReducer,
  emptyProps,
  on,
  props,
} from '@ngrx/store';
import { TenantSummary } from '../api/tenant-api.models';

export interface TenantContextState {
  readonly activeTenant: TenantSummary | null;
  readonly status: 'idle' | 'switching' | 'error';
}

const isTenantSummary = (value: unknown): value is TenantSummary =>
  typeof value === 'object' &&
  value !== null &&
  typeof (value as Record<string, unknown>)['id'] === 'string' &&
  typeof (value as Record<string, unknown>)['name'] === 'string' &&
  typeof (value as Record<string, unknown>)['slug'] === 'string' &&
  ((value as Record<string, unknown>)['status'] === 'active' ||
    (value as Record<string, unknown>)['status'] === 'suspended');

export const createInitialTenantContextState = (): TenantContextState => {
  const stored =
    typeof localStorage !== 'undefined' && typeof localStorage.getItem === 'function'
      ? localStorage.getItem('app.tenant')
      : null;
  if (stored !== null) {
    try {
      const parsed = JSON.parse(stored) as unknown;
      if (isTenantSummary(parsed)) {
        return { activeTenant: parsed, status: 'idle' };
      }
    } catch {}
  }
  return { activeTenant: null, status: 'idle' };
};

export const tenantContextActions = createActionGroup({
  source: 'Tenant Context',
  events: {
    'Set Active Tenant': props<{ tenant: TenantSummary }>(),
    'Clear Active Tenant': emptyProps(),
    'Switch Tenant Requested': props<{ tenantId: string }>(),
    'Switch Tenant Succeeded': props<{ tenant: TenantSummary }>(),
    'Switch Tenant Failed': emptyProps(),
  },
});

export const tenantContextFeature = createFeature({
  name: 'tenantContext',
  reducer: createReducer(
    createInitialTenantContextState(),
    on(tenantContextActions.setActiveTenant, (state, { tenant }) => ({
      ...state,
      activeTenant: tenant,
      status: 'idle' as const,
    })),
    on(tenantContextActions.clearActiveTenant, (state) => ({
      ...state,
      activeTenant: null,
      status: 'idle' as const,
    })),
    on(tenantContextActions.switchTenantRequested, (state) => ({
      ...state,
      status: 'switching' as const,
    })),
    on(tenantContextActions.switchTenantSucceeded, (state, { tenant }) => ({
      ...state,
      activeTenant: tenant,
      status: 'idle' as const,
    })),
    on(tenantContextActions.switchTenantFailed, (state) => ({
      ...state,
      status: 'error' as const,
    })),
  ),
});

export const { selectActiveTenant, selectStatus } = tenantContextFeature;
