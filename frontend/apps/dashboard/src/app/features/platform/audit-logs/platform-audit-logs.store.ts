import { ApiError } from '../../../core/api/api.models';
import { toApiError } from '../../../core/errors/api-error';
import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { AuditEntry } from '../../../core/api/tenant-api.models';
import { PlatformAuditLogsApiService } from './platform-audit-logs-api.service';

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

export interface PlatformAuditLogsState {
  readonly entries: AuditEntry[];
  readonly nextCursor: string | null;
  readonly hasMore: boolean;
  readonly from: string;
  readonly to: string;
  readonly category: string | null;
  readonly actorId: string | null;
  readonly tenantId: string | null;
  readonly loading: boolean;
  readonly loadingMore: boolean;
  readonly error: ApiError | null;
  /** The range the person picked ends before it starts; nothing is asked of the API. */
  readonly dateRangeInvalid: boolean;
  readonly selectedEntry: AuditEntry | null;
  readonly drawerOpen: boolean;
}

export const PlatformAuditLogsStore = signalStore(
  withState<PlatformAuditLogsState>({
    entries: [],
    nextCursor: null,
    hasMore: false,
    from: initialFrom(),
    to: initialTo(),
    category: null,
    actorId: null,
    tenantId: null,
    loading: false,
    loadingMore: false,
    error: null,
    dateRangeInvalid: false,
    selectedEntry: null,
    drawerOpen: false,
  }),
  withMethods((store, api = inject(PlatformAuditLogsApiService)) => {
    const load = rxMethod<{
      from: string;
      to: string;
      category: string | null;
      actorId: string | null;
      tenantId: string | null;
    }>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null, entries: [], nextCursor: null })),
        switchMap((params) =>
          api.list({ ...params, cursor: null }).pipe(
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
        tap((result) => {
          if (result) {
            patchState(store, {
              entries: result.data,
              nextCursor: result.pagination.nextCursor,
              hasMore: result.pagination.hasMore,
              loading: false,
            });
          }
        }),
      ),
    );

    const loadMore = rxMethod<{
      cursor: string;
      from: string;
      to: string;
      category: string | null;
      actorId: string | null;
      tenantId: string | null;
    }>(
      pipe(
        tap(() => patchState(store, { loadingMore: true })),
        switchMap((params) =>
          api.list(params).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loadingMore: false,
                error: toApiError(err),
              });
              return of(null);
            }),
          ),
        ),
        tap((result) => {
          if (result) {
            patchState(store, {
              entries: [...store.entries(), ...result.data],
              nextCursor: result.pagination.nextCursor,
              hasMore: result.pagination.hasMore,
              loadingMore: false,
            });
          }
        }),
      ),
    );

    return {
      load(): void {
        load({
          from: store.from(),
          to: store.to(),
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      loadMore(): void {
        const cursor = store.nextCursor();
        if (!cursor) return;
        loadMore({
          cursor,
          from: store.from(),
          to: store.to(),
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      setCategory(value: string): void {
        patchState(store, { category: value === 'all' ? null : value });
        load({
          from: store.from(),
          to: store.to(),
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      setDateRange(from: string, to: string): void {
        if (from > to) {
          patchState(store, { dateRangeInvalid: true });
          return;
        }
        patchState(store, { from, to, dateRangeInvalid: false });
        load({
          from,
          to,
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      setActor(id: string | null): void {
        patchState(store, { actorId: id });
        load({
          from: store.from(),
          to: store.to(),
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      setTenant(id: string | null): void {
        patchState(store, { tenantId: id });
        load({
          from: store.from(),
          to: store.to(),
          category: store.category(),
          actorId: store.actorId(),
          tenantId: store.tenantId(),
        });
      },
      openEntry(entry: AuditEntry): void {
        patchState(store, { selectedEntry: entry, drawerOpen: true });
      },
      closeDrawer(): void {
        patchState(store, { drawerOpen: false });
      },
    };
  }),
  withHooks((store) => ({
    onInit(): void {
      store.load();
    },
  })),
);
