import { inject } from '@angular/core';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { BillingOverview } from '../../../core/api/tenant-api.models';
import { toApiError } from '../../../core/errors/api-error';
import { BillingApiService } from './billing-api.service';
import { withTenantScope } from '../../../core/state/tenant-scope';

export interface BillingState {
  readonly overview: BillingOverview | null;
  readonly loading: boolean;
  readonly error: ApiError | null;
}

const initialState: BillingState = {
  overview: null,
  loading: false,
  error: null,
};

export const BillingStore = signalStore(
  withState<BillingState>(initialState),
  withMethods((store, api = inject(BillingApiService)) => {
    const load = rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null, overview: null })),
        switchMap(() =>
          api.getOverview().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, { loading: false, error: toApiError(err) });
              return of(null);
            }),
          ),
        ),
        tap((overview) => {
          if (overview) {
            patchState(store, { overview, loading: false });
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
  withTenantScope({
    initialState,
    loadOnInit: true,
    reload: (store) => store.load(),
  }),
);
