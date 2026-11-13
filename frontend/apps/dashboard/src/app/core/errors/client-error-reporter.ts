import { DOCUMENT } from '@angular/common';
import { DestroyRef, Injectable, inject } from '@angular/core';
import { ActivatedRouteSnapshot, Router } from '@angular/router';
import { Store } from '@ngrx/store';
import type { components } from '../api/generated/api';
import { APP_CONFIG } from '../config/app-config';
import { selectActiveTenant } from '../state/tenant-context.feature';
import { CurrentUserService } from '../tenant/current-user.service';
import { isChunkLoadError } from '../version/chunk-load-error';
import { describeError, ErrorDescription } from './scrub';

type ClientErrorKind = components['schemas']['ClientErrorKind'];
type ClientErrorReport = components['schemas']['ClientErrorReport'];

/** Errors wait this long to be sent together. */
export const REPORT_FLUSH_MS = 5_000;
/** Reports per request. Small enough that a `keepalive` body (64 KiB at most) always fits. */
export const MAX_BATCH = 10;
/** Distinct errors waiting to be sent; more are dropped. */
export const MAX_PENDING = 50;
/** Reports sent per rolling minute, and in one page session. */
export const MAX_PER_MINUTE = 20;
export const MAX_PER_SESSION = 100;
/** An error sent less than this long ago is not sent again. */
export const DEDUPE_WINDOW_MS = 60_000;
/** After the server refuses (too many reports, or no session) nothing is sent for this long. */
export const PAUSE_AFTER_REFUSAL_MS = 60_000;

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const REQUEST_ID = /^req_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

interface Pending {
  readonly error: unknown;
  readonly description: ErrorDescription;
  readonly kind: ClientErrorKind | undefined;
  readonly route: string | undefined;
  readonly tenantId: string | undefined;
  count: number;
}

/**
 * Sends the errors the dashboard could not recover from to the backend (MOH-168), so they appear in
 * the backend's logs and in `frontend_errors_total` instead of only when a user complains.
 *
 * Errors reach it from `GlobalErrorHandler` (Angular routes `error` and `unhandledrejection` events
 * there) and from the chunk-load recovery. Before anything is sent it is:
 * - **scrubbed** (`describeError`): only a message, a stack, a route pattern and a few IDs leave,
 *   never a form value, a body, a cookie or a token;
 * - **deduplicated**: the same error waiting to be sent only raises a count, and one sent in the
 *   last minute is not sent again;
 * - **batched** (every five seconds, ten reports at most) and **rate limited** (twenty a minute,
 *   a hundred a page session; paused for a minute after a refusal).
 *
 * It uses `fetch` with `keepalive`, so a request started as the page closes still goes out, and
 * not `HttpClient`: the interceptors would retry it, show its failure to the user and report that
 * failure in turn. Only a signed-in user's errors are sent (the endpoint needs a session).
 */
@Injectable({ providedIn: 'root' })
export class ClientErrorReporter {
  private readonly document = inject(DOCUMENT);
  private readonly config = inject(APP_CONFIG);
  private readonly store = inject(Store);
  private readonly router = inject(Router);
  private readonly currentUser = inject(CurrentUserService);
  private readonly destroyRef = inject(DestroyRef);
  private readonly activeTenant = this.store.selectSignal(selectActiveTenant);

  private readonly pending = new Map<string, Pending>();
  private readonly sentAt = new Map<string, number>();
  private readonly sentTimes: number[] = [];
  /** Error objects already taken in, so one error is never reported twice (by two paths). */
  private readonly seen = new WeakSet<object>();
  /** Reasons of unhandled promise rejections: Angular passes only the reason to the handler. */
  private readonly rejections = new WeakSet<object>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private pausedUntil = 0;
  private sentTotal = 0;
  private started = false;

  /** Listen for page events. Call once, at start-up. */
  start(): void {
    const view = this.document.defaultView;
    if (this.started || !view) return;
    this.started = true;
    const mark = (event: PromiseRejectionEvent) => {
      if (typeof event.reason === 'object' && event.reason !== null)
        this.rejections.add(event.reason);
    };
    const hide = () => this.flush();
    const onVisibility = () => {
      if (this.document.visibilityState === 'hidden') this.flush();
    };
    view.addEventListener('unhandledrejection', mark, true);
    view.addEventListener('pagehide', hide);
    this.document.addEventListener('visibilitychange', onVisibility);
    this.destroyRef.onDestroy(() => {
      view.removeEventListener('unhandledrejection', mark, true);
      view.removeEventListener('pagehide', hide);
      this.document.removeEventListener('visibilitychange', onVisibility);
      clearTimeout(this.timer);
    });
  }

