import { inject } from '@angular/core';
import { patchState, signalStore, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { AnalyticsSummary, AnalyticsTimeseries } from '../../../core/api/tenant-api.models';
import { AnalyticsApiService } from './analytics-api.service';
import { withTenantScope } from '../../../core/state/tenant-scope';

export interface AnalyticsState {
  readonly from: string;
  readonly to: string;
  readonly preset: '7' | '30' | '90' | 'custom';
  readonly channel: string | null;
  readonly summary: AnalyticsSummary | null;
  readonly timeseries: AnalyticsTimeseries | null;
  readonly loading: boolean;
  readonly error: ApiError | null;
  /** The range the person picked ends before it starts; nothing is asked of the API. */
  readonly rangeInvalid: boolean;
}

function formatDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function initialFrom(): string {
  const d = new Date();
  d.setDate(d.getDate() - 29);
  return formatDate(d);
}

function initialTo(): string {
  return formatDate(new Date());
}

const initialState = (): AnalyticsState => ({
  from: initialFrom(),
  to: initialTo(),
  preset: '30',
  channel: null,
  summary: null,
  timeseries: null,
  loading: false,
  error: null,
  rangeInvalid: false,
});

export const AnalyticsStore = signalStore(
  withState<AnalyticsState>(initialState()),
  withMethods((store, api = inject(AnalyticsApiService)) => {
    const loadSummary = rxMethod<{ from: string; to: string; channel: string | null }>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null, summary: null })),
        switchMap(({ from, to, channel }) =>
          api
            .getSummary({
              from,
              to,
              channel,
            })
            .pipe(
              map(({ data }) => data),
              catchError((err: unknown) => {
                patchState(store, {
                  loading: false,
                  error: toApiError(err),
                });
                return of(null);
              }),
            ),
        ),
        tap((summary) => {
          if (summary) {
            patchState(store, { summary, loading: false });
          }
        }),
      ),
    );

    const loadTimeseries = rxMethod<{ from: string; to: string; channel: string | null }>(
      pipe(
        tap(() => patchState(store, { loading: true })),
        switchMap(({ from, to, channel }) =>
          api
            .getTimeseries({
              from,
              to,
              channel,
            })
            .pipe(
              map(({ data }) => data),
              catchError((err: unknown) => {
                patchState(store, {
                  loading: false,
                  error: toApiError(err),
                });
                return of(null);
              }),
            ),
        ),
        tap((timeseries) => {
          if (timeseries) {
            patchState(store, { timeseries, loading: false });
          }
        }),
      ),
    );

    return {
      load(): void {
        loadSummary({ from: store.from(), to: store.to(), channel: store.channel() });
        loadTimeseries({ from: store.from(), to: store.to(), channel: store.channel() });
      },
      setPreset(days: '7' | '30' | '90' | 'custom'): void {
        if (days === 'custom') {
          patchState(store, { preset: days });
          return;
        }
        const n = parseInt(days, 10);
        const to = new Date();
        const from = new Date();
        from.setDate(from.getDate() - (n - 1));
        patchState(store, {
          preset: days,
          from: formatDate(from),
          to: formatDate(to),
          rangeInvalid: false,
        });
        loadSummary({ from: store.from(), to: store.to(), channel: store.channel() });
        loadTimeseries({ from: store.from(), to: store.to(), channel: store.channel() });
      },
      setCustomRange(from: string, to: string): void {
        if (from > to) {
          patchState(store, { preset: 'custom', rangeInvalid: true });
          return;
        }
        patchState(store, { from, to, preset: 'custom', rangeInvalid: false });
        loadSummary({ from: store.from(), to: store.to(), channel: store.channel() });
        loadTimeseries({ from: store.from(), to: store.to(), channel: store.channel() });
      },
      setChannel(value: string): void {
        patchState(store, { channel: value === 'all' ? null : value });
        loadSummary({ from: store.from(), to: store.to(), channel: store.channel() });
        loadTimeseries({ from: store.from(), to: store.to(), channel: store.channel() });
      },
    };
  }),
  withTenantScope({
    initialState,
    loadOnInit: true,
    reload: (store) => store.load(),
  }),
);
