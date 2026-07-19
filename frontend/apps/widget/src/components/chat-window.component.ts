import {
  Component,
  Output,
  EventEmitter,
  inject,
  ChangeDetectionStrategy,
  computed,
  ElementRef,
  HostListener,
} from '@angular/core';
import { WidgetStore } from '../core/widget.store';
import { MessageListComponent } from './message-list.component';
import { ComposerComponent } from './composer.component';
import { HandoffBannerComponent } from './handoff-banner.component';
import { FeedbackPromptComponent } from './feedback-prompt.component';

@Component({
  selector: 'hx-chat-window',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageListComponent, ComposerComponent, HandoffBannerComponent, FeedbackPromptComponent],
  template: `
    <div class="window" role="dialog" aria-label="Chat window">
      <header class="window__header">
        <span class="window__title">{{ store.config()?.displayName ?? 'Chat' }}</span>
        <button class="window__close" (click)="onClose()" aria-label="Close chat">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </header>

      @if (showWelcome()) {
        <div class="window__welcome">
          <div class="bubble bubble--assistant">{{ store.config()?.welcomeMessage }}</div>
        </div>
      }

      <hx-message-list [messages]="store.messages()" [streamingText]="store.streamingText()" />

      @if (store.uiState() === 'error') {
        <div class="window__error">
          <span>Something went wrong.</span>
          <button class="window__retry" (click)="store.retry()">Try again</button>
        </div>
      }

      @if (store.uiState() === 'rate-limited') {
        <div class="window__rate-limited">You're sending messages too fast. Please slow down.</div>
      }

      @if (store.conversation()?.handling === 'human') {
        <hx-handoff-banner [teamOnline]="store.conversation()?.teamOnline ?? false" />
      }

      @if (store.feedbackState() !== 'none') {
        <wgt-feedback-prompt
          [state]="store.feedbackState()"
          [feedback]="store.feedback()"
          (submitRating)="store.submitFeedback($event)"
          (dismiss)="store.dismissFeedback()"
          (expand)="store.expandFeedback()"
        />
      }

      @if (!isClosed() && store.feedbackState() !== 'prompt' && store.feedbackState() !== 'submitted') {
        <hx-composer (sendMessage)="onSend($event)" />
      }
    </div>
  `,
  styles: [
    `
      .window {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--wgt-surface);
        font-family: var(--wgt-font);
        color: var(--wgt-text);
        border-radius: var(--wgt-radius);
        overflow: hidden;
        box-shadow: var(--wgt-shadow);
      }
      .window__header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 14px 16px;
        background: var(--wgt-primary, var(--wgt-bubble-visitor));
        color: #fff;
        flex-shrink: 0;
      }
      .window__title {
        font-weight: 600;
        font-size: 15px;
      }
      .window__close {
        background: none;
        border: none;
        color: #fff;
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        display: flex;
        align-items: center;
        justify-content: center;
        opacity: 0.8;
      }
      .window__close:hover {
        opacity: 1;
      }
      .window__welcome {
        padding: 8px 12px 0;
        display: flex;
      }
      .bubble--assistant {
        max-width: 80%;
        padding: 10px 14px;
        border-radius: 12px;
        font-size: 14px;
        background: var(--wgt-bubble-assistant);
        color: var(--wgt-text);
        align-self: flex-start;
        border-bottom-left-radius: 4px;
      }
      .window__error {
        padding: 12px;
        text-align: center;
        color: var(--wgt-muted-text);
        font-size: 13px;
        display: flex;
        flex-direction: column;
        gap: 8px;
        align-items: center;
      }
      .window__retry {
        background: var(--wgt-primary, var(--wgt-bubble-visitor));
        color: #fff;
        border: none;
        border-radius: 6px;
        padding: 6px 16px;
        cursor: pointer;
        font-size: 13px;
      }
      .window__rate-limited {
        padding: 12px;
        text-align: center;
        color: var(--wgt-muted-text);
        font-size: 13px;
      }
    `,
  ],
})
export class ChatWindowComponent {
  readonly store = inject(WidgetStore);
  private readonly elementRef = inject(ElementRef);

  @Output() closeWindow = new EventEmitter<void>();
  @Output() resizeWindow = new EventEmitter<{ width: number; height: number }>();

  @HostListener('keydown', ['$event'])
  onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      this.onClose();
      return;
    }

    if (event.key === 'Tab') {
      const focusable = this.getFocusableElements();
      if (focusable.length === 0) return;

      if (event.shiftKey) {
        if (document.activeElement === focusable[0]) {
          event.preventDefault();
          (focusable[focusable.length - 1] as HTMLElement).focus();
        }
      } else {
        if (document.activeElement === focusable[focusable.length - 1]) {
          event.preventDefault();
          (focusable[0] as HTMLElement).focus();
        }
      }
    }
  }

  private getFocusableElements(): HTMLElement[] {
    const hostEl: HTMLElement = this.elementRef.nativeElement;
    const nodes = hostEl.querySelectorAll(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    return Array.prototype.filter.call(nodes, (el: HTMLElement) => !el.hasAttribute('disabled'));
  }

  showWelcome = computed(() => {
    const config = this.store.config();
    const messages = this.store.messages();
    const streaming = this.store.streamingText();
    return config?.welcomeMessage != null && messages.length === 0 && !streaming;
  });

  isClosed = computed(() => this.store.conversation()?.handling === 'closed');

  onSend(body: string): void {
    const conv = this.store.conversation();
    if (conv) {
      this.store.sendMessage(body, conv.id);
    }
  }

  onClose(): void {
    this.store.close();
    this.closeWindow.emit();
  }
}
