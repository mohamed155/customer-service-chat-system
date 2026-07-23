import { effect, inject, untracked } from '@angular/core';
import { Store } from '@ngrx/store';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { IntegrationListItem } from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { IntegrationsApiService } from './integrations-api.service';

export interface IntegrationsState {
  readonly items: IntegrationListItem[];
  readonly loading: boolean;
  readonly error: string | null;
}

export const IntegrationsStore = signalStore(
  withState<IntegrationsState>({
    items: [],
    loading: false,
    error: null,
  }),
  withMethods((store, api = inject(IntegrationsApiService)) => {
    const load = rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null, items: [] })),
        switchMap(() =>
          api.list().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load integrations',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              items: result.items,
              loading: false,
            });
          }
        }),
      ),
    );

    return {
      load(): void {
        load();
      },
    };
  }),
  withHooks((store, globalStore = inject(Store, { optional: true })) => {
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    return {
      onInit(): void {
        store.load();
        effect(() => {
          if (activeTenant()?.id) {
            untracked(() => store.load());
          }
        });
      },
    };
  }),
);
