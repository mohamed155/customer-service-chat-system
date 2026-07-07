import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-escalation-banner',
  imports: [TuiIcon],
  template: `
    <div class="icon"><tui-icon icon="@tui.triangle-alert" /></div>
    <div class="copy">
      <strong>{{ title() }}</strong>
      <p>{{ description() }}</p>
    </div>
    <button type="button" aria-label="Dismiss alert" (click)="dismissed.emit()">
      <tui-icon icon="@tui.x" />
    </button>
  `,
  styles: [
    `
      :host {
        display: flex;
        align-items: flex-start;
        gap: var(--app-space-3);
        padding: var(--app-space-4);
        border: 1px solid color-mix(in srgb, var(--app-amber) 24%, transparent);
        border-radius: var(--app-radius-lg);
        background: var(--app-amber-soft);
        color: var(--app-text);
      }
      .icon {
        width: 34px;
        height: 34px;
        display: grid;
        place-items: center;
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-amber);
      }
      .copy {
        flex: 1;
        min-width: 0;
      }
      strong {
        display: block;
        font-size: var(--app-font-base);
      }
      p {
        margin: 4px 0 0;
        color: var(--app-text-2);
        font-size: var(--app-font-sm);
      }
      button {
        width: 30px;
        height: 30px;
        display: grid;
        place-items: center;
        border: 0;
        border-radius: var(--app-radius-sm);
        background: transparent;
        color: var(--app-text-2);
        cursor: pointer;
      }
      button:hover {
        background: var(--app-panel);
        color: var(--app-text);
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EscalationBannerComponent {
  readonly title = input.required<string>();
  readonly description = input.required<string>();
  readonly dismissed = output<void>();
}
