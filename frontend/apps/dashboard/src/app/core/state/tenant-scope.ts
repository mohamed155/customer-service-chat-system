import { computed, DestroyRef, inject, Injectable, untracked } from '@angular/core';
import { Store } from '@ngrx/store';
import {
  patchState,
  SignalStoreFeature,
  SignalStoreFeatureResult,
  EmptyFeatureResult,
  StateSignals,
  withHooks,
  WritableStateSource,
} from '@ngrx/signals';
import { distinctUntilChanged, map, Observable, pairwise, Subject } from 'rxjs';
import { selectActiveTenant } from './tenant-context.feature';

/**
 * One rule for every piece of workspace data in the dashboard (architecture section 17):
 * when the active workspace changes, nothing from the old one may be on screen or still arriving.
 *
 * `TenantScope` announces the change. A store opts in with `withTenantScope()`, the HTTP layer
 * cancels the old workspace's requests, and the realtime stream reconnects. All three listen to
 * the same announcement, so a new store cannot forget a separate mechanism.
 */

export interface TenantChange {
  /** The workspace that was active, or `null`. */
  readonly from: string | null;
  /** The workspace that is active now, or `null` after `clearActiveTenant`. */
  readonly to: string | null;
}

@Injectable({ providedIn: 'root' })
export class TenantScope {
  private readonly changeSubject = new Subject<TenantChange>();
  private readonly activeTenant = inject(Store).selectSignal(selectActiveTenant);

  /** The id of the active workspace now, or `null`. */
  readonly activeTenantId = computed(() => this.activeTenant()?.id ?? null);

  /**
   * Emits when the active workspace id changes, in the same call stack as the reducer, before
   * anything renders. Hot and not replayed: a listener that subscribes later sees only later
   * changes.
   */
  readonly changes$: Observable<TenantChange> = this.changeSubject.asObservable();

  constructor() {
    inject(Store)
      .select(selectActiveTenant)
      .pipe(
        map((tenant) => tenant?.id ?? null),
        distinctUntilChanged(),
        pairwise(),
      )
      .subscribe(([from, to]) => this.changeSubject.next({ from, to }));
  }
}

type StoreOf<Input extends SignalStoreFeatureResult> = StateSignals<Input['state']> &
  Input['props'] &
  Input['methods'] &
  WritableStateSource<Input['state']>;

export interface TenantScopeConfig<Input extends SignalStoreFeatureResult> {
  /**
   * The state a store starts in. It is applied again on every workspace change. Give a function
   * when the state holds a value that goes stale, such as today's date.
   */
  readonly initialState: Input['state'] | (() => Input['state']);
  /** Also call `reload` when the store is created and a workspace is already active. */
  readonly loadOnInit?: boolean;
  /**
   * Called before the reset, for anything the state does not hold: a poll, a timer, a request
   * that is not made through the HTTP client. Called for every change.
   */
  readonly cancel?: (store: StoreOf<Input>, change: TenantChange) => void;
  /**
   * Called after the reset when a workspace is now active, to load the new workspace's data.
   * Not called after `clearActiveTenant`.
   */
  readonly reload?: (store: StoreOf<Input>, change: TenantChange) => void;
}

/**
 * Puts a store back to `initialState` the moment the active workspace changes, then reloads for
 * the new one. Add it as the last feature of every store that holds workspace data.
 */
export function withTenantScope<Input extends SignalStoreFeatureResult>(
  config: TenantScopeConfig<Input>,
): SignalStoreFeature<Input, EmptyFeatureResult> {
  return withHooks<Input>({
    onInit(store) {
      // A store created without the global Store (a unit test of the store alone) has no
      // workspace to follow.
      if (!inject(Store, { optional: true })) return;
      const scope = inject(TenantScope);
      const destroyRef = inject(DestroyRef);
      const asStore = store as unknown as StoreOf<Input>;
      const initial = (): Input['state'] =>
        typeof config.initialState === 'function'
          ? (config.initialState as () => Input['state'])()
          : config.initialState;

      const activeId = untracked(() => scope.activeTenantId());
      if (config.loadOnInit && activeId !== null) {
        untracked(() => config.reload?.(asStore, { from: null, to: activeId }));
      }

      const subscription = scope.changes$.subscribe((change) => {
        untracked(() => config.cancel?.(asStore, change));
        patchState(store as WritableStateSource<Input['state']>, initial());
        if (change.to !== null) {
          untracked(() => config.reload?.(asStore, change));
        }
      });
      destroyRef.onDestroy(() => subscription.unsubscribe());
    },
  });
}

/**
 * Runs `callback` when the active workspace changes, for state a component or service holds
 * itself (an open dialog, a picked row). Call it in an injection context; it unsubscribes when
 * the owner is destroyed. Stores use `withTenantScope()` instead.
 */
export function onTenantChange(callback: (change: TenantChange) => void): void {
  if (!inject(Store, { optional: true })) return;
  const subscription = inject(TenantScope).changes$.subscribe(callback);
  inject(DestroyRef).onDestroy(() => subscription.unsubscribe());
}
