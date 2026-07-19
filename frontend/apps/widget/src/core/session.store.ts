import { Injectable, signal, inject } from '@angular/core';
import { WidgetApiService, WIDGET_API_BASE } from './widget-api.service';

@Injectable({ providedIn: 'root' })
export class SessionStore {
  private readonly api = inject(WidgetApiService);
  private readonly base = inject(WIDGET_API_BASE);

  private token = signal<string | null>(null);
  private widgetId = signal<string>('');
  private inMemoryToken: string | null = null;

  readonly token$ = this.token.asReadonly();

  private storageKey(widgetId: string): string {
    return `hx_widget_session_${widgetId}`;
  }

  private readStorage(widgetId: string): string | null {
    try {
      return localStorage.getItem(this.storageKey(widgetId));
    } catch {
      return this.inMemoryToken;
    }
  }

  private writeStorage(widgetId: string, token: string): void {
    try {
      localStorage.setItem(this.storageKey(widgetId), token);
    } catch {
      this.inMemoryToken = token;
    }
  }

  private removeStorage(widgetId: string): void {
    try {
      localStorage.removeItem(this.storageKey(widgetId));
    } catch {
      this.inMemoryToken = null;
    }
  }

  init(widgetId: string): void {
    this.widgetId.set(widgetId);
    const stored = this.readStorage(widgetId);
    if (stored) {
      this.token.set(stored);
    } else {
      this.mint(widgetId);
    }
  }

  mint(widgetId: string): void {
    this.api.createSession(widgetId).subscribe({
      next: (res) => {
        this.token.set(res.sessionToken);
        this.writeStorage(widgetId, res.sessionToken);
      },
    });
  }

  getToken(): string | null {
    return this.token();
  }

  handleExpired(): void {
    const wid = this.widgetId();
    this.token.set(null);
    this.removeStorage(wid);
    this.mint(wid);
  }
}
