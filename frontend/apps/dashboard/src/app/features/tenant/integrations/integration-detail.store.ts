import { effect, inject, untracked } from '@angular/core';
import { Store } from '@ngrx/store';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { IntegrationDetail, IntegrationEvent } from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { IntegrationConfigPayload, IntegrationsApiService } from './integrations-api.service';

export interface IntegrationDetailState {
  readonly detail: IntegrationDetail | null;
  readonly loading: boolean;
  readonly saving: boolean;
  readonly error: string | null;
  readonly events: readonly IntegrationEvent[];
  readonly eventsLoading: boolean;
  readonly eventsCursor: string | null;
  readonly eventsHasMore: boolean;
  readonly eventsError: string | null;
}

export const IntegrationDetailStore = signalStore(
  withState<IntegrationDetailState>({
    detail: null,
    loading: false,
    saving: false,
    error: null,
    events: [],
    eventsLoading: false,
    eventsCursor: null,
    eventsHasMore: false,
    eventsError: null,
  }),
  withMethods((store, api = inject(IntegrationsApiService)) => {
    const load = rxMethod<string>(
      pipe(
        tap(() =>
          patchState(store, {
            loading: true,
            error: null,
            detail: null,
            events: [],
            eventsCursor: null,
            eventsHasMore: false,
            eventsError: null,
          }),
        ),
        switchMap((slug) =>
          api.detail(slug).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load integration',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, { detail: result, loading: false });
          }
        }),
      ),
    );

    const connect = rxMethod<{ slug: string; payload: IntegrationConfigPayload }>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap(({ slug, payload }) =>
          api.connect(slug, payload).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to connect integration',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, { detail: result, saving: false });
          }
        }),
      ),
    );

    const updateConfig = rxMethod<{ slug: string; payload: IntegrationConfigPayload }>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap(({ slug, payload }) =>
          api.updateConfig(slug, payload).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to update integration config',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, { detail: result, saving: false });
          }
        }),
      ),
    );

    const disconnect = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap((slug) =>
          api.disconnect(slug).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to disconnect integration',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, { detail: result, saving: false });
          }
        }),
      ),
    );

    const loadFirstPageEvents = rxMethod<string>(
      pipe(
        tap(() =>
          patchState(store, {
            eventsLoading: true,
            eventsError: null,
            events: [],
            eventsCursor: null,
            eventsHasMore: false,
          }),
        ),
        switchMap((slug) =>
          api.events(slug, null).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                eventsLoading: false,
                eventsError: (err as Error)?.message ?? 'Failed to load events',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              events: result.data,
              eventsCursor: result.pagination.nextCursor,
              eventsHasMore: result.pagination.hasMore,
              eventsLoading: false,
            });
          }
        }),
      ),
    );

    const loadMoreEvents = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { eventsLoading: true, eventsError: null })),
        switchMap((slug) => {
          const cursor = store.eventsCursor();
          if (!cursor) {
            patchState(store, { eventsLoading: false });
            return of(null);
          }
          return api.events(slug, cursor).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                eventsLoading: false,
                eventsError: (err as Error)?.message ?? 'Failed to load more events',
              });
              return of(null);
            }),
          );
        }),
        tap((result) => {
          if (result) {
            patchState(store, {
              events: [...store.events(), ...result.data],
              eventsCursor: result.pagination.nextCursor,
              eventsHasMore: result.pagination.hasMore,
              eventsLoading: false,
            });
          }
        }),
      ),
    );

    return {
      load(slug: string): void {
        load(slug);
      },
      connect(slug: string, payload: IntegrationConfigPayload): void {
        connect({ slug, payload });
      },
      updateConfig(slug: string, payload: IntegrationConfigPayload): void {
        updateConfig({ slug, payload });
      },
      disconnect(slug: string): void {
        disconnect(slug);
      },
      loadFirstPageEvents(slug: string): void {
        loadFirstPageEvents(slug);
      },
      loadMoreEvents(slug: string): void {
        loadMoreEvents(slug);
      },
    };
  }),
  withHooks((store, globalStore = inject(Store, { optional: true })) => {
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    return {
      onInit(): void {
        effect(() => {
          if (activeTenant()?.id) {
            untracked(() => store.load(''));
          }
        });
      },
    };
  }),
);
