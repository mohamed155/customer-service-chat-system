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
import { filter, pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import {
  AddMessagePayload,
  AiMessageCompleted,
  AiMessageDelta,
  AiMessageStarted,
  ConversationDetailEscalation,
  ConversationStatus,
  Message,
  ToolRequest,
  ToolRequestUpdatedEvent,
} from '../../../core/api/tenant-api.models';
import { RealtimeService, SseEvent } from '../../../core/realtime/realtime.service';
import { ConversationsApiService } from './conversations-api.service';

export type GenerationPhase = 'idle' | 'thinking' | 'streaming';

export interface ActiveGeneration {
  readonly generationId: string;
  readonly phase: GenerationPhase;
  readonly buffer: string;
}

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
  readonly activeGeneration: ActiveGeneration | null;
  readonly openConversationId: string | null;
  readonly toolActivity: Record<string, ToolRequest>;
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
    activeGeneration: null,
    openConversationId: null,
    toolActivity: {},
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

    const loadToolActivity = rxMethod<string>(
      pipe(
        switchMap((id) =>
          api.getToolActivity(id).pipe(
            map(({ data }) => data),
            catchError(() => of({ items: [] as ToolRequest[] })),
          ),
        ),
        tap((result) => {
          const map: Record<string, ToolRequest> = {};
          for (const item of result.items) {
            map[item.id] = item;
          }
          patchState(store, { toolActivity: map });
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
        loadToolActivity(id);
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
      setConversationId(id: string | null): void {
        patchState(store, { openConversationId: id });
        if (id) {
          loadDetail(id);
          loadTimeline(id);
          loadToolActivity(id);
        } else {
          patchState(store, { activeGeneration: null, toolActivity: {} });
        }
      },

      openConversation(id: string): void {
        const prev = store.openConversationId();
        if (prev !== id) {
          patchState(store, { openConversationId: id, activeGeneration: null });
          loadDetail(id);
          loadTimeline(id);
          loadToolActivity(id);
        }
      },

      handleAiEvent(event: SseEvent): void {
        const convId = store.openConversationId();
        if (!convId) return;

        const parsed = tryParseJson(event.data);
        if (!parsed) return;
        const data = parsed as { conversationId?: string };
        if (!data.conversationId || data.conversationId !== convId) return;

        switch (event.event) {
          case 'ai.message.started': {
            const payload = data as unknown as AiMessageStarted;
            patchState(store, {
              activeGeneration: {
                generationId: payload.generationId,
                phase: 'thinking',
                buffer: '',
              },
            });
            break;
          }
          case 'ai.message.delta': {
            const payload = data as unknown as AiMessageDelta;
            const current = store.activeGeneration();
            if (current?.generationId === payload.generationId) {
              patchState(store, {
                activeGeneration: {
                  ...current,
                  phase: 'streaming',
                  buffer: current.buffer + payload.text,
                },
              });
            }
            break;
          }
          case 'ai.message.completed': {
            const payload = data as unknown as AiMessageCompleted;
            const current = store.activeGeneration();
            if (current?.generationId === payload.generationId) {
              patchState(store, {
                timelinePages: [
                  ...store.timelinePages(),
                  { items: [payload.message], cursor: null, hasMore: false },
                ],
                activeGeneration: null,
              });
            }
            break;
          }
          case 'ai.message.superseded':
          case 'ai.message.failed': {
            patchState(store, { activeGeneration: null });
            break;
          }
        }
      },

      handleToolEvent(event: SseEvent): void {
        const convId = store.openConversationId();
        if (!convId) return;

        const parsed = tryParseJson(event.data);
        if (!parsed) return;

        const wrapped = parsed as { payload?: Record<string, unknown> };
        const payload = wrapped.payload;
        if (!payload) return;

        const data = payload as { conversationId?: string };
        if (!data.conversationId || data.conversationId !== convId) return;

        switch (event.event) {
          case 'tool.request.created': {
            const entry = payload as unknown as ToolRequest;
            patchState(store, {
              toolActivity: { ...store.toolActivity(), [entry.id]: entry },
            });
            break;
          }
          case 'tool.request.updated': {
            const update = payload as unknown as ToolRequestUpdatedEvent;
            const current = store.toolActivity()[update.id];
            if (current) {
              const merged: ToolRequest = {
                ...current,
                status: update.status,
                durationMs: update.durationMs,
                error: update.error,
              };
              patchState(store, {
                toolActivity: { ...store.toolActivity(), [update.id]: merged },
              });
            }
            break;
          }
        }
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
  withHooks({
    onInit(store, _realtime = inject(RealtimeService)) {
      const sub = _realtime
        .events()
        .pipe(
          filter((e) => e.event.startsWith('ai.message.') && store.openConversationId() != null),
        )
        .subscribe((e) => store.handleAiEvent(e));

      const toolSub = _realtime
        .events()
        .pipe(
          filter((e) => e.event.startsWith('tool.request.') && store.openConversationId() != null),
        )
        .subscribe((e) => store.handleToolEvent(e));

      (
        store as unknown as {
          realtimeSub: { unsubscribe: () => void };
          toolRealtimeSub: { unsubscribe: () => void };
        }
      ).realtimeSub = sub;
      (
        store as unknown as {
          realtimeSub: { unsubscribe: () => void };
          toolRealtimeSub: { unsubscribe: () => void };
        }
      ).toolRealtimeSub = toolSub;
    },
    onDestroy(store) {
      (
        store as unknown as { realtimeSub?: { unsubscribe: () => void } }
      ).realtimeSub?.unsubscribe();
      (
        store as unknown as { toolRealtimeSub?: { unsubscribe: () => void } }
      ).toolRealtimeSub?.unsubscribe();
    },
  }),
);

function tryParseJson(text: string): Record<string, unknown> | null {
  try {
    return JSON.parse(text) as Record<string, unknown>;
  } catch {
    return null;
  }
}
