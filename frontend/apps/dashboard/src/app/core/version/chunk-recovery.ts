import { DOCUMENT, Location } from '@angular/common';
import { inject } from '@angular/core';
import { NavigationError } from '@angular/router';
import { ClientErrorReporter } from '../errors/client-error-reporter';
import { AppUpdateService } from './app-update.service';
import { isChunkLoadError } from './chunk-load-error';
import { PageReloader } from './page-reloader';

/** Session storage key of the reload guard: `{ url, at }` of the last automatic reload. */
export const CHUNK_RELOAD_KEY = 'app.chunk-reload';
/** A second chunk failure within this time of a reload is not answered with another reload. */
export const CHUNK_RELOAD_WINDOW_MS = 60_000;

/**
 * Router navigation-error handler (`withNavigationErrorHandler`) for a lazy route whose files are
 * gone because a new release was deployed after this page was loaded (MOH-167).
 *
 * The new page has the new file names, so the fix is to load the target URL again, once. The
 * guard sits in `sessionStorage`, which survives the reload: when the failure comes back within
 * a minute (the files are really missing, or the server is down), or when storage is not usable
 * so the guard could not be kept, the page does not reload again and shows the update notice
 * instead. Any other navigation error is left to the router, which rethrows it to the global
 * error handler.
 */
export function recoverFromChunkLoadError(error: NavigationError): void {
  if (!isChunkLoadError(error.error)) return;

  const update = inject(AppUpdateService);
  const target = inject(Location).prepareExternalUrl(error.url);
  const view = inject(DOCUMENT).defaultView;

  // Say so before the page reloads (MOH-168): the report goes out with `keepalive`, so it survives
  // the reload. The router rethrows this error to the global handler afterwards; the reporter has
  // already taken it in and ignores it there.
  const reporter = inject(ClientErrorReporter);
  reporter.report(error.error, 'chunk_load');
  reporter.flush();

  if (!guardAllowsReload(view, target)) {
    update.markUpdateAvailable();
    return;
  }
  inject(PageReloader).reload(target);
}

/** Record the reload and return true, unless one already happened lately or it cannot be recorded. */
function guardAllowsReload(view: Window | null, url: string): boolean {
  try {
    const storage = view?.sessionStorage;
    if (!storage) return false;
    const last = JSON.parse(storage.getItem(CHUNK_RELOAD_KEY) ?? 'null') as {
      at?: unknown;
    } | null;
    if (typeof last?.at === 'number' && Date.now() - last.at < CHUNK_RELOAD_WINDOW_MS) {
      return false;
    }
    storage.setItem(CHUNK_RELOAD_KEY, JSON.stringify({ url, at: Date.now() }));
    return true;
  } catch {
    return false; // storage blocked or full: no guard, so no automatic reload
  }
}
