import { computed, inject } from '@angular/core';
import { ApiError } from '../../../core/api/api.models';
import {
  CreateTenantPayload,
  PlatformTenantDetail,
  TenantStatus,
  TenantSummary,
  UpdateTenantPayload,
} from '../../../core/api/tenant-api.models';
import { patchState, signalStore, withComputed, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import {
  catchError,
  concatMap,
  debounceTime,
  distinctUntilChanged,
  EMPTY,
  map,
  Observable,
  of,
  pipe,
  Subject,
  switchMap,
  tap,
} from 'rxjs';
import { PlatformTenantsService } from './platform-tenants.service';

export type TenantListStatus = 'pending' | 'loadingMore' | 'success' | 'empty' | 'error';
export type TenantStatusFilter = TenantStatus | null;

interface TenantsState {
  readonly items: readonly TenantSummary[];
  readonly query: string;
  readonly statusFilter: TenantStatusFilter;
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
  readonly status: TenantListStatus;
  readonly error: ApiError | null;
  readonly loadMoreError: ApiError | null;
}

const PAGE_LIMIT = 25;
const SEARCH_DEBOUNCE_MS = 300;

const initialState: TenantsState = {
  items: [],
  query: '',
  statusFilter: null,
  nextCursor: null,
  hasMore: false,
  status: 'pending',
  error: null,
  loadMoreError: null,
};

interface QueryArgs {
  readonly query: string;
  readonly statusFilter: TenantStatusFilter;
  readonly cursor: string | null;
  readonly limit: number;
}

const buildListParams = (args: QueryArgs) => ({
  q: args.query || undefined,
  status: args.statusFilter ?? undefined,
  cursor: args.cursor ?? undefined,
  limit: args.limit,
});

interface CreateWrite {
  readonly payload: CreateTenantPayload;
  readonly result$: Subject<PlatformTenantDetail>;
}

interface UpdateWrite {
  readonly id: string;
  readonly payload: UpdateTenantPayload;
  readonly result$: Subject<PlatformTenantDetail>;
}

export const TenantsStore = signalStore(
  { providedIn: 'root' },
  withState(initialState),
  withComputed((store) => ({
    loading: computed(() => store.status() === 'pending'),
    loadingMore: computed(() => store.status() === 'loadingMore'),
  })),
  withMethods((store, service = inject(PlatformTenantsService)) => {
    const reloadArgs = (): QueryArgs => ({
      query: store.query(),
      statusFilter: store.statusFilter(),
      cursor: null,
      limit: PAGE_LIMIT,
    });

    const _reload = rxMethod<QueryArgs>(
      pipe(
        switchMap((args) => {
          patchState(store, { status: 'pending', error: null, loadMoreError: null });
          return service.list(buildListParams(args)).pipe(
            tap((response) => {
              const data = response.data;
              patchState(store, {
                items: data.items,
                nextCursor: data.nextCursor,
                hasMore: data.hasMore,
                status: data.items.length === 0 ? 'empty' : 'success',
              });
            }),
            catchError((error: unknown) => {
              patchState(store, { status: 'error', error: error as ApiError });
              return EMPTY;
            }),
          );
        }),
      ),
    );

    const _loadMore = rxMethod<QueryArgs>(
      pipe(
        switchMap((args) => {
          if (!args.cursor) return EMPTY;
          patchState(store, { status: 'loadingMore' });
          return service.list(buildListParams(args)).pipe(
            tap((response) => {
              const data = response.data;
              patchState(store, {
                items: [...store.items(), ...data.items],
                nextCursor: data.nextCursor,
                hasMore: data.hasMore,
                status: 'success',
                loadMoreError: null,
              });
            }),
            catchError((error: unknown) => {
              patchState(store, { status: 'success', loadMoreError: error as ApiError });
              return EMPTY;
            }),
          );
        }),
      ),
    );

    const _setQueryDebounced = rxMethod<string>(
      pipe(
        debounceTime(SEARCH_DEBOUNCE_MS),
        distinctUntilChanged(),
        concatMap((query) => {
          const currentQuery = store.query();
          if (currentQuery !== query) return EMPTY;
          return of(query).pipe(
            tap(() =>
              _reload({
                query,
                statusFilter: store.statusFilter(),
                cursor: null,
                limit: PAGE_LIMIT,
              }),
            ),
          );
        }),
      ),
    );

    // Ordered write pipelines.
    //
    // Each write (create or update) is queued onto a dedicated Subject and
    // processed via `concatMap`, which guarantees:
    //   * Writes run sequentially; a second call while a first is in flight
    //     does not cancel the first, and both eventually resolve in order.
    //   * Exactly one HTTP request per queued write.
    //
    // The public `create()` / `update()` methods return a cold Observable
    // that subscribes the caller to a per-call `Subject<PlatformTenantDetail>`.
    // The write pipeline resolves that subject exactly once (with `next` on
    // success or `error` on failure) and never leaves it stranded, unlike
    // the previous `switchMap` + `ReplaySubject` design where a canceled
    // inner observable would silently drop the caller's subscription.
    //
    // The list reload is a side effect inside the pipeline (via `tap`) and
    // is decoupled from the caller's Observable so subscribers only see the
    // write's outcome.

    const _createQueue$ = new Subject<CreateWrite>();

    _createQueue$
      .pipe(
        concatMap(({ payload, result$ }) =>
          service.create(payload).pipe(
            map((response) => response.data),
            tap((detail) => {
              _reload(reloadArgs());
              result$.next(detail);
              result$.complete();
            }),
            catchError((error: unknown) => {
              result$.error(error as ApiError);
              return EMPTY;
            }),
          ),
        ),
      )
      .subscribe();

    const _updateQueue$ = new Subject<UpdateWrite>();

    _updateQueue$
      .pipe(
        concatMap(({ id, payload, result$ }) =>
          service.update(id, payload).pipe(
            map((response) => response.data),
            tap((detail) => {
              _reload(reloadArgs());
              result$.next(detail);
              result$.complete();
            }),
            catchError((error: unknown) => {
              result$.error(error as ApiError);
              return EMPTY;
            }),
          ),
        ),
      )
      .subscribe();

    const createWriteObservable = (
      payload: CreateTenantPayload,
    ): Observable<PlatformTenantDetail> =>
      new Observable<PlatformTenantDetail>((subscriber) => {
        const result$ = new Subject<PlatformTenantDetail>();
        const sub = result$.subscribe(subscriber);
        _createQueue$.next({ payload, result$ });
        return () => {
          sub.unsubscribe();
        };
      });

    const updateWriteObservable = (
      id: string,
      payload: UpdateTenantPayload,
    ): Observable<PlatformTenantDetail> =>
      new Observable<PlatformTenantDetail>((subscriber) => {
        const result$ = new Subject<PlatformTenantDetail>();
        const sub = result$.subscribe(subscriber);
        _updateQueue$.next({ id, payload, result$ });
        return () => {
          sub.unsubscribe();
        };
      });

    return {
      load(): void {
        patchState(store, {
          items: [],
          query: '',
          statusFilter: null,
          nextCursor: null,
          hasMore: false,
          status: 'pending',
          error: null,
          loadMoreError: null,
        });
        _reload({ query: '', statusFilter: null, cursor: null, limit: PAGE_LIMIT });
      },
      getDetail(id: string): Observable<PlatformTenantDetail> {
        return service.get(id).pipe(map((response) => response.data));
      },
      setQueryInput(query: string): void {
        patchState(store, { query });
        _setQueryDebounced(query);
      },
      setStatusFilter(statusFilter: TenantStatusFilter): void {
        patchState(store, {
          statusFilter,
          items: [],
          nextCursor: null,
          hasMore: false,
        });
        _reload({
          query: store.query(),
          statusFilter,
          cursor: null,
          limit: PAGE_LIMIT,
        });
      },
      resetFilters(): void {
        patchState(store, {
          items: [],
          query: '',
          statusFilter: null,
          nextCursor: null,
          hasMore: false,
          loadMoreError: null,
        });
        _reload({ query: '', statusFilter: null, cursor: null, limit: PAGE_LIMIT });
      },
      loadMore(): void {
        const cursor = store.nextCursor();
        if (!cursor) return;
        _loadMore({
          query: store.query(),
          statusFilter: store.statusFilter(),
          cursor,
          limit: PAGE_LIMIT,
        });
      },
      create(payload: CreateTenantPayload): Observable<PlatformTenantDetail> {
        return createWriteObservable(payload);
      },
      update(id: string, payload: UpdateTenantPayload): Observable<PlatformTenantDetail> {
        return updateWriteObservable(id, payload);
      },
    };
  }),
);
