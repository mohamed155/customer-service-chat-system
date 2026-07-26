import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { SystemHealth } from '../../../core/api/platform-analytics.models';
import { SystemHealthApiService } from './system-health-api.service';

export interface SystemHealthState {
  readonly health: SystemHealth | null;
  readonly loading: boolean;
  readonly error: string | null;
}

export const SystemHealthStore = signalStore(
  withState<SystemHealthState>({
    health: null,
    loading: false,
    error: null,
  }),
  withMethods((store, api = inject(SystemHealthApiService)) => {
    const load = rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          api.getHealth().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load system health',
              });
              return of(null);
            }),
          ),
        ),
        tap((health) => {
          if (health) {
            patchState(store, { health, loading: false });
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
  withHooks((store) => {
    let intervalId: ReturnType<typeof setInterval> | undefined;

    return {
      onInit(): void {
        store.load();
        intervalId = setInterval(() => store.load(), 30_000);
      },
      onDestroy(): void {
        if (intervalId) clearInterval(intervalId);
      },
    };
  }),
);
