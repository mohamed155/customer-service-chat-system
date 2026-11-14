import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { WidgetInstance } from '../../../core/api/widget.models';

@Component({
  selector: 'app-widget-preview',
  standalone: true,
  imports: [TranslocoPipe],
  providers: [provideTranslocoScope('integrations')],
  templateUrl: './widget-preview.component.html',
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
  /** The sign on a close button: a symbol, the same in every language. */
  protected readonly closeGlyph = '\u00d7';

  readonly formState = input<Partial<WidgetInstance>>({});
}
