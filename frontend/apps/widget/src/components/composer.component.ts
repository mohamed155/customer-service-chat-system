import { Component, Output, EventEmitter, ChangeDetectionStrategy, signal } from '@angular/core';

@Component({
  selector: 'hx-composer',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="composer">
      <div class="composer__input-wrap">
        <textarea
          #textarea
          class="composer__input"
          [value]="text()"
          (input)="onInput($event)"
          (keydown)="onKeydown($event)"
          placeholder="Type your message…"
          rows="1"
          aria-label="Message input"
        ></textarea>
        @if (text().length > 3500) {
          <div class="composer__counter" [class.composer__counter--warn]="text().length >= 4000">
            {{ text().length }}/4000
          </div>
        }
      </div>
      <button
        class="composer__send"
        [disabled]="!canSend()"
        (click)="onSend()"
        aria-label="Send message"
      >
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <line x1="22" y1="2" x2="11" y2="13"></line>
          <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
        </svg>
      </button>
    </div>
  `,
  styles: [
    `
      .composer {
        display: flex;
        align-items: flex-end;
        gap: 8px;
        padding: 8px 12px;
        border-top: 1px solid var(--wgt-border);
        background: var(--wgt-surface);
      }
      .composer__input-wrap {
        flex: 1;
        position: relative;
        display: flex;
        flex-direction: column;
      }
      .composer__input {
        width: 100%;
        border: 1px solid var(--wgt-border);
        border-radius: 8px;
        padding: 8px 12px;
        font-size: 14px;
        font-family: var(--wgt-font);
        color: var(--wgt-text);
        background: var(--wgt-surface);
        resize: none;
        outline: none;
        box-sizing: border-box;
        max-height: 120px;
        line-height: 1.4;
      }
      .composer__input:focus {
        border-color: var(--wgt-primary, var(--wgt-bubble-visitor));
      }
      .composer__counter {
        font-size: 11px;
        color: var(--wgt-muted-text);
        text-align: right;
        padding-top: 2px;
      }
      .composer__counter--warn {
        color: #ef4444;
      }
      .composer__send {
        width: 36px;
        height: 36px;
        border: none;
        border-radius: 50%;
        background: var(--wgt-primary, var(--wgt-bubble-visitor));
        color: #fff;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        transition: opacity 0.15s;
      }
      .composer__send:disabled {
        opacity: 0.4;
        cursor: default;
      }
      .composer__send:not(:disabled):hover {
        opacity: 0.85;
      }
    `,
  ],
})
export class ComposerComponent {
  @Output() sendMessage = new EventEmitter<string>();

  text = signal('');
  canSend = signal(false);

  onInput(event: Event): void {
    const el = event.target as HTMLTextAreaElement;
    const value = el.value;
    if (value.length > 4000) {
      el.value = value.slice(0, 4000);
      this.text.set(el.value);
      return;
    }
    this.text.set(value);
    const trimmed = value.trim();
    this.canSend.set(trimmed.length > 0 && trimmed.length <= 4000);
  }

  onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.onSend();
    }
  }

  onSend(): void {
    const trimmed = this.text().trim();
    if (!trimmed || trimmed.length > 4000) return;
    this.sendMessage.emit(trimmed);
    this.text.set('');
    this.canSend.set(false);
  }
}
