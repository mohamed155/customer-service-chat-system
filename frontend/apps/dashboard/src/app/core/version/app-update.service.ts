import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import { PageReloader } from './page-reloader';

/** How often an open, visible page asks the server whether a new release is out. */
export const UPDATE_CHECK_INTERVAL_MS = 5 * 60_000;
/** A page that comes back to the foreground checks at once, unless it checked within this time. */
export const UPDATE_CHECK_MIN_GAP_MS = 60_000;

/**
 * The hashed script and stylesheet names an `index.html` points to, in a fixed order, or `null`
 * when it names none (a development build, or something that is not the app's page).
 *
 * The build gives every file a content hash, so these names change whenever the code does and
 * `index.html` is the manifest of a release. Comparing them needs no extra file from the build.
 */
export function releaseFingerprint(html: string): string | null {
  const names = new Set<string>();
  for (const match of html.matchAll(/(?:src|href)="([^"]*-[A-Z0-9]{8}\.(?:js|css))"/g)) {
    names.add(match[1]);
  }
  return names.size ? [...names].sort().join('|') : null;
}

/**
 * Knows whether a newer release of the dashboard than the one running is out (MOH-167).
 *
 * `start()` fingerprints the page the browser loaded and, from then on, fetches `index.html`
 * fresh every few minutes and when the tab comes back to the foreground. A different fingerprint
 * makes `updateAvailable` true; the update banner offers a reload. The chunk-load recovery marks
 * an update available too when it must not reload by itself.
 */
@Injectable({ providedIn: 'root' })
export class AppUpdateService {
  private readonly document = inject(DOCUMENT);
  private readonly reloader = inject(PageReloader);
  private readonly destroyRef = inject(DestroyRef);

  private readonly available = signal(false);
  private readonly hidden = signal(false);
  private running = false;
  private lastCheckAt = 0;

  /** A newer release is out and the person has not dismissed the notice. */
  readonly updateAvailable = this.available.asReadonly();
  readonly bannerVisible = () => this.available() && !this.hidden();

  markUpdateAvailable(): void {
    this.available.set(true);
    this.hidden.set(false);
  }

  dismiss(): void {
    this.hidden.set(true);
  }

  reload(): void {
    this.reloader.reload();
  }

  /** Start the periodic check. Does nothing in a development build, which has no hashed files. */
  start(): void {
    if (this.running) return;
    const current = releaseFingerprint(this.document.documentElement.outerHTML);
    if (!current) return;
    this.running = true;

    const timer = setInterval(() => {
      if (!this.document.hidden) void this.check(current);
    }, UPDATE_CHECK_INTERVAL_MS);
    const onVisible = () => {
      if (!this.document.hidden && Date.now() - this.lastCheckAt >= UPDATE_CHECK_MIN_GAP_MS) {
        void this.check(current);
      }
    };
    this.document.addEventListener('visibilitychange', onVisible);
    this.destroyRef.onDestroy(() => {
      clearInterval(timer);
      this.document.removeEventListener('visibilitychange', onVisible);
    });
  }

  /** Fetch the current `index.html` and compare. A failed fetch says nothing, so it is ignored. */
  async check(current: string): Promise<void> {
    if (this.available()) return;
    this.lastCheckAt = Date.now();
    try {
      const response = await fetch(new URL('index.html', this.document.baseURI), {
        cache: 'no-store',
        credentials: 'omit',
      });
      if (!response.ok) return;
      const latest = releaseFingerprint(await response.text());
      if (latest && latest !== current) this.markUpdateAvailable();
    } catch {
      // Offline or the server is restarting: try again at the next interval.
    }
  }
}