  /** Take an error in. It is sent within a few seconds, or by `flush()`. */
  report(error: unknown, kind?: ClientErrorKind): void {
    if (!this.currentUser.currentUser()) return;
    if (typeof error === 'object' && error !== null) {
      if (this.seen.has(error)) return;
      this.seen.add(error);
    }
    const description = describeError(error);
    const fingerprint = `${description.message}|${firstFrame(description.stack)}`;

    const waiting = this.pending.get(fingerprint);
    if (waiting) {
      waiting.count += 1;
      return;
    }
    const lastSent = this.sentAt.get(fingerprint);
    if (lastSent !== undefined && Date.now() - lastSent < DEDUPE_WINDOW_MS) return;
    if (this.pending.size >= MAX_PENDING) return;

    this.pending.set(fingerprint, {
      error,
      description,
      kind,
      route: this.routePattern(),
      tenantId: this.activeTenant()?.id,
      count: 1,
    });
    if (this.pending.size >= MAX_BATCH) this.flush();
    else this.timer ??= setTimeout(() => this.flush(), REPORT_FLUSH_MS);
  }

  /** Send what is waiting now (page hidden, or about to reload). */
  flush(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    if (this.pending.size === 0) return;

    const now = Date.now();
    if (now < this.pausedUntil) {
      this.pending.clear();
      return;
    }
    while (this.sentTimes.length && now - this.sentTimes[0] >= 60_000) this.sentTimes.shift();

    const batch: ClientErrorReport[] = [];
    for (const [fingerprint, item] of this.pending) {
      if (
        batch.length >= MAX_BATCH ||
        this.sentTimes.length + batch.length >= MAX_PER_MINUTE ||
        this.sentTotal + batch.length >= MAX_PER_SESSION
      ) {
        break;
      }
      this.pending.delete(fingerprint);
      this.sentAt.set(fingerprint, now);
      batch.push(this.toReport(item));
    }
    if (this.pending.size > 0) {
      // Over a budget or over one batch. What is over the minute's budget is dropped; the rest
      // goes out in the next batch.
      if (
        this.sentTimes.length + batch.length >= MAX_PER_MINUTE ||
        this.sentTotal + batch.length >= MAX_PER_SESSION
      ) {
        this.pending.clear();
      } else {
        this.timer = setTimeout(() => this.flush(), REPORT_FLUSH_MS);
      }
    }
    if (batch.length === 0) return;
    for (let i = 0; i < batch.length; i += 1) this.sentTimes.push(now);
    this.sentTotal += batch.length;
    this.send({ reports: batch });
  }

  private toReport(item: Pending): ClientErrorReport {
    const { description } = item;
    const kind =
      item.kind ??
      (isChunkLoadError(item.error)
        ? 'chunk_load'
        : typeof item.error === 'object' && item.error !== null && this.rejections.has(item.error)
          ? 'unhandled_rejection'
          : 'error');
    return {
      kind,
      message: description.message,
      stack: description.stack,
      route: item.route,
      release: this.release(),
      request_id:
        description.requestId && REQUEST_ID.test(description.requestId)
          ? description.requestId
          : undefined,
      tenant_id: item.tenantId && UUID.test(item.tenantId) ? item.tenantId : undefined,
      count: item.count > 1 ? Math.min(item.count, 1000) : undefined,
    };
  }

  private send(batch: components['schemas']['ClientErrorBatch']): void {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      // The session cookie rides on this request, so it carries the header the backend requires
      // of every state-changing cookie request (MOH-36).
      'X-Requested-With': 'XMLHttpRequest',
    };
    if (this.config.environmentName === 'development') {
      const devUserId = safeStorageGet('app.devUserId');
      if (devUserId) headers['X-Dev-User-Id'] = devUserId;
    }
    const url = `${this.config.apiBaseUrl.replace(/\/$/, '')}/client-errors`;
    void fetch(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(batch),
      keepalive: true,
      credentials: 'include',
    })
      .then((response) => {
        // Too many reports, or the session is gone: stop for a while rather than keep trying.
        if (response.status === 429 || response.status === 401 || response.status === 403) {
          this.pausedUntil = Date.now() + PAUSE_AFTER_REFUSAL_MS;
        }
      })
      .catch(() => {
        // The network is down or the page is closing: a report is not worth a retry.
      });
  }

  /** The route pattern (`tenant/customers/:id`), never the URL with its values. */
  private routePattern(): string | undefined {
    const parts: string[] = [];
    let route: ActivatedRouteSnapshot | null | undefined = this.router.routerState?.snapshot?.root;
    while (route) {
      const path = route.routeConfig?.path;
      if (path) parts.push(path);
      route = route.firstChild ?? null;
    }
    return parts.length ? parts.join('/').slice(0, 200) : undefined;
  }

  /** The dashboard release: the hash in the name of its main script (`main-4TYQZ6FJ.js`). */
  private release(): string {
    const scripts = this.document.querySelectorAll('script[src]');
    for (const script of Array.from(scripts)) {
      const match = /main-([A-Za-z0-9_-]{8})\.js/.exec(script.getAttribute('src') ?? '');
      if (match) return match[1];
    }
    return 'dev';
  }
}

/** The first line of a stack that names a place, so two errors from one place group together. */
function firstFrame(stack: string | undefined): string {
  return (
    stack
      ?.split('\n')
      .find((line) => line.trim().startsWith('at '))
      ?.trim() ?? ''
  );
}

function safeStorageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
