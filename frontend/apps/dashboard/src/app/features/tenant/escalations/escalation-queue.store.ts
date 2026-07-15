import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { QueueEntry } from '../../../core/api/tenant-api.models';
import { EscalationsApiService } from './escalations-api.service';
import { RealtimeService } from '../../../core/realtime/realtime.service';

interface EscalationQueueState {
  items: QueueEntry[];
  loading: boolean;
  cursor: string | null;
  hasMore: boolean;
  error: string | null;
}

const initialState: EscalationQueueState = {
  items: [],
  loading: false,
  cursor: null,
  hasMore: false,
  error: null,
};

export const EscalationQueueStore = signalStore(
  withState(initialState),
  withMethods((store, api = inject(EscalationsApiService)) => ({
    loadQueue: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          api.listQueue().pipe(
            tap({
              next: (res) =>
                patchState(store, {
                  items: res.data.items,
                  cursor: res.data.nextCursor,
                  hasMore: res.data.hasMore,
                  loading: false,
                }),
              error: () => patchState(store, { loading: false, error: 'Failed to load queue' }),
            }),
          ),
        ),
      ),
    ),
    loadMore: rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true })),
        switchMap(() => {
          const cursor = store.cursor();
          if (!cursor) {
            patchState(store, { loading: false });
            return [];
          }
          return api.listQueue({ cursor }).pipe(
            tap({
              next: (res) =>
                patchState(store, {
                  items: [...store.items(), ...res.data.items],
                  cursor: res.data.nextCursor,
                  hasMore: res.data.hasMore,
                  loading: false,
                }),
              error: () => patchState(store, { loading: false, error: 'Failed to load more' }),
            }),
          );
        }),
      ),
    ),
    claim(id: string): void {
      const previous = store.items();
      patchState(store, {
        items: previous.filter((item) => item.escalation.id !== id),
      });
      api.claim(id).subscribe({
        error: () => {
          patchState(store, {
            items: previous,
            error: 'This escalation was already claimed by another agent.',
          });
        },
      });
    },
  })),
  withHooks({
    onInit(store, realtime = inject(RealtimeService)) {
      store.loadQueue();

      realtime.events().subscribe((event) => {
        if (event.event === 'escalation.queued') {
          store.loadQueue();
        }
        if (event.event === 'escalation.assigned' || event.event === 'escalation.removed') {
          const data = JSON.parse(event.data) as { escalationId?: string };
          if (data.escalationId) {
            patchState(store, {
              items: store.items().filter((item) => item.escalation.id !== data.escalationId),
            });
          }
        }
      });
    },
  }),
);
