import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { inject } from '@angular/core';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { PlatformSummary } from '../../../core/api/platform-analytics.models';
import { PlatformOverviewApiService } from './platform-overview-api.service';

export interface PlatformOverviewState {
  readonly summary: PlatformSummary | null;
  readonly loading: boolean;
  readonly error: ApiError | null;
  readonly from: string | null;
  readonly to: string | null;
}

export const PlatformOverviewStore = signalStore(
  withState<PlatformOverviewState>({
    summary: null,
    loading: false,
    error: null,
    from: null,
    to: null,
  }),
  withMethods((store, api = inject(PlatformOverviewApiService)) => {
    const load = rxMethod<{ from?: string; to?: string }>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap((query) =>
          api.getSummary(query).pipe(
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
        tap((summary) => {
          if (summary) {
            patchState(store, { summary, loading: false });
          }
        }),
      ),
    );

    return {
      load(query: { from?: string; to?: string }): void {
        load(query);
      },
      setRange(from: string | null, to: string | null): void {
        patchState(store, { from, to });
        load({ from: from ?? undefined, to: to ?? undefined });
      },
    };
  }),
);
