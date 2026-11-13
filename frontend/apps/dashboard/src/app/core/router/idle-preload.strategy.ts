import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';
import { PreloadingStrategy, Route } from '@angular/router';
import {
  EMPTY,
  Observable,
  catchError,
  defer,
  fromEvent,
  map,
  of,
  switchMap,
  take,
  timer,
} from 'rxjs';

/** The browser's Network Information API: not in every browser, not in the DOM typings. */
interface NetworkInformation {
  readonly saveData?: boolean;
  readonly effectiveType?: string;
}

/** How long to wait when the browser has no idle callback. */
export const FALLBACK_IDLE_MS = 2000;

/**
 * How long after the page has finished loading before the first preload starts. Preloading right
 * away competed with the first page for a slow connection: Lighthouse (mobile, slow 4G) scored
 * Overview and Inbox 2 points lower and measured a later largest contentful paint (MOH-171).
 */
export const PRELOAD_DELAY_MS = 3000;

/**
 * Route preloading (MOH-171): a route marked `data: { preload: true }` is fetched once the first
 * page has finished loading, a few seconds have passed and the browser is idle, so the next click
 * on it needs no download and the files never compete with the page being loaded. Everything
 * else loads when it is opened, and so does every route on a slow or data-saving connection.
 *
 * An area route (`loadChildren`) has to be marked as well: its children are only looked at once
 * the area itself has been loaded. A failed preload is ignored. The page loads the file again when
 * the user opens it, and `recoverFromChunkLoadError` handles a file a deploy removed.
 */
@Injectable({ providedIn: 'root' })
export class IdlePreloadStrategy implements PreloadingStrategy {
  private readonly view = inject(DOCUMENT).defaultView;

  preload(route: Route, load: () => Observable<unknown>): Observable<unknown> {
    if (route.data?.['preload'] !== true || this.saveData()) return EMPTY;
    return this.loaded().pipe(
      switchMap(() => timer(PRELOAD_DELAY_MS)),
      switchMap(() => this.idle()),
      switchMap(() => load()),
      catchError(() => EMPTY),
    );
  }

  /** Emits once, when the page has finished loading (now, if it already has). */
  private loaded(): Observable<void> {
    return defer(() => {
      const view = this.view;
      if (!view || view.document.readyState === 'complete') return of(undefined);
      return fromEvent(view, 'load').pipe(
        take(1),
        map(() => undefined),
      );
    });
  }

  /** Emits once, when the browser is idle; dropping the subscription cancels the wait. */
  private idle(): Observable<void> {
    return defer(
      () =>
        new Observable<void>((subscriber) => {
          const view = this.view;
          const done = () => {
            subscriber.next();
            subscriber.complete();
          };
          if (view && typeof view.requestIdleCallback === 'function') {
            const handle = view.requestIdleCallback(done);
            return () => view.cancelIdleCallback(handle);
          }
          const timer = setTimeout(done, FALLBACK_IDLE_MS);
          return () => clearTimeout(timer);
        }),
    );
  }

  /** The user asked for less data, or the connection is 2G: leave the files until they are needed. */
  private saveData(): boolean {
    const connection = (this.view?.navigator as { connection?: NetworkInformation } | undefined)
      ?.connection;
    return connection?.saveData === true || /(^|-)2g$/.test(connection?.effectiveType ?? '');
  }
}
