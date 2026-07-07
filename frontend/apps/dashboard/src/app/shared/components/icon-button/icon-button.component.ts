import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { TuiIcon } from '@taiga-ui/core';

@Component({
  selector: 'app-icon-button',
  imports: [TuiIcon],
  host: { '[class.active]': 'active()' },
  template: `
    <button type="button" [attr.aria-label]="label()">
      <tui-icon [icon]="icon()" />
    </button>
  `,
  styles: [
    `
      :host {
        display: inline-flex;
      }
      button {
        width: 38px;
        height: 38px;
        display: inline-grid;
        place-items: center;
        border: 1px solid var(--app-border);
        border-radius: var(--app-radius-md);
        background: var(--app-panel);
        color: var(--app-text-2);
        cursor: pointer;
        transition:
          background var(--app-transition-fast),
          border-color var(--app-transition-fast),
          color var(--app-transition-fast);
      }
      button:hover,
      :host(.active) button {
        background: var(--app-panel-2);
        border-color: var(--app-border-strong);
        color: var(--app-text);
      }
      button:focus-visible {
        outline: 3px solid var(--app-accent-soft);
        outline-offset: 2px;
        border-color: var(--app-accent);
      }
      tui-icon {
        font-size: 17px;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IconButtonComponent {
  readonly icon = input.required<string>();
  readonly label = input.required<string>();
  readonly active = input(false);
}
