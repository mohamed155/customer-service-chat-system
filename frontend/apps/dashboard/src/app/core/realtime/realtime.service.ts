import { inject, Injectable, NgZone } from '@angular/core';
import { Store } from '@ngrx/store';
import { Observable, Subject } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { selectActiveTenant } from '../state/tenant-context.feature';

export interface SseEvent {
  event: string;
  id: string;
  data: string;
}

@Injectable({ providedIn: 'root' })
export class RealtimeService {
  private readonly store = inject(Store);
  private readonly config = inject(APP_CONFIG);
  private readonly zone = inject(NgZone);
  private readonly events$ = new Subject<SseEvent>();
  private abortController?: AbortController;
  private retryDelay = 1000;

  events(): Observable<SseEvent> {
    return this.events$;
  }

  connect(): void {
    this.disconnect();
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
              this.zone.run(() => {
                this.events$.next({ event, id, data });
              });
            }
          }
        }
      } catch (err: unknown) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
      }

      if (!this.abortController?.signal.aborted) {
        this.zone.run(() => {
          setTimeout(() => {
            this.retryDelay = Math.min(this.retryDelay * 2, 30000);
            this.connect();
          }, this.retryDelay);
        });
      }
    };

    start();
  }

  disconnect(): void {
    this.abortController?.abort();
    this.abortController = undefined;
    this.retryDelay = 1000;
  }
}
