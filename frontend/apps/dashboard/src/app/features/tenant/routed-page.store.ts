import { computed, effect, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { selectActiveTenant } from '../../core/state/tenant-context.feature';
import { PAGE_ROUTE, PagePayload, RoutedPageDataService } from './routed-page-data.service';

export { PAGE_ROUTE };

export type PageLifecycle<T> =
  | { status: 'pending' }
  | { status: 'data'; data: T }
  | { status: 'empty' }
  | { status: 'error'; error: unknown };

export const RoutedPageStore = signalStore(
  withState({
    lifecycle: { status: 'pending' } as PageLifecycle<PagePayload>,
  }),
  withComputed((store) => ({
    loading: computed(() => {
      const lc = store.lifecycle();
      return lc.status === 'pending';
    }),
    data: computed(() => {
      const lc = store.lifecycle();
      return lc.status === 'data' ? lc.data : undefined;
    }),
    error: computed(() => {
      const lc = store.lifecycle();
      return lc.status === 'error' ? lc.error : null;
    }),
  })),
  withMethods(
    (
      store,
      dataService = inject(RoutedPageDataService),
      pageRoute = inject(PAGE_ROUTE),
      globalStore = inject(Store, { optional: true }),
    ) => {
      const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);

      const _load = rxMethod<string | null>(
        pipe(
          tap(() =>
            patchState(store, { lifecycle: { status: 'pending' } as PageLifecycle<PagePayload> }),
          ),
          switchMap((tenantId) =>
            dataService.load(pageRoute, tenantId).pipe(
              map((result) =>
                result == null
                  ? ({ status: 'empty' } as PageLifecycle<PagePayload>)
                  : ({ status: 'data', data: result } as PageLifecycle<PagePayload>),
              ),
              catchError((err: unknown) =>
                of({ status: 'error', error: err } as PageLifecycle<PagePayload>),
              ),
            ),
          ),
          tap((newState) => patchState(store, { lifecycle: newState })),
        ),
      );

      return {
        load(tenantId: string | null): void {
          _load(tenantId);
        },
        retry(): void {
          _load(activeTenant()?.id ?? null);
        },
      };
    },
  ),
  withHooks((store, globalStore = inject(Store, { optional: true })) => {
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    return {
      onInit(): void {
        effect(() => store.load(activeTenant()?.id ?? null));
      },
    };
  }),
);
