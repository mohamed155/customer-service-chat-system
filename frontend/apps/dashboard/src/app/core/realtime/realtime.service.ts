import { inject, Injectable } from '@angular/core';
import { Store } from '@ngrx/store';
import { Observable, Subject } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { selectActiveTenant } from '../state/tenant-context.feature';
import { TenantScope } from '../state/tenant-scope';

export interface SseEvent {
  event: string;
  id: string;
  data: string;
}

/** The first reconnect waits at most this long; each failure doubles the ceiling. */
export const RECONNECT_BASE_MS = 1_000;
/** The ceiling of the wait never goes above this. */
export const RECONNECT_CAP_MS = 30_000;

/**
 * How long to wait before the next reconnect, with **full jitter**: a random time between zero and
 * `min(cap, base * 2^attempt)` (`attempt` counts from 0). After a backend restart every open
 * dashboard reconnects, and a fixed or merely doubling delay would send them all at the same
 * moments (a thundering herd); a random wait spreads them out (Principle XI).
 */
export function reconnectDelay(attempt: number, random: () => number = Math.random): number {
  const ceiling = Math.min(RECONNECT_CAP_MS, RECONNECT_BASE_MS * 2 ** attempt);
  return Math.floor(random() * ceiling);
}

/**
 * The tenant event stream (Server-Sent Events over `fetch`, because `EventSource` cannot send the
 * `X-Tenant-ID` header).
 *
 * A stream that ends or fails is opened again after a jittered, growing wait; a successful
 * connection starts the count over. The server keeps no event log and sends no resumable ids
 * (`/tenant/events` is an in-process broadcast, ADR-009), so it ignores `Last-Event-ID`: events
 * sent while the stream was down are lost, and a screen that must be right after a gap has to
 * reload its data.
 */
@Injectable({ providedIn: 'root' })
export class RealtimeService {
  private readonly store = inject(Store);
  private readonly config = inject(APP_CONFIG);
  private readonly events$ = new Subject<SseEvent>();
  private abortController?: AbortController;
  private retryTimer?: ReturnType<typeof setTimeout>;
  /** Failed or ended connections in a row; a successful connection resets it. */
  private attempt = 0;

  constructor() {
    // The stream carries the workspace it was opened for. After a switch it must be opened again
    // for the new one, or the old workspace's events would reach the new one's stores (MOH-61).
    inject(TenantScope).changes$.subscribe(({ to }) => {
      if (!this.abortController) return;
      if (to === null) {
        this.disconnect();
      } else {
        this.connect();
      }
    });
  }

  events(): Observable<SseEvent> {
    return this.events$;
  }

  /** Open the stream for the active workspace, closing any earlier one. */
  connect(): void {
    this.disconnect();
    this.open();
  }

  /** Open a stream. Unlike `connect()` it keeps the reconnect count. */
  private open(): void {
    this.abortController = new AbortController();
    const signal = this.abortController.signal;
    const tenantId = this.store.selectSignal(selectActiveTenant)()?.id;
    const url = `${this.config.apiBaseUrl.replace(/\/$/, '')}/tenant/events`;

    const headers: Record<string, string> = {};
    if (tenantId) headers['X-Tenant-ID'] = tenantId;

    const start = async () => {
      try {
        const response = await fetch(url, {
          headers: { ...headers, Accept: 'text/event-stream' },
          credentials: 'include',
          signal,
        });

        if (!response.ok || !response.body) {
          throw new Error(`SSE connection failed: ${response.status}`);
        }
        // Connected: the next failure starts the backoff over.
        this.attempt = 0;

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const frames = buffer.split('\n\n');
          buffer = frames.pop() ?? '';

          for (const frame of frames) {
            if (!frame.trim()) continue;
            const lines = frame.split('\n');
            let event = '';
            let id = '';
            let data = '';

            for (const line of lines) {
              if (line.startsWith('event: ')) event = line.slice(7);
              else if (line.startsWith('id: ')) id = line.slice(4);
              else if (line.startsWith('data: ')) data = line.slice(6);
            }

            if (event && data) {
              this.events$.next({ event, id, data });
            }
          }
        }
      } catch (err: unknown) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
      }

      if (!signal.aborted) {
        this.scheduleReconnect();
      }
    };

    start();
  }

  private scheduleReconnect(): void {
    const delay = reconnectDelay(this.attempt);
    this.attempt += 1;
    this.retryTimer = setTimeout(() => {
      this.retryTimer = undefined;
      this.open();
    }, delay);
  }

  /** Close the stream and cancel a reconnect that is waiting. */
  disconnect(): void {
    clearTimeout(this.retryTimer);
    this.retryTimer = undefined;
    this.abortController?.abort();
    this.abortController = undefined;
    this.attempt = 0;
  }
}
