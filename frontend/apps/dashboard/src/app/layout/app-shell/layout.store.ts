import { computed, inject } from '@angular/core';
import { Router, NavigationEnd } from '@angular/router';
import { Store } from '@ngrx/store';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { filter } from 'rxjs/operators';
import { appUiActions } from '../../core/state/app-ui.feature';

export const LAYOUT_COLLAPSE_BREAKPOINT = 1024;
export const MOBILE_BREAKPOINT = 768;

export const LayoutStore = signalStore(
  withState({
    viewportWidth: typeof window === 'undefined' ? LAYOUT_COLLAPSE_BREAKPOINT : window.innerWidth,
    drawerOpen: false,
  }),
  withComputed(({ viewportWidth }) => ({
    isNarrow: computed(() => viewportWidth() < LAYOUT_COLLAPSE_BREAKPOINT),
    isMobile: computed(() => viewportWidth() < MOBILE_BREAKPOINT),
  })),
  withMethods((store) => ({
    openDrawer(): void {
      patchState(store, { drawerOpen: true });
    },
    closeDrawer(): void {
      patchState(store, { drawerOpen: false });
    },
  })),
  withHooks((store) => {
    const globalStore = inject(Store);
    const router = inject(Router);
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
        router.events
          .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
          .subscribe(() => {
            store.closeDrawer();
          });
      },
      onDestroy(): void {
        if (typeof window !== 'undefined') window.removeEventListener('resize', update);
      },
    };
  }),
);
