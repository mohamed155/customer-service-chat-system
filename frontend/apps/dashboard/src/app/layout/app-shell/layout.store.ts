import { computed, inject } from '@angular/core';
import { Store } from '@ngrx/store';
import { patchState, signalStore, withComputed, withHooks, withState } from '@ngrx/signals';
import { appUiActions } from '../../core/state/app-ui.feature';

export const LAYOUT_COLLAPSE_BREAKPOINT = 1024;

export const LayoutStore = signalStore(
  withState({
    viewportWidth: typeof window === 'undefined' ? LAYOUT_COLLAPSE_BREAKPOINT : window.innerWidth,
  }),
  withComputed(({ viewportWidth }) => ({
    isNarrow: computed(() => viewportWidth() < LAYOUT_COLLAPSE_BREAKPOINT),
  })),
  withHooks((store) => {
    const globalStore = inject(Store);
    let previousNarrow = false;
    const update = (): void => {
      const width = window.innerWidth;
      const narrow = width < LAYOUT_COLLAPSE_BREAKPOINT;
      patchState(store, { viewportWidth: width });
      if (narrow && !previousNarrow)
        globalStore.dispatch(appUiActions.sidebarCollapsedSet({ collapsed: true }));
      previousNarrow = narrow;
    };
    return {
      onInit(): void {
        if (typeof window !== 'undefined') {
          update();
          window.addEventListener('resize', update);
        }
      },
      onDestroy(): void {
        if (typeof window !== 'undefined') window.removeEventListener('resize', update);
      },
    };
  }),
);
