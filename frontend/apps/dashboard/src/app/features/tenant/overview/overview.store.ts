import { computed, inject } from '@angular/core';
import { patchState, signalStore, withComputed, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, map, Observable, of, pipe, switchMap, tap } from 'rxjs';
import { AnalyticsApiService } from '../../../core/api/analytics-api.service';
import { ApiError, ApiResponse } from '../../../core/api/api.models';
import {
  AnalyticsSummary,
  AnalyticsTimeseries,
  Conversation,
} from '../../../core/api/tenant-api.models';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { toApiError } from '../../../core/errors/api-error';
import { withTenantScope } from '../../../core/state/tenant-scope';
import { OverviewApiService } from './overview-api.service';

/** How many days the overview looks back, today included. */
export const OVERVIEW_DAYS = 30;
/** How many conversations "recent activity" lists. */
export const RECENT_CONVERSATIONS = 5;

/** One part of the page: it loads, fails and is empty on its own, so one failure blanks nothing else. */
export type SectionStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface Section<T> {
  readonly status: SectionStatus;
  readonly data: T | null;
  readonly error: ApiError | null;
}

export interface OverviewState {
  readonly summary: Section<AnalyticsSummary>;
  readonly timeseries: Section<AnalyticsTimeseries>;
  readonly recent: Section<readonly Conversation[]>;
}

function idle<T>(): Section<T> {
  return { status: 'idle', data: null, error: null };
}

const initialState = (): OverviewState => ({
  summary: idle(),
  timeseries: idle(),
  recent: idle(),
});

function formatDate(d: Date): string {
  const month = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${month}-${day}`;
}

/** The overview's window: the last `OVERVIEW_DAYS` days, ending today. */
export function overviewRange(today = new Date()): { from: string; to: string } {
  const from = new Date(today);
  from.setDate(from.getDate() - (OVERVIEW_DAYS - 1));
  return { from: formatDate(from), to: formatDate(today) };
}

export const OverviewStore = signalStore(
  withState<OverviewState>(initialState()),
  withComputed((store, permissions = inject(PermissionsService)) => ({
    /** The analytics sections need `analytics.view`; without it the API answers 403. */
    canViewAnalytics: computed(() => permissions.has('analytics.view')),
    /** The activity list needs `conversations.view`. */
    canViewConversations: computed(() => permissions.has('conversations.view')),
    /** The summary has no channel with a conversation in the window. */
    channelsEmpty: computed(() => (store.summary().data?.channels.length ?? 0) === 0),
    /** The trend has no day with a conversation in the window. */
    trendEmpty: computed(() => {
      const days = store.timeseries().data?.days ?? [];
      return days.every((day) => day.conversationVolume === 0);
    }),
  })),
  withMethods(
    (
      store,
      analytics = inject(AnalyticsApiService),
      overview = inject(OverviewApiService),
      permissions = inject(PermissionsService),
    ) => {
      type Key = keyof OverviewState;

      /** Loads one section: `loading` first, then the data or the error; a newer call wins. */
      function sectionLoader<A, T>(key: Key, request: (arg: A) => Observable<ApiResponse<T>>) {
        return rxMethod<A>(
          pipe(
            tap(() => patchState(store, { [key]: { status: 'loading', data: null, error: null } })),
            switchMap((arg) =>
              request(arg).pipe(
                map(({ data }): Section<T> => ({ status: 'ready', data, error: null })),
                catchError((err: unknown) =>
                  of<Section<T>>({ status: 'error', data: null, error: toApiError(err) }),
                ),
              ),
            ),
            tap((section) => patchState(store, { [key]: section })),
          ),
        );
      }

      const loadSummary = sectionLoader('summary', (range: { from: string; to: string }) =>
        analytics.getSummary(range),
      );
      const loadTimeseries = sectionLoader('timeseries', (range: { from: string; to: string }) =>
        analytics.getTimeseries(range),
      );
      const loadRecent = sectionLoader('recent', (limit: number) =>
        overview.recentConversations(limit),
      );

      return {
        /** Loads every section the person may see; the others are never asked for. */
        load(): void {
          if (permissions.has('analytics.view')) {
            const range = overviewRange();
            loadSummary(range);
            loadTimeseries(range);
          }
          if (permissions.has('conversations.view')) loadRecent(RECENT_CONVERSATIONS);
        },
        retrySummary(): void {
          loadSummary(overviewRange());
        },
        retryTimeseries(): void {
          loadTimeseries(overviewRange());
        },
        retryRecent(): void {
          loadRecent(RECENT_CONVERSATIONS);
        },
      };
    },
  ),
  withTenantScope({
    initialState,
    loadOnInit: true,
    reload: (store) => store.load(),
  }),
);
