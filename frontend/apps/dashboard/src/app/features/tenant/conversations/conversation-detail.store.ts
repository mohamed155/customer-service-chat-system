import { computed, inject } from '@angular/core';
import { patchState, signalStore, withComputed, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import {
  AddMessagePayload,
  ConversationDetailEscalation,
  ConversationStatus,
  Message,
} from '../../../core/api/tenant-api.models';
import { ConversationsApiService } from './conversations-api.service';

export interface TimelinePage {
  readonly items: Message[];
  readonly cursor: string | null;
  readonly hasMore: boolean;
}

interface ConversationDetailState {
  readonly conversation: ConversationDetailEscalation | null;
  readonly timelinePages: TimelinePage[];
  readonly loading: boolean;
  readonly loadingTimeline: boolean;
  readonly submitting: boolean;
  readonly error: string | null;
  readonly timelineError: string | null;
}

export const ConversationDetailStore = signalStore(
  withState<ConversationDetailState>({
    conversation: null,
    timelinePages: [],
    loading: false,
    loadingTimeline: false,
    submitting: false,
    error: null,
    timelineError: null,
  }),
  withComputed(({ timelinePages }) => ({
    timeline: computed(() =>
      [...timelinePages()]
        .reverse()
        .flatMap((page) => page.items)
        .sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()),
    ),
    hasMoreTimeline: computed(() => {
      const pages = timelinePages();
      return pages.length === 0 || pages[pages.length - 1]?.hasMore === true;
    }),
    oldestCursor: computed(() => {
      const pages = timelinePages();
      return pages.length > 0 ? (pages[pages.length - 1]?.cursor ?? null) : null;
    }),
  })),
  withMethods((store, api = inject(ConversationsApiService)) => {
    const loadDetail = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap((id) =>
          api.get(id).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load conversation',
              });
              return of(null);
            }),
          ),
        ),
        tap((conversation) => {
          if (conversation) {
            patchState(store, {
              conversation,
              loading: false,
            });
          }
        }),
      ),
    );

    const loadTimeline = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { loadingTimeline: true, timelineError: null })),
        switchMap((id) =>
          api.getTimeline(id).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loadingTimeline: false,
                timelineError: (err as Error)?.message ?? 'Failed to load timeline',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              timelinePages: [
                { items: result.items, cursor: result.nextCursor, hasMore: result.hasMore },
              ],
              loadingTimeline: false,
            });
          }
        }),
      ),
    );

    const loadOlderRx = rxMethod<{ id: string; cursor: string }>(
      pipe(
        tap(() => patchState(store, { loadingTimeline: true, timelineError: null })),
        switchMap(({ id, cursor }) =>
          api.getTimeline(id, cursor).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loadingTimeline: false,
                timelineError: (err as Error)?.message ?? 'Failed to load older messages',
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              timelinePages: [
                ...store.timelinePages(),
                { items: result.items, cursor: result.nextCursor, hasMore: result.hasMore },
              ],
              loadingTimeline: false,
            });
          }
        }),
      ),
    );

    return {
      load(id: string): void {
        loadDetail(id);
        loadTimeline(id);
      },
      loadOlder(id: string): void {
        const cursor = store.oldestCursor();
        if (cursor) loadOlderRx({ id, cursor });
      },
      addMessage(conversationId: string, message: AddMessagePayload): void {
        const convId = conversationId;
        const isNote = message.kind === 'note';
        patchState(store, { submitting: true });
        api.addMessage(convId, message).subscribe({
          next: (response) => {
            const { message: newMessage, conversation: convState } = response.data;
            patchState(store, {
              timelinePages: [
                ...store.timelinePages(),
                { items: [newMessage], cursor: null, hasMore: false },
              ],
              submitting: false,
            });
            if (!isNote) {
              const current = store.conversation();
              if (current) {
                patchState(store, {
                  conversation: {
                    ...current,
                    status: convState.status,
                    lastActivityAt: convState.lastActivityAt,
                  },
                });
              }
            }
          },
          error: (err: unknown) => {
            patchState(store, {
              submitting: false,
              error: (err as Error)?.message ?? 'Failed to send message',
            });
          },
        });
      },
      patchStatus(id: string, status: ConversationStatus): void {
        api.patch(id, { status }).subscribe({
          next: (response) => {
            const current = store.conversation();
            patchState(store, {
              conversation: current
                ? { ...response.data, escalation: current.escalation }
                : (response.data as ConversationDetailEscalation),
            });
          },
          error: (err: unknown) => {
            patchState(store, {
              error: (err as Error)?.message ?? 'Failed to update status',
            });
          },
        });
      },
      setAiHandling(conversationId: string, mode: 'platform_ai' | 'human'): void {
        api.setConversationAiHandling(conversationId, mode).subscribe({
          next: (response) => {
            const current = store.conversation();
            patchState(store, {
              conversation: current
                ? { ...current, ...response.data }
                : (response.data as ConversationDetailEscalation),
            });
          },
          error: (err: unknown) => {
            patchState(store, {
              error: (err as Error)?.message ?? 'Failed to set AI handling',
            });
          },
        });
      },

      patchAssignment(id: string, membershipId: string | null): void {
        api.patch(id, { assignedMembershipId: membershipId }).subscribe({
          next: (response) => {
            const current = store.conversation();
            patchState(store, {
              conversation: current
                ? { ...response.data, escalation: current.escalation }
                : (response.data as ConversationDetailEscalation),
            });
          },
          error: (err: unknown) => {
            patchState(store, {
              error: (err as Error)?.message ?? 'Failed to update assignment',
            });
          },
        });
      },
    };
  }),
);
