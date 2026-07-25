import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { pipe, switchMap, tap } from 'rxjs';
import { catchError, map, of } from 'rxjs';
import { PlatformPlan, PlatformSubscriptionRow } from '../../../core/api/tenant-api.models';
import { CreatePlanRequest, PlatformBillingApiService } from './platform-billing-api.service';

export interface PlatformBillingState {
  readonly plans: PlatformPlan[];
  readonly subscriptions: PlatformSubscriptionRow[];
  readonly loading: boolean;
  readonly error: string | null;
  readonly saving: boolean;
}

export const PlatformBillingStore = signalStore(
  withState<PlatformBillingState>({
    plans: [],
    subscriptions: [],
    loading: false,
    error: null,
    saving: false,
  }),
  withMethods((store, api = inject(PlatformBillingApiService)) => {
    const loadAll = rxMethod<void>(
      pipe(
        tap(() => patchState(store, { loading: true, error: null })),
        switchMap(() =>
          api.getPlans().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load plans',
              });
              return of(null);
            }),
          ),
        ),
        switchMap((plans) => {
          if (!plans) return of(null);
          patchState(store, { plans });
          return api.getSubscriptions().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                loading: false,
                error: (err as Error)?.message ?? 'Failed to load subscriptions',
              });
              return of(null);
            }),
          );
        }),
        tap((subscriptions) => {
          if (subscriptions) {
            patchState(store, { subscriptions, loading: false });
          }
        }),
      ),
    );

    const createPlan = rxMethod<CreatePlanRequest>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap((req) =>
          api.createPlan(req).pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to create plan',
              });
              return of(null);
            }),
          ),
        ),
        tap((plan) => {
          if (plan) {
            patchState(store, { plans: [...store.plans(), plan], saving: false });
          }
        }),
      ),
    );

    const retirePlan = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap((planId) =>
          api.updatePlanStatus(planId, 'retired').pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to retire plan',
              });
              return of(null);
            }),
          ),
        ),
        tap((updated) => {
          if (updated) {
            patchState(store, {
              plans: store.plans().map((p) => (p.id === updated.id ? updated : p)),
              saving: false,
            });
          }
        }),
      ),
    );

    const activatePlan = rxMethod<string>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap((planId) =>
          api.updatePlanStatus(planId, 'active').pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to activate plan',
              });
              return of(null);
            }),
          ),
        ),
        tap((updated) => {
          if (updated) {
            patchState(store, {
              plans: store.plans().map((p) => (p.id === updated.id ? updated : p)),
              saving: false,
            });
          }
        }),
      ),
    );

    const assignSubscription = rxMethod<{ tenantId: string; planId: string; status: string }>(
      pipe(
        tap(() => patchState(store, { saving: true, error: null })),
        switchMap(({ tenantId, planId, status }) =>
          api.putSubscription(tenantId, { planId, status }).pipe(
            map(() => undefined),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to assign subscription',
              });
              return of(null);
            }),
          ),
        ),
        switchMap(() =>
          api.getSubscriptions().pipe(
            map(({ data }) => data),
            catchError((err: unknown) => {
              patchState(store, {
                saving: false,
                error: (err as Error)?.message ?? 'Failed to reload subscriptions',
              });
              return of(null);
            }),
          ),
        ),
        tap((subscriptions) => {
          if (subscriptions) {
            patchState(store, { subscriptions, saving: false });
          }
        }),
      ),
    );

    return {
      loadAll(): void {
        loadAll();
      },
      createPlan(req: CreatePlanRequest): void {
        createPlan(req);
      },
      retirePlan(planId: string): void {
        retirePlan(planId);
      },
      activatePlan(planId: string): void {
        activatePlan(planId);
      },
      assignSubscription(tenantId: string, planId: string, status: string): void {
        assignSubscription({ tenantId, planId, status });
      },
    };
  }),
  withHooks((store) => ({
    onInit(): void {
      store.loadAll();
    },
  })),
);
