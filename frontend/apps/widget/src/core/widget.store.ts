import { Injectable, signal, computed, inject } from '@angular/core';
import { Subscription, timer } from 'rxjs';
import {
  WidgetConfig,
  WidgetConversation,
  WidgetMessage,
  WidgetEvent,
  RateLimitedError,
  SessionExpiredError,
} from './models';
import { WidgetApiService } from './widget-api.service';
import { SessionStore } from './session.store';
import { WidgetSseClient } from './widget-sse.client';

export const REPLY_TIMEOUT_MS = 45_000;

export type UiState = 'closed' | 'open' | 'sending' | 'responding' | 'error' | 'rate-limited';

@Injectable({ providedIn: 'root' })
export class WidgetStore {
  private readonly api = inject(WidgetApiService);
  private readonly session = inject(SessionStore);
  private readonly sse = inject(WidgetSseClient);

  private configSignal = signal<WidgetConfig | null>(null);
  private conversationSignal = signal<WidgetConversation | null>(null);
  private messagesSignal = signal<WidgetMessage[]>([]);
  private streamingTextSignal = signal('');
  private uiStateSignal = signal<UiState>('closed');
  private sseSubscription: Subscription | null = null;
  private replyTimer: Subscription | null = null;
  private initialized = false;

  readonly config = this.configSignal.asReadonly();
  readonly conversation = this.conversationSignal.asReadonly();
  readonly messages = this.messagesSignal.asReadonly();
  readonly streamingText = this.streamingTextSignal.asReadonly();
  readonly uiState = this.uiStateSignal.asReadonly();

  readonly isAiResponding = computed(() => {
    const state = this.uiStateSignal();
    return state === 'responding' || state === 'sending';
  });

  setConfig(config: WidgetConfig): void {
    this.configSignal.set(config);
  }

  initSession(widgetId: string): void {
    if (this.initialized) return;
    this.initialized = true;
    this.session.init(widgetId);
  }

  open(): void {
    const config = this.configSignal();
    if (!config) return;

    const token = this.session.getToken();
    if (token) {
      this.api.getConversation(token).subscribe({
        next: (res) => {
          if (res.data?.conversation) {
            const conv = res.data.conversation;
            this.conversationSignal.set(conv);
            this.messagesSignal.set(conv.messages);
            this.uiStateSignal.set('open');
            this.connectSse(conv.id);
            if (conv.handling === 'human') {
              this.cancelReplyTimer();
            }
          } else {
            this.createConversation();
          }
        },
        error: () => {
          this.createConversation();
        },
      });
    } else {
      this.createConversation();
    }
  }

  private createConversation(): void {
    const token = this.session.getToken();
    if (!token) return;

    this.uiStateSignal.set('open');
    this.api.createConversation(token).subscribe({
      next: (conv) => {
        this.conversationSignal.set(conv);
        this.messagesSignal.set(conv.messages);
        this.connectSse(conv.id);
      },
    });
  }

  sendMessage(body: string, conversationId: string): void {
    const token = this.session.getToken();
    if (!token) return;

    const optimistic: WidgetMessage = {
      id: `opt-${Date.now()}`,
      sender: 'visitor',
      body,
      createdAt: new Date().toISOString(),
    };

    this.messagesSignal.update((m) => [...m, optimistic]);
    this.uiStateSignal.set('responding');
    this.startReplyTimer();

    this.api.sendMessage(token, conversationId, body).subscribe({
      next: (msg) => {
        this.messagesSignal.update((m) => m.map((x) => (x.id === optimistic.id ? msg : x)));
      },
      error: (err) => {
        this.messagesSignal.update((m) => m.filter((x) => x.id !== optimistic.id));
        if (err instanceof RateLimitedError) {
          this.uiStateSignal.set('rate-limited');
        } else if (err instanceof SessionExpiredError) {
          this.session.handleExpired();
          this.uiStateSignal.set('error');
        } else {
          this.uiStateSignal.set('error');
        }
      },
    });
  }

  private connectSse(conversationId: string): void {
    this.sseSubscription?.unsubscribe();
    const token = this.session.getToken();
    if (!token) return;

    const url = this.api.eventsUrl(token, conversationId);
    const stream = this.sse.stream(url, token);

    this.sseSubscription = stream.subscribe({
      next: (event) => this.handleSseEvent(event),
    });
  }

  private handleSseEvent(event: WidgetEvent): void {
    switch (event.type) {
      case 'ai.delta': {
        this.cancelReplyTimer();
        this.uiStateSignal.set('responding');
        this.streamingTextSignal.update((t) => t + event.text);
        break;
      }
      case 'message.created': {
        this.cancelReplyTimer();
        this.streamingTextSignal.set('');
        this.uiStateSignal.set('open');
        this.messagesSignal.update((m) => [...m, event.message]);
        break;
      }
      case 'conversation.updated': {
        this.conversationSignal.update((c) =>
          c
            ? {
                ...c,
                handling: event.handling as 'ai' | 'human' | 'closed',
                teamOnline: event.teamOnline,
              }
            : c,
        );
        if (event.handling === 'human') {
          this.cancelReplyTimer();
          this.streamingTextSignal.set('');
        }
        if (event.handling === 'closed') {
          this.cancelReplyTimer();
          this.streamingTextSignal.set('');
        }
        break;
      }
    }
  }

  private startReplyTimer(): void {
    this.cancelReplyTimer();
    this.replyTimer = timer(REPLY_TIMEOUT_MS).subscribe(() => {
      const state = this.uiStateSignal();
      if (state === 'responding' || state === 'sending') {
        this.uiStateSignal.set('error');
      }
    });
  }

  cancelReplyTimer(): void {
    this.replyTimer?.unsubscribe();
    this.replyTimer = null;
  }

  close(): void {
    this.uiStateSignal.set('closed');
  }

  retry(): void {
    this.uiStateSignal.set('open');
  }

  resetStreaming(): void {
    this.streamingTextSignal.set('');
  }

  handleClosedConversation(): void {
    this.conversationSignal.set(null);
    this.messagesSignal.set([]);
    this.streamingTextSignal.set('');
    this.createConversation();
  }

  destroy(): void {
    this.sseSubscription?.unsubscribe();
    this.cancelReplyTimer();
  }
}
