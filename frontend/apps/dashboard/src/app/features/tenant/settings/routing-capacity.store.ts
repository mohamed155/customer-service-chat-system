import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, EMPTY, exhaustMap, pipe, switchMap, tap } from 'rxjs';
import { PermissionsService } from '../../../core/authz/permissions.service';
import { ApiError } from '../../../core/api/api.models';
import { RoutingCapacityApiService } from '../../../core/api/routing-capacity-api.service';
import { toApiError } from '../../../core/errors/api-error';
import { withTenantScope } from '../../../core/state/tenant-scope';

type RoutingCapacityStatus = 'pending' | 'loading' | 'success' | 'error';
type SaveStatus = 'idle' | 'submitting' | 'success' | 'error';

interface RoutingCapacityState {
  defaultCapacity: number;
  status: RoutingCapacityStatus;
  error: ApiError | null;
  saveStatus: SaveStatus;
  saveError: ApiError | null;
}

const initialState: RoutingCapacityState = {
  defaultCapacity: 10,
  status: 'pending',
  error: null,
  saveStatus: 'idle',
  saveError: null,
};

export const RoutingCapacityStore = signalStore(
  withState(initialState),
  withMethods(
    (store, api = inject(RoutingCapacityApiService), permissions = inject(PermissionsService)) => {
      const load = rxMethod<void>(
        pipe(
          switchMap(() => {
            if (!permissions.has('members.manage')) return EMPTY;
            patchState(store, { status: 'loading' as const, error: null });
            return api.get().pipe(
              tap({
                next: ({ data }) =>
                  patchState(store, {
                    defaultCapacity: data.default_capacity,
                    status: 'success' as const,
                    error: null,
                  }),
                error: (error) =>
                  patchState(store, {
                    status: 'error' as const,
                    error: toApiError(error),
                  }),
              }),
              catchError(() => EMPTY),
            );
          }),
        ),
      );

      const saveDefault = rxMethod<number>(
        pipe(
          tap(() => patchState(store, { saveStatus: 'submitting' as const, saveError: null })),
          exhaustMap((capacity) =>
            api.setDefault(capacity).pipe(
              tap({
                next: ({ data }) =>
                  patchState(store, {
                    defaultCapacity: data.default_capacity,
                    saveStatus: 'success' as const,
                    saveError: null,
                  }),
                error: (error) =>
                  patchState(store, {
                    saveStatus: 'error' as const,
                    saveError: toApiError(error),
                  }),
              }),
              catchError(() => EMPTY),
            ),
          ),
        ),
      );

      return {
        load,
        saveDefault,
        retry(): void {
          load();
        },
        clearSaveError(): void {
          patchState(store, { saveStatus: 'idle' as const, saveError: null });
        },
      };
    },
  ),
  withHooks((store) => ({
    onInit(): void {
      store.load();
    },
  })),
  withTenantScope({
    initialState,
    reload: (store) => store.load(),
  }),
);
