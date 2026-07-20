import { computed, inject } from '@angular/core';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { NotificationEntry } from '../api/tenant-api.models';
import { NotificationsApiService } from './notifications.api';

interface NotificationsState {
  readonly items: NotificationEntry[];
  readonly unreadCount: number;
  readonly loading: boolean;
  readonly loadingMore: boolean;
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
}

const initial: NotificationsState = {
  items: [],
  unreadCount: 0,
  loading: false,
  loadingMore: false,
  nextCursor: null,
  hasMore: false,
};

export const NotificationsStore = signalStore(
  { providedIn: 'root' },
  withState<NotificationsState>(initial),
  withComputed((store) => ({
    unreadItems: computed(() => store.items().filter((i) => i.state === 'unread')),
  })),
  withMethods((store, api = inject(NotificationsApiService)) => {
    const loadFirstPage = rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, items: [], nextCursor: null })),
        switchMap(() =>
          api.list().pipe(
            map(({ data }) => data),
            catchError(() => {
              patchState(store, { loading: false });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              items: result.items,
              nextCursor: result.nextCursor,
              hasMore: result.hasMore,
              loading: false,
            });
          }
        }),
      ),
    );

    const loadMore = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { loadingMore: true })),
        switchMap((cursor) =>
          api.list(undefined, cursor).pipe(
            map(({ data }) => data),
            catchError(() => {
              patchState(store, { loadingMore: false });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              items: [...store.items(), ...result.items],
              nextCursor: result.nextCursor,
              hasMore: result.hasMore,
              loadingMore: false,
            });
          }
        }),
      ),
    );

    return {
      loadFirstPage(): void {
        loadFirstPage();
      },
      loadMore(): void {
        const cursor = store.nextCursor();
        if (!cursor || !store.hasMore()) return;
        loadMore(cursor);
      },
      refreshUnreadCount(): void {
        api.unreadCount().subscribe({
          next: ({ data }) => patchState(store, { unreadCount: data.count }),
        });
      },
      markRead(id: string): void {
        api.markRead(id).subscribe({
          next: ({ data }) => {
            patchState(store, {
              items: store.items().map((i) => (i.id === id ? data : i)),
            });
            api.unreadCount().subscribe({
              next: (res) => patchState(store, { unreadCount: res.data.count }),
            });
          },
        });
      },
      markAllRead(): void {
        api.markAllRead().subscribe({
          next: () => {
            patchState(store, {
              items: store
                .items()
                .map((i) =>
                  i.state === 'unread'
                    ? { ...i, state: 'read', readAt: new Date().toISOString() }
                    : i,
                ),
            });
            api.unreadCount().subscribe({
              next: (res) => patchState(store, { unreadCount: res.data.count }),
            });
          },
        });
      },
      setUnreadCount(n: number): void {
        patchState(store, { unreadCount: n });
      },
    };
  }),
  withHooks((store) => ({
    onInit() {
      store.refreshUnreadCount();
    },
  })),
);
