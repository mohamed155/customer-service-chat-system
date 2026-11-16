import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { TranslocoPipe, provideTranslocoScope } from '@jsverse/transloco';
import { WidgetInstance } from '../../../core/api/widget.models';
import { DEFAULT_WIDGET_COLOR } from './widget-defaults';

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
        --wgt-surface: var(--app-wgt-light-surface);
        --wgt-text: var(--app-wgt-light-text);
        --wgt-text-muted: var(--app-wgt-light-text-muted);
        --wgt-border: var(--app-wgt-light-border);
        --wgt-bubble-visitor: var(--app-wgt-bubble-visitor);
        --wgt-bubble-assistant: var(--app-wgt-light-bubble-assistant);
        --wgt-radius: 12px;
        --wgt-space: 8px;
        --wgt-font: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        --wgt-shadow: var(--app-wgt-shadow);
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
        --wgt-surface: var(--app-wgt-dark-surface);
        --wgt-text: var(--app-wgt-dark-text);
        --wgt-text-muted: var(--app-wgt-light-text-muted);
        --wgt-border: var(--app-wgt-dark-border);
        --wgt-bubble-assistant: var(--app-wgt-dark-bubble-assistant);
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
        color: var(--app-wgt-on-primary);
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
        color: var(--app-wgt-on-primary);
      }
      .wgt-header-name {
        font-weight: 600;
        font-size: 15px;
      }
      .wgt-close {
        background: none;
        border: none;
        color: var(--app-wgt-on-primary-muted);
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
        color: var(--app-wgt-on-primary);
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

  /** The widget's brand color: the one the tenant chose, else the default a new widget starts with. */
  protected readonly brandColor = computed(
    () => this.formState().primaryColor || DEFAULT_WIDGET_COLOR,
  );
}
