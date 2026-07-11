import { inject, Signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRouteSnapshot, NavigationEnd, Router } from '@angular/router';
import { filter, map } from 'rxjs';

import { PAGE_TITLES } from './page-title';
import type { PageTitleKey } from './page-title';

export interface Crumb {
  readonly label: string;
  readonly link: string | null;
}

function readCrumbs(router: Router): Crumb[] {
  const segments: string[] = [];
  const pageCrumbs: Crumb[] = [];

  let route: ActivatedRouteSnapshot | null = router.routerState.snapshot.root;
  while (route) {
    for (const segment of route.url) {
      segments.push(segment.path);
    }

    const pageTitleKey = route.data['pageTitle'] as PageTitleKey | undefined;
    if (pageTitleKey) {
      const entry = PAGE_TITLES[pageTitleKey];
      if (entry) {
        pageCrumbs.push({
          label: entry.title,
          link: '/' + segments.join('/'),
        });
      }
    }

    route = route.firstChild;
  }

  const firstSegment = segments[0] ?? '';
  let areaLabel: string;
  if (firstSegment === 'tenant') {
    areaLabel = 'Workspace';
  } else if (firstSegment === 'platform') {
    areaLabel = 'Platform';
  } else {
    areaLabel = 'Home';
  }

  const areaLink: string | null =
    areaLabel === 'Workspace'
      ? '/tenant/overview'
      : areaLabel === 'Platform'
        ? '/platform/overview-placeholder'
        : null;

  const crumbs: Crumb[] = [{ label: areaLabel, link: areaLink }, ...pageCrumbs];

  if (crumbs.length > 0) {
    crumbs[crumbs.length - 1] = { ...crumbs[crumbs.length - 1], link: null };
  }

  return crumbs;
}

export function injectBreadcrumbs(): Signal<Crumb[]> {
  const router = inject(Router);

  return toSignal(
    router.events.pipe(
      filter((event): event is NavigationEnd => event instanceof NavigationEnd),
      map(() => readCrumbs(router)),
    ),
    { initialValue: readCrumbs(router) },
  );
}
