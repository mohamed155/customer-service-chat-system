import { inject } from '@angular/core';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { BillingOverview } from '../../../core/api/tenant-api.models';
import { BillingApiService } from './billing-api.service';
import { withTenantScope } from '../../../core/state/tenant-scope';

export interface BillingState {
  readonly overview: BillingOverview | null;
  readonly loading: boolean;
  readonly error: string | null;
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
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load billing overview',
              });
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
