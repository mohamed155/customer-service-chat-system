import { Injectable } from '@angular/core';
import { Observable, Subscriber } from 'rxjs';
import { WidgetEvent } from './models';

@Injectable({ providedIn: 'root' })
export class WidgetSseClient {
  private backoff = 1000;
  private readonly MAX_BACKOFF = 30_000;
  private abortController: AbortController | null = null;

  stream(url: string, token: string): Observable<WidgetEvent> {
    this.backoff = 1000;
    return new Observable<WidgetEvent>((subscriber) => {
      this.connect(url, token, subscriber);
      return () => this.abortController?.abort();
    });
  }

  private connect(url: string, token: string, subscriber: Subscriber<WidgetEvent>): void {
    this.abortController?.abort();
    this.abortController = new AbortController();

    fetch(url, {
      headers: { Authorization: `Bearer ${token}` },
      signal: this.abortController.signal,
    })
      .then((response) => {
        if (!response.ok || !response.body) {
          this.scheduleReconnect(url, token, subscriber);
          return;
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buf = '';
        let currentEvent = '';

        const read = (): void => {
          reader
            .read()
            .then(({ done, value }) => {
              if (done) {
                this.scheduleReconnect(url, token, subscriber);
                return;
              }
              buf += decoder.decode(value, { stream: true });
              const lines = buf.split('\n');
              buf = lines.pop() ?? '';

              for (const line of lines) {
                if (line.startsWith('event: ')) {
                  currentEvent = line.slice(7).trim();
                } else if (line.startsWith('data: ')) {
                  const data = line.slice(6);
                  if (currentEvent) {
                    try {
                      const parsed = JSON.parse(data) as WidgetEvent & { event?: string };
                      subscriber.next(parsed);
                    } catch {}
                    currentEvent = '';
                  }
                }
              }

              read();
            })
            .catch(() => {
              this.scheduleReconnect(url, token, subscriber);
            });
        };

        read();
      })
      .catch(() => {
        this.scheduleReconnect(url, token, subscriber);
      });
  }

  private scheduleReconnect(url: string, token: string, subscriber: Subscriber<WidgetEvent>): void {
    setTimeout(() => {
      this.backoff = Math.min(this.backoff * 2, this.MAX_BACKOFF);
      subscriber.next({ type: 'conversation.updated' as never } as unknown as WidgetEvent);
      this.connect(url, token, subscriber);
    }, this.backoff);
  }
}
