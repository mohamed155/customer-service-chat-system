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
import { catchError, debounceTime, distinctUntilChanged, EMPTY, pipe, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { Customer } from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { CustomersApiService } from './customers-api.service';

type CustomersStatus = 'pending' | 'loading' | 'success' | 'empty' | 'error';

interface CustomersState {
  readonly items: readonly Customer[];
  readonly query: string;
  readonly searchGeneration: number;
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
  readonly status: CustomersStatus;
  readonly error: ApiError | null;
  readonly loadMoreError: ApiError | null;
}

interface FetchArgs {
  readonly query: string;
  readonly cursor: string | null;
  readonly append: boolean;
}

interface SearchArgs {
  readonly query: string;
  readonly generation: number;
}

const PAGE_LIMIT = 25;
const SEARCH_DEBOUNCE_MS = 300;

const initialState: CustomersState = {
  items: [],
  query: '',
  searchGeneration: 0,
  nextCursor: null,
  hasMore: false,
  status: 'pending',
  error: null,
  loadMoreError: null,
};

export const CustomersStore = signalStore(
  { providedIn: 'root' },
  withState(initialState),
  withComputed((store) => ({
    loading: computed(() => store.status() === 'loading'),
  })),
  withMethods((store, api = inject(CustomersApiService)) => {
    const fetch = rxMethod<FetchArgs>(
      pipe(
        tap(({ append }) =>
          patchState(store, {
            status: 'loading',
            error: null,
            loadMoreError: null,
            ...(append ? {} : { items: [], nextCursor: null, hasMore: false }),
          }),
        ),
        switchMap(({ query, cursor, append }) =>
          api.list({ q: query || undefined, limit: PAGE_LIMIT }, cursor ?? undefined).pipe(
            tap((response) => {
              const { items, nextCursor, hasMore } = response.data;
              patchState(store, {
                items: append ? [...store.items(), ...items] : items,
                nextCursor,
                hasMore,
                status: items.length === 0 && !append ? 'empty' : 'success',
                loadMoreError: null,
              });
            }),
            catchError((error: unknown) => {
              patchState(
                store,
                append
                  ? {
                      status: store.items().length === 0 ? 'empty' : 'success',
                      loadMoreError: error as ApiError,
                    }
                  : { status: 'error', error: error as ApiError },
              );
              return EMPTY;
            }),
          ),
        ),
      ),
    );

    const searchInput = rxMethod<SearchArgs>(
      pipe(
        debounceTime(SEARCH_DEBOUNCE_MS),
        distinctUntilChanged(
          (previous, current) =>
            previous.query === current.query && previous.generation === current.generation,
        ),
        tap(({ query, generation }) => {
          if (store.query() === query && store.searchGeneration() === generation) {
            fetch({ query, cursor: null, append: false });
          }
        }),
      ),
    );

    return {
      load(): void {
        patchState(store, {
          query: '',
          searchGeneration: store.searchGeneration() + 1,
          items: [],
          nextCursor: null,
          hasMore: false,
          loadMoreError: null,
        });
        fetch({ query: '', cursor: null, append: false });
      },
      search(query: string): void {
        patchState(store, { query });
        searchInput({ query, generation: store.searchGeneration() });
      },
      loadMore(): void {
        const cursor = store.nextCursor();
        if (!cursor || !store.hasMore()) return;
        fetch({ query: store.query(), cursor, append: true });
      },
      retry(): void {
        const cursor = store.nextCursor();
        if (store.loadMoreError() && cursor && store.hasMore()) {
          fetch({ query: store.query(), cursor, append: true });
          return;
        }
        fetch({ query: store.query(), cursor: null, append: false });
      },
    };
  }),
  withHooks((store) => {
    const globalStore = inject(Store, { optional: true });
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    let initialized = false;
    let previousTenantId: string | null = null;

    effect(() => {
      const tenantId = activeTenant()?.id ?? null;
      if (!initialized) {
        initialized = true;
        previousTenantId = tenantId;
        return;
      }
      if (tenantId === previousTenantId) return;

      previousTenantId = tenantId;
      store.load();
    });

    return {
      onInit(): void {
        store.load();
      },
    };
  }),
);
