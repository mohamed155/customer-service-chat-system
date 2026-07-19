import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { WidgetInstance } from '../../../core/api/widget.models';

@Component({
  selector: 'app-widget-preview',
  standalone: true,
  template: `
    <div class="preview-container">
      <div
        class="preview-phone"
        [style.--wgt-primary]="formState().primaryColor || '#0066FF'"
        [attr.data-wgt-theme]="formState().theme || 'light'"
      >
        @if (formState().enabled !== false) {
          <button
            class="wgt-launcher"
            [style.background]="formState().primaryColor || '#0066FF'"
            [style.bottom-right]="formState().position === 'bottom-left' ? undefined : '20px'"
            [style.bottom-left]="formState().position === 'bottom-left' ? '20px' : undefined"
            aria-label="Open chat"
          >
            <svg
              width="24"
              height="24"
              viewBox="0 0 24 24"
              fill="none"
              stroke="white"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
          </button>
        }
        <div class="wgt-window">
          <div class="wgt-header" [style.background]="formState().primaryColor || '#0066FF'">
            <span class="wgt-header-name">{{
              formState().displayName || formState().name || 'Chat'
            }}</span>
            <button class="wgt-close" aria-label="Close">&times;</button>
          </div>
          <div class="wgt-messages">
            <div class="wgt-bubble wgt-bubble-assistant">
              {{ formState().welcomeMessage || 'Hello! How can we help you today?' }}
            </div>
          </div>
          <div class="wgt-composer">
            <input
              class="wgt-input"
              type="text"
              placeholder="Type a message..."
              disabled
              value=""
            />
            <button class="wgt-send-btn" disabled>
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <line x1="22" y1="2" x2="11" y2="13" />
                <polygon points="22 2 15 22 11 13 2 9 22 2" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
      }
      .preview-container {
        display: flex;
        justify-content: center;
        align-items: center;
        min-height: 400px;
        background: var(--app-panel-2);
        border-radius: var(--app-radius-xl);
        border: 1px solid var(--app-border);
        overflow: hidden;
        position: relative;
      }
      .preview-phone {
        --wgt-surface: #ffffff;
        --wgt-text: #1a1a2e;
        --wgt-text-muted: #8e8ea0;
        --wgt-border: #e2e2ea;
        --wgt-bubble-visitor: #0066ff;
        --wgt-bubble-assistant: #f0f0f5;
        --wgt-radius: 12px;
        --wgt-space: 8px;
        --wgt-font: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        --wgt-shadow: 0 4px 24px rgba(0, 0, 0, 0.12);
        width: 340px;
        height: 480px;
        border-radius: 16px;
        background: var(--wgt-surface);
        box-shadow: var(--wgt-shadow);
        display: flex;
        flex-direction: column;
        position: relative;
        overflow: hidden;
        font-family: var(--wgt-font);
        color: var(--wgt-text);
      }
      .preview-phone[data-wgt-theme='dark'] {
        --wgt-surface: #1a1a2e;
        --wgt-text: #e2e2ea;
        --wgt-text-muted: #8e8ea0;
        --wgt-border: #2a2a3e;
        --wgt-bubble-assistant: #2a2a3e;
      }
      .wgt-launcher {
        position: absolute;
        bottom: 20px;
        right: 20px;
        width: 56px;
        height: 56px;
        border-radius: 50%;
        border: none;
        display: grid;
        place-items: center;
        cursor: pointer;
        box-shadow: var(--wgt-shadow);
        z-index: 10;
        color: white;
      }
      .wgt-window {
        display: flex;
        flex-direction: column;
        height: 100%;
      }
      .wgt-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 14px 16px;
        color: white;
      }
      .wgt-header-name {
        font-weight: 600;
        font-size: 15px;
      }
      .wgt-close {
        background: none;
        border: none;
        color: rgba(255, 255, 255, 0.85);
        font-size: 22px;
        cursor: pointer;
        line-height: 1;
      }
      .wgt-messages {
        flex: 1;
        overflow-y: auto;
        padding: var(--wgt-space);
        display: flex;
        flex-direction: column;
        gap: var(--wgt-space);
      }
      .wgt-bubble {
        max-width: 80%;
        padding: 10px 14px;
        border-radius: var(--wgt-radius);
        font-size: 14px;
        line-height: 1.4;
      }
      .wgt-bubble-assistant {
        align-self: flex-start;
        background: var(--wgt-bubble-assistant);
        color: var(--wgt-text);
        border-bottom-left-radius: 4px;
      }
      .wgt-composer {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 10px 12px;
        border-top: 1px solid var(--wgt-border);
      }
      .wgt-input {
        flex: 1;
        height: 38px;
        padding: 0 12px;
        border: 1px solid var(--wgt-border);
        border-radius: 20px;
        background: transparent;
        color: var(--wgt-text);
        font-size: 14px;
        outline: none;
      }
      .wgt-input::placeholder {
        color: var(--wgt-text-muted);
      }
      .wgt-send-btn {
        width: 38px;
        height: 38px;
        border-radius: 50%;
        border: none;
        display: grid;
        place-items: center;
        background: var(--wgt-primary);
        color: white;
        cursor: pointer;
        flex-shrink: 0;
        opacity: 0.5;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WidgetPreviewComponent {
  readonly formState = input<Partial<WidgetInstance>>({});
}
