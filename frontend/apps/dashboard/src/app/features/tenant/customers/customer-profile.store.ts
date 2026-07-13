import { effect, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { catchError, combineLatest, EMPTY, of, pipe, switchMap, tap } from 'rxjs';
import { ApiError } from '../../../core/api/api.models';
import { ConversationSummary, CustomerDetail } from '../../../core/api/tenant-api.models';
import { selectActiveTenant } from '../../../core/state/tenant-context.feature';
import { CustomersApiService } from './customers-api.service';

export type CustomerProfileStatus = 'idle' | 'loading' | 'success' | 'not_found' | 'error';

export interface CustomerProfileState {
  readonly customerId: string | null;
  readonly customer: CustomerDetail | null;
  readonly conversations: readonly ConversationSummary[];
  readonly hasMoreConversations: boolean;
  readonly status: CustomerProfileStatus;
  readonly loading: boolean;
  readonly error: ApiError | null;
  readonly notFound: boolean;
}

const initialState: CustomerProfileState = {
  customerId: null,
  customer: null,
  conversations: [],
  hasMoreConversations: false,
  status: 'idle',
  loading: false,
  error: null,
  notFound: false,
};

const isNotFound = (error: unknown): boolean => {
  const candidate = error as { status?: number; code?: string } | null;
  return candidate?.status === 404 || candidate?.code === 'not_found';
};

const toApiError = (error: unknown): ApiError =>
  (error as ApiError) ?? {
    code: 'internal_error',
    message: 'Something went wrong',
    status: 0,
  };

export const CustomerProfileStore = signalStore(
  { providedIn: 'root' },
  withState(initialState),
  withMethods((store, api = inject(CustomersApiService)) => {
    const fetch = rxMethod<string>(
      pipe(
        switchMap((id) =>
          combineLatest([api.getCustomer(id), api.getConversationHistory(id)]).pipe(
            tap(([customerResponse, historyResponse]) => {
              if (store.customerId() !== id) return;
              patchState(store, {
                customerId: id,
                customer: customerResponse.data,
                conversations: historyResponse.data.items,
                hasMoreConversations: historyResponse.data.hasMore,
                status: 'success',
                loading: false,
                error: null,
                notFound: false,
              });
            }),
            switchMap(() => of(null)),
            catchError((error: unknown) => {
              if (store.customerId() !== id) return of(null);
              if (isNotFound(error)) {
                patchState(store, {
                  customerId: id,
                  customer: null,
                  conversations: [],
                  hasMoreConversations: false,
                  status: 'not_found',
                  loading: false,
                  error: toApiError(error),
                  notFound: true,
                });
                return EMPTY;
              }
              patchState(store, {
                customerId: id,
                status: 'error',
                loading: false,
                error: toApiError(error),
                notFound: false,
              });
              return of(null);
            }),
          ),
        ),
      ),
    );

    return {
      loadProfile(id: string): void {
        if (!id) return;
        patchState(store, {
          ...initialState,
          customerId: id,
          status: 'loading',
          loading: true,
        });
        fetch(id);
      },
      retry(): void {
        const id = store.customerId();
        if (!id) return;
        patchState(store, { status: 'loading', loading: true, error: null, notFound: false });
        fetch(id);
      },
      reset(): void {
        patchState(store, initialState);
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
      store.reset();
    });

    return {
      onInit(): void {
        /* component drives loading via route param */
      },
    };
  }),
);
