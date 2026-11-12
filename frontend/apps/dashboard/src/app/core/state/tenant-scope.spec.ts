import { Injector, provideZonelessChangeDetection, runInInjectionContext } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Action, provideStore, Store } from '@ngrx/store';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { TenantSummary } from '../api/tenant-api.models';
import { tenantContextActions, tenantContextFeature } from './tenant-context.feature';
import { TenantChange, TenantScope, withTenantScope } from './tenant-scope';

const tenant = (id: string): TenantSummary => ({
  id,
  name: `Tenant ${id}`,
  slug: id,
  status: 'active',
  plan: 'starter',
});

interface ListState {
  items: string[];
  status: 'idle' | 'loaded';
}
const initialState: ListState = { items: [], status: 'idle' };

const reloads: TenantChange[] = [];
const ListStore = signalStore(
  withState<ListState>(initialState),
  withMethods((store) => ({
    show(items: string[]): void {
      patchState(store, { items, status: 'loaded' });
    },
  })),
  withTenantScope({
    initialState,
    reload: (_store, change) => reloads.push(change),
  }),
);

describe('TenantScope and withTenantScope', () => {
  beforeEach(() => {
    reloads.length = 0;
    localStorage.clear();
    TestBed.configureTestingModule({
      providers: [
        provideZonelessChangeDetection(),
        provideStore({ [tenantContextFeature.name]: tenantContextFeature.reducer }),
        ListStore,
      ],
    });
  });

  const dispatch = (action: Action) => TestBed.inject(Store).dispatch(action);

  it('announces a change of workspace id, with the old and the new id', () => {
    const seen: TenantChange[] = [];
    TestBed.inject(TenantScope).changes$.subscribe((change) => seen.push(change));

    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    dispatch(tenantContextActions.switchTenantSucceeded({ tenant: tenant('b') }));
    dispatch(tenantContextActions.clearActiveTenant());

    expect(seen).toEqual([
      { from: null, to: 'a' },
      { from: 'a', to: 'b' },
      { from: 'b', to: null },
    ]);
  });

  it('stays quiet when the same workspace is set again', () => {
    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    const seen: TenantChange[] = [];
    TestBed.inject(TenantScope).changes$.subscribe((change) => seen.push(change));

    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    dispatch(tenantContextActions.switchTenantSucceeded({ tenant: tenant('a') }));

    expect(seen).toEqual([]);
  });

  it('resets a store in the same call as the switch, with no tick or effect flush in between', () => {
    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    const store = TestBed.inject(ListStore);
    store.show(['tenant A conversation']);
    expect(store.items()).toEqual(['tenant A conversation']);

    dispatch(tenantContextActions.switchTenantSucceeded({ tenant: tenant('b') }));

    expect(store.items()).toEqual([]);
    expect(store.status()).toBe('idle');
  });

  it('resets on clearActiveTenant and does not reload', () => {
    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    const store = TestBed.inject(ListStore);
    store.show(['x']);
    reloads.length = 0;

    dispatch(tenantContextActions.clearActiveTenant());

    expect(store.items()).toEqual([]);
    expect(reloads).toEqual([]);
  });

  it('reloads for the new workspace after the reset', () => {
    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    TestBed.inject(ListStore);

    dispatch(tenantContextActions.switchTenantSucceeded({ tenant: tenant('b') }));

    expect(reloads).toEqual([{ from: 'a', to: 'b' }]);
  });

  it('stops following a store that was destroyed', () => {
    dispatch(tenantContextActions.setActiveTenant({ tenant: tenant('a') }));
    const injector = Injector.create({ providers: [ListStore], parent: TestBed.inject(Injector) });
    runInInjectionContext(injector, () => injector.get(ListStore));
    (injector as Injector & { destroy(): void }).destroy();
    reloads.length = 0;

    dispatch(tenantContextActions.switchTenantSucceeded({ tenant: tenant('b') }));

    expect(reloads).toEqual([]);
  });
});
