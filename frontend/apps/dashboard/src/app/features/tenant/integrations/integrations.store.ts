import { inject } from '@angular/core';
import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { IntegrationListItem } from '../../../core/api/tenant-api.models';
import { IntegrationsApiService } from './integrations-api.service';
import { withTenantScope } from '../../../core/state/tenant-scope';

export interface IntegrationsState {
  readonly items: IntegrationListItem[];
  readonly loading: boolean;
  readonly error: ApiError | null;
}

const initialState: IntegrationsState = {
  items: [],
  loading: false,
  error: null,
};

export const IntegrationsStore = signalStore(
  withState<IntegrationsState>(initialState),
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
                error: toApiError(err),
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
  withHooks((store) => ({
    onInit(): void {
      store.load();
    },
  })),
  withTenantScope({
    initialState,
    reload: (store) => store.load(),
  }),
);
