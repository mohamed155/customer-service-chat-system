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
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { Conversation, ConversationListQuery } from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { ConversationsApiService } from './conversations-api.service';

export interface InboxFilters {
  readonly status?: ConversationListQuery['status'];
  readonly assignee?: string;
  readonly channel?: string;
  readonly escalated?: string;
}

interface ConversationsState {
  readonly filters: InboxFilters;
  readonly cursor: string | null;
  readonly hasMore: boolean;
  readonly items: Conversation[];
  readonly loading: boolean;
  readonly error: string | null;
  readonly selectedId: string | null;
  readonly averageRating: number | null;
  readonly feedbackCount: number;
}

export const ConversationsStore = signalStore(
  withState<ConversationsState>({
    filters: { status: 'open' },
    cursor: null,
    hasMore: false,
    items: [],
    loading: false,
    error: null,
    selectedId: null,
    averageRating: null,
    feedbackCount: 0,
  }),
  withComputed(({ items, selectedId, filters }) => ({
    filteredConversations: computed(() => items()),
    selectedConversation: computed(() => items().find((c) => c.id === selectedId()) ?? null),
    statusFilter: computed(() => filters().status ?? 'all'),
  })),
  withMethods(
    (
      store,
      api = inject(ConversationsApiService),
      globalStore = inject(Store, { optional: true }),
    ) => {
      const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);

      const load = rxMethod<{
        tenantId: string | null;
        filters: InboxFilters;
        cursor: string | null;
      }>(
        pipe(
          tap(() => patchState(store, { loading: true, error: null, items: [], selectedId: null })),
          switchMap(({ tenantId, filters, cursor }) => {
            if (!tenantId) return of(null);
            const query: ConversationListQuery = {
              ...filters,
              cursor: cursor || undefined,
            };
            return api.list(query).pipe(
              map(({ data }) => data),
              catchError((err: unknown) => {
                patchState(store, {
                  loading: false,
                  error: (err as Error)?.message ?? 'Failed to load conversations',
                });
                return of(null);
              }),
            );
          }),
          tap((result) => {
            if (result) {
              patchState(store, {
                items: result.items,
                cursor: result.nextCursor,
                hasMore: result.hasMore,
                loading: false,
                selectedId: result.items[0]?.id ?? null,
              });
            }
          }),
        ),
      );

      const loadFeedbackSummary = rxMethod<string | null>(
        pipe(
          tap(() => patchState(store, { averageRating: null, feedbackCount: 0 })),
          switchMap((tenantId) => {
            if (!tenantId) return of(null);
            return api.getFeedbackSummary().pipe(
              map(({ data }) => data),
              catchError(() => of(null)),
            );
          }),
          tap((result) => {
            if (result) {
              patchState(store, {
                averageRating: result.averageRating,
                feedbackCount: result.feedbackCount,
              });
            }
          }),
        ),
      );

      return {
        loadInbox(): void {
          load({ tenantId: activeTenant()?.id ?? null, filters: store.filters(), cursor: '' });
        },
        setFilter(filter: Partial<InboxFilters>): void {
          const merged = { ...store.filters(), ...filter };
          patchState(store, { filters: merged, cursor: null });
          load({ tenantId: activeTenant()?.id ?? null, filters: merged, cursor: '' });
        },
        nextPage(): void {
          if (store.hasMore()) {
            load({
              tenantId: activeTenant()?.id ?? null,
              filters: store.filters(),
              cursor: store.cursor() ?? '',
            });
          }
        },
        resetFilters(): void {
          const defaults: InboxFilters = { status: 'open' };
          patchState(store, { filters: defaults, cursor: null });
          load({ tenantId: activeTenant()?.id ?? null, filters: defaults, cursor: '' });
        },
        select(id: string): void {
          patchState(store, { selectedId: id });
        },
        loadFeedbackSummary(): void {
          loadFeedbackSummary(activeTenant()?.id ?? null);
        },
      };
    },
  ),
  withHooks((store, globalStore = inject(Store, { optional: true })) => {
    const activeTenant = globalStore?.selectSignal(selectActiveTenant) ?? (() => null);
    return {
      onInit(): void {
        effect(() => {
          if (activeTenant()?.id) {
            store.loadInbox();
            store.loadFeedbackSummary();
          }
        });
      },
    };
  }),
);
