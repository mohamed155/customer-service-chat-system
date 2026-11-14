import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { inject } from '@angular/core';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { ProviderStatus } from '../../../core/api/platform-analytics.models';
import { AiProvidersApiService } from './ai-providers-api.service';

export interface AiProvidersState {
  readonly status: ProviderStatus | null;
  readonly window: '1h' | '24h' | '7d';
  readonly loading: boolean;
  readonly error: ApiError | null;
}

export const AiProvidersStore = signalStore(
  withState<AiProvidersState>({
    status: null,
    window: '24h',
    loading: false,
    error: null,
  }),
  withMethods((store, api = inject(AiProvidersApiService)) => {
    const load = rxMethod<'1h' | '24h' | '7d'>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap((window) =>
          api.getStatus(window).pipe(
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
        tap((status) => {
          if (status) {
            patchState(store, { status, loading: false });
          }
        }),
      ),
    );

    return {
      load(window: '1h' | '24h' | '7d'): void {
        load(window);
      },
      setWindow(window: '1h' | '24h' | '7d'): void {
        patchState(store, { window });
        load(window);
      },
    };
  }),
);
